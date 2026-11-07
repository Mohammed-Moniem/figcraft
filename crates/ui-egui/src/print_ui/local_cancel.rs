//! Cancellation of this dialog's own CUPS receipt, never an arbitrary or foreign queue job.

use crate::i18n::{tr, trf};
use craft_print_ui::printing::Feedback;
use std::sync::{
    Arc,
    mpsc::{Receiver, TryRecvError},
};
use std::time::{Duration, Instant};

type Cancel = Arc<dyn Fn(&str) -> Result<(), String> + Send + Sync>;

pub(super) struct State {
    printer: String,
    receipt: Option<String>,
    job: Option<String>,
    pending: bool,
    worker: Option<Receiver<Result<(), String>>>,
    note: Option<String>,
    run: Cancel,
}

impl Default for State {
    fn default() -> Self {
        Self { printer: String::new(), receipt: None, job: None, pending: false, worker: None, note: None, run: Arc::new(cancel) }
    }
}

/// CUPS ids are `<queue>-<positive job number>`. The receipt must name the queue this job used;
/// output text cannot choose another queue or inject a command option.
fn own_id(receipt: &str, printer: &str) -> Option<String> {
    let id = receipt.trim().strip_prefix("request id is ")?.split_whitespace().next()?;
    if id.len() > 256 || id.starts_with('-') || !id.bytes().all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b)) {
        return None;
    }
    let (queue, number) = id.rsplit_once('-')?;
    if queue != printer || !number.bytes().all(|b| b.is_ascii_digit()) || number.parse::<u64>().ok()? == 0 {
        return None;
    }
    Some(id.to_owned())
}

/// Only called after an explicit Cancel press, and only with an id parsed from this dialog's
/// own receipt. Bound the helper wait; an uncertain response is never called a cancellation.
fn cancel(id: &str) -> Result<(), String> {
    use std::process::{Command, Stdio};
    let mut child = Command::new("cancel")
        .arg(id)
        .env("LC_ALL", "C")
        .env("LANG", "C")
        .env("SOFTWARE", "FigCraft")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let output = child.wait_with_output().map_err(|e| e.to_string())?;
                return if status.success() { Ok(()) } else { Err(String::from_utf8_lossy(&output.stderr).chars().take(4096).collect()) };
            }
            Ok(None) if start.elapsed() < Duration::from_secs(5) => std::thread::sleep(Duration::from_millis(10)),
            result => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(match result {
                    Err(e) => e.to_string(),
                    _ => tr("The cancellation response timed out; check the print queue.").to_owned(),
                });
            }
        }
    }
}

impl State {
    pub(super) fn message(&self) -> Option<&str> {
        self.note.as_deref()
    }
    pub(super) fn begin(&mut self, printer: String) {
        self.printer = printer;
        self.receipt = None;
        self.job = None;
        self.pending = false;
        self.note = None;
    }
    pub(super) fn busy(&self) -> bool {
        self.pending || self.worker.is_some()
    }
    fn request(&mut self) {
        if self.worker.is_some() {
            return;
        }
        let Some(id) = self.job.clone() else {
            if self.receipt.is_some() {
                return;
            }
            self.pending = true;
            self.note = Some(tr("Cancellation will be requested after the queue accepts the handover.").to_owned());
            return;
        };
        self.pending = false;
        let (tx, rx) = std::sync::mpsc::channel();
        let run = self.run.clone();
        match std::thread::Builder::new().name("print-cancel".into()).spawn(move || {
            let _ = tx.send(run(&id));
        }) {
            Ok(worker) => {
                self.worker = Some(rx);
                self.note = Some(tr("Requesting cancellation…").to_owned());
                let mut workers = super::JOBS.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
                workers.retain(|j| !j.is_finished());
                workers.push(worker);
            }
            Err(e) => self.note = Some(trf("Cancellation was not confirmed: {error}", &[("error", &e)])),
        }
    }
    pub(super) fn poll(&mut self, feedback: &Option<Feedback>, ctx: &egui::Context) {
        if let Some(Feedback::Submitted(receipt)) = feedback {
            if self.receipt.as_ref() != Some(receipt) {
                self.receipt = Some(receipt.clone());
                self.job = own_id(receipt, &self.printer);
                if self.pending {
                    if self.job.is_some() {
                        self.request();
                    } else {
                        self.pending = false;
                        self.note = Some(tr("The queue did not return a cancellable job id; check the print queue.").to_owned());
                    }
                }
            }
        } else if matches!(feedback, Some(Feedback::Failed(_) | Feedback::WorkerStopped)) && self.pending {
            self.pending = false;
            self.note = Some(tr("The handover failed; cancellation could not be confirmed. Check the print queue.").to_owned());
        }
        if let Some(rx) = self.worker.as_ref() {
            match rx.try_recv() {
                Ok(Ok(())) => {
                    self.worker = None;
                    self.job = None;
                    self.note = Some(tr("Cancellation requested. Some pages may already have printed.").to_owned());
                }
                Ok(Err(error)) => {
                    self.worker = None;
                    self.note = Some(trf("Cancellation was not confirmed: {error}", &[("error", &error)]));
                }
                Err(TryRecvError::Disconnected) => {
                    self.worker = None;
                    self.note = Some(tr("The cancellation worker stopped; check the print queue.").to_owned());
                }
                Err(TryRecvError::Empty) => {}
            }
        }
        if self.busy() {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
    }
    pub(super) fn show(&mut self, ui: &mut egui::Ui, sending: bool) {
        if (sending || self.job.is_some()) && ui.add_enabled(!self.busy(), egui::Button::new(tr("Cancel print job"))).clicked() {
            self.request();
        }
        if let Some(note) = &self.note {
            ui.label(note);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    #[test]
    fn print_cancel_only_accepts_the_exact_owned_queue_receipt() {
        assert_eq!(own_id("request id is Office-42 (1 file(s))", "Office"), Some("Office-42".into()));
        for receipt in [
            "request id is Other-42",
            "request id is Office-0",
            "request id is -a-42",
            "request id is Office-42;rm",
            "request id is Office-x",
            "Done",
        ] {
            assert!(own_id(receipt, "Office").is_none(), "{receipt}");
        }
    }
    #[test]
    fn print_cancel_waits_for_handover_and_never_submits_or_cancels_twice() {
        let got = Arc::new(Mutex::new(Vec::new()));
        let observed = got.clone();
        let mut state = State {
            run: Arc::new(move |id| {
                observed.lock().unwrap().push(id.to_owned());
                Ok(())
            }),
            ..Default::default()
        };
        state.begin("Office".into());
        state.request();
        assert!(got.lock().unwrap().is_empty());
        assert!(state.pending);
        let feedback = Some(Feedback::Submitted("request id is Office-42 (1 file(s))".into()));
        let ctx = egui::Context::default();
        state.poll(&feedback, &ctx);
        state.request();
        for _ in 0..100 {
            state.poll(&feedback, &ctx);
            if !state.busy() {
                break;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(*got.lock().unwrap(), vec!["Office-42"]);
        assert!(!state.busy());
        assert!(state.job.is_none());
        state.poll(&feedback, &ctx);
        assert!(state.job.is_none(), "a repeated receipt cannot resurrect a cancelled job");
        assert!(state.note.as_ref().unwrap().contains("may already have printed"));
    }
    #[test]
    fn print_cancel_reports_failure_and_never_acts_on_foreign_or_unknown_receipts() {
        let mut state = State { run: Arc::new(|_| Err("synthetic refusal".into())), ..Default::default() };
        state.begin("Office".into());
        state.request();
        state.poll(&Some(Feedback::Submitted("request id is Other-42".into())), &egui::Context::default());
        assert!(!state.busy());
        assert!(state.job.is_none());
        assert!(state.note.as_ref().unwrap().contains("check the print queue"));
        state.begin("Office".into());
        state.poll(&Some(Feedback::Submitted("request id is Office-42".into())), &egui::Context::default());
        state.request();
        for _ in 0..100 {
            state.poll(&None, &egui::Context::default());
            if !state.busy() {
                break;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(state.note.as_ref().unwrap().contains("synthetic refusal"));
        assert!(state.job.is_some(), "failed cancellation remains retryable, never called successful");
    }
}
