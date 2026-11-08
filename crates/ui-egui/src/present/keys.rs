//! Presenter-owned keyboard/controller holds. No source edit or new owner inherits an old edge.

use std::sync::Arc;

use egui::{Context, Event, Key};
use figcraft_doc::proto::{KeyMods, Trigger, gamepad_button};
use figcraft_doc::{Document, Node, NodeId};

use super::Presenter;
use super::actions::find_trigger;

const MAX_HELD_INPUTS: usize = 128;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Input {
    Keyboard(Key),
    Pad(u8),
}

impl Input {
    fn name(self) -> Option<&'static str> {
        match self {
            Self::Keyboard(key) => Some(key.name()),
            Self::Pad(index) => gamepad_button(usize::from(index)),
        }
    }
}

#[derive(Clone)]
struct Binding {
    node: Arc<Node>,
    index: usize,
    root: NodeId,
    base: NodeId,
    phase: Phase,
    minimum_hold_ms: u32,
    overlay_owner: Option<Arc<()>>,
}

#[derive(Clone, Copy)]
enum Phase {
    Legacy,
    Down,
    Up,
}

impl Binding {
    fn valid(&self, player: &Presenter) -> bool {
        let same_owner = match &self.overlay_owner {
            Some(owner) => player.overlays.iter().any(|overlay| overlay.id == self.root && Arc::ptr_eq(&overlay.key_owner, owner)),
            None => self.root == self.base,
        };
        if player.current != self.base || !same_owner {
            return false;
        }
        let Some(current) = player.doc.as_ref().and_then(|doc| doc.node_arc(self.node.id)) else { return false };
        // A layout/variable change can clone a node without changing its binding. A different
        // interaction (including actions) must not inherit the original input's authority.
        Arc::ptr_eq(current, &self.node) || current.interactions.get(self.index) == self.node.interactions.get(self.index)
    }
}

struct Held {
    input: Input,
    started: f64,
    down: Option<Binding>,
    up: Option<Binding>,
    fired: bool,
}

#[derive(Default)]
pub(super) struct KeyInputs {
    held: Vec<Held>,
    pad_edges: Vec<(u8, bool)>,
    pad_mask: u32,
    pad_ready: bool,
    clock: Option<f64>,
    frame: Option<(egui::ViewportId, u64)>,
}

impl KeyInputs {
    pub(super) fn clear(&mut self) {
        self.held.clear();
        self.pad_edges.clear();
        self.pad_ready = false;
        self.clock = None;
        self.frame = None;
    }

    /// Separate from legacy rising-edge pulses: a button held before focus/ownership is adopted
    /// must not become a new timed gesture when the prototype first sees it.
    pub(super) fn poll_pads(&mut self, held: u32) {
        if self.pad_ready {
            let changed = held ^ self.pad_mask;
            for index in 0..17 {
                if changed & (1 << index) != 0 && self.pad_edges.len() < 2 * MAX_HELD_INPUTS {
                    self.pad_edges.push((index, held & (1 << index) != 0));
                }
            }
        }
        self.pad_mask = held;
        self.pad_ready = true;
    }

    pub(super) fn next_due(&self, now: f64) -> Option<f64> {
        if !now.is_finite() {
            return None;
        }
        self.held
            .iter()
            .filter(|held| !held.fired)
            // Use elapsed time like the firing predicate. Adding a subsecond delay to a large
            // finite clock can round back to the start and otherwise request immediate wakes.
            .filter_map(|held| held.down.as_ref().map(|binding| (f64::from(binding.minimum_hold_ms) / 1000.0 - (now - held.started)).max(0.0)))
            .reduce(f64::min)
    }
}

#[derive(Default)]
pub(super) struct Outcome {
    pub fired: bool,
    pub claimed: Vec<Key>,
}

pub(super) struct Scope {
    pub keyboard: bool,
    pub controller: bool,
}

fn find(doc: &Document, root: NodeId, base: NodeId, name: &str, mods: KeyMods, up: bool, overlay_owner: Option<Arc<()>>) -> Option<Binding> {
    doc.subtree(root).into_iter().find_map(|id| {
        let index = find_trigger(doc, id, |trigger| {
            trigger.matches_key(name, mods)
                && if up { matches!(trigger, Trigger::KeyUp { .. }) } else { matches!(trigger, Trigger::KeyPress { .. } | Trigger::KeyDown { .. }) }
        })?;
        let node = doc.node_arc(id)?.clone();
        let trigger = &node.interactions.get(index)?.trigger;
        let phase = match trigger {
            Trigger::KeyPress { .. } => Phase::Legacy,
            Trigger::KeyDown { .. } => Phase::Down,
            Trigger::KeyUp { .. } => Phase::Up,
            _ => return None,
        };
        let minimum_hold_ms = trigger.minimum_hold_ms();
        Some(Binding { node, index, root, base, phase, minimum_hold_ms, overlay_owner: overlay_owner.clone() })
    })
}

fn fire(player: &mut Presenter, binding: &Binding, now: f64, fresh: bool) -> bool {
    binding.valid(player) && player.fire_from(binding.node.id, binding.index, None, now, fresh)
}

/// True claims this edge for new phase handling. Legacy-only key presses keep their old path.
fn down(player: &mut Presenter, doc: &Document, root: NodeId, input: Input, mods: KeyMods, now: f64) -> (bool, bool) {
    if player.key_inputs.held.iter().any(|held| held.input == input) {
        return (true, false);
    }
    let Some(name) = input.name() else { return (false, false) };
    let owner = player.overlays.last().filter(|overlay| overlay.id == root).map(|overlay| overlay.key_owner.clone());
    let binding = find(doc, root, player.current, name, mods, false, owner.clone());
    let up = find(doc, root, player.current, name, mods, true, owner);
    let new_down = binding.as_ref().is_some_and(|binding| matches!(binding.phase, Phase::Down));
    if !new_down && up.is_none() {
        return (false, false);
    }
    if player.key_inputs.held.len() >= MAX_HELD_INPUTS {
        return (true, false);
    }
    let immediate = binding.as_ref().is_some_and(|binding| binding.minimum_hold_ms == 0);
    player.key_inputs.held.push(Held { input, started: now, down: binding.clone(), up, fired: immediate });
    let fired = immediate && binding.as_ref().is_some_and(|binding| fire(player, binding, now, true));
    (true, fired)
}

fn up(player: &mut Presenter, input: Input, now: f64) -> (bool, bool) {
    let Some(index) = player.key_inputs.held.iter().position(|held| held.input == input) else { return (false, false) };
    let held = player.key_inputs.held.remove(index);
    let binding = held.up.filter(|binding| now - held.started >= f64::from(binding.minimum_hold_ms) / 1000.0);
    let fired = binding.is_some_and(|binding| {
        let fresh = binding.minimum_hold_ms == 0;
        fire(player, &binding, now, fresh)
    });
    (true, fired)
}

fn enter_frame(player: &mut Presenter, ctx: &Context, allowed: bool, now: f64) -> bool {
    if !allowed || !now.is_finite() || player.key_inputs.clock.is_some_and(|previous| now < previous) {
        player.key_inputs.clear();
        return false;
    }
    let frame = (ctx.viewport_id(), ctx.cumulative_frame_nr());
    if player.key_inputs.frame.is_some_and(|previous| previous.0 != frame.0 || frame.1 > previous.1.saturating_add(1)) {
        // A prototype tab/viewport that was not taking input cannot resume a hold acquired
        // before it was hidden, even if the physical key remained down in the other owner.
        player.key_inputs.clear();
        // The focused poll already adopted this frame's physical mask. Keep it armed so the
        // next genuinely new edge is accepted, rather than discarding a second fresh press.
        player.key_inputs.pad_ready = true;
    }
    player.key_inputs.frame = Some(frame);
    player.key_inputs.clock = Some(now);
    let mut retained = std::mem::take(&mut player.key_inputs.held);
    retained.retain(|held| held.down.as_ref().or(held.up.as_ref()).is_some_and(|binding| binding.valid(player)));
    player.key_inputs.held = retained;
    true
}

fn process_pads(player: &mut Presenter, doc: &Document, root: NodeId, now: f64, pads: &mut Vec<String>, outcome: &mut Outcome) {
    // A real controller edge can pair with a legacy down binding. Remove its rising pulse only
    // when that binding was already fired here, so the old fire_pads path cannot run it twice.
    for (index, pressed) in std::mem::take(&mut player.key_inputs.pad_edges) {
        let (claimed, fired) = if pressed {
            if outcome.fired {
                continue;
            }
            down(player, doc, root, Input::Pad(index), KeyMods::default(), now)
        } else {
            up(player, Input::Pad(index), now)
        };
        outcome.fired |= fired;
        if claimed && let Some(name) = gamepad_button(usize::from(index)) {
            pads.retain(|pad| pad != name);
        }
    }
}

fn process_keyboard(player: &mut Presenter, ctx: &Context, doc: &Document, root: NodeId, now: f64, outcome: &mut Outcome) {
    let events: Vec<_> = ctx.input(|input| {
        input
            .events
            .iter()
            .filter_map(|event| match event {
                Event::Key { key, pressed, repeat, modifiers, .. } => Some((*key, *pressed, *repeat, *modifiers)),
                _ => None,
            })
            .take(2 * MAX_HELD_INPUTS)
            .collect()
    });
    for (key, pressed, repeat, modifiers) in events {
        let tracked = player.key_inputs.held.iter().any(|held| held.input == Input::Keyboard(key));
        let (claimed, fired) = if !pressed {
            up(player, Input::Keyboard(key), now)
        } else if repeat || outcome.fired {
            (tracked, false)
        } else {
            let mods = KeyMods { ctrl: modifiers.ctrl, alt: modifiers.alt, shift: modifiers.shift, cmd: modifiers.mac_cmd };
            down(player, doc, root, Input::Keyboard(key), mods, now)
        };
        outcome.fired |= fired;
        if claimed && !outcome.claimed.contains(&key) {
            outcome.claimed.push(key);
        }
    }
}

fn retire_released(player: &mut Presenter, ctx: &Context) {
    // Missing release events (focus/pointer dispatch changed) cannot leave a pending hold alive.
    let pad_mask = player.key_inputs.pad_mask;
    player.key_inputs.held.retain(|held| match held.input {
        Input::Keyboard(key) => ctx.input(|input| input.key_down(key)),
        Input::Pad(index) => pad_mask & (1 << index) != 0,
    });
}

fn fire_due(player: &mut Presenter, now: f64, outcome: &mut Outcome) {
    for _ in 0..MAX_HELD_INPUTS {
        if outcome.fired {
            break;
        }
        let due = player.key_inputs.held.iter().enumerate().find_map(|(index, held)| {
            let binding = held.down.as_ref().filter(|_| !held.fired)?;
            (now - held.started >= f64::from(binding.minimum_hold_ms) / 1000.0).then(|| (index, binding.clone()))
        });
        let Some((index, binding)) = due else { break };
        if let Some(held) = player.key_inputs.held.get_mut(index) {
            held.fired = true;
        }
        if fire(player, &binding, now, false) {
            outcome.fired = true;
            break;
        }
    }
}

/// Claim only inputs with a new down/up binding, keeping unrelated legacy dispatch unchanged.
pub(super) fn process(
    player: &mut Presenter,
    ctx: &Context,
    doc: &Document,
    root: NodeId,
    scope: Scope,
    now: f64,
    pads: &mut Vec<String>,
) -> Outcome {
    let mut outcome = Outcome::default();
    if !enter_frame(player, ctx, (scope.keyboard || scope.controller) && ctx.input(|input| input.focused), now) {
        return outcome;
    }
    player.key_inputs.held.retain(|held| match held.input {
        Input::Keyboard(_) => scope.keyboard,
        Input::Pad(_) => scope.controller,
    });
    if scope.controller {
        process_pads(player, doc, root, now, pads, &mut outcome);
    } else {
        player.key_inputs.pad_edges.clear();
    }
    if scope.keyboard {
        process_keyboard(player, ctx, doc, root, now, &mut outcome);
    }
    retire_released(player, ctx);
    fire_due(player, now, &mut outcome);
    if !outcome.claimed.is_empty() {
        ctx.input_mut(|input| input.events.retain(|event| !matches!(event, Event::Key { key, .. } if outcome.claimed.contains(key))));
    }
    outcome
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prototype_key_timing_large_finite_clock_does_not_schedule_an_immediate_not_yet_due_hold() {
        let mut doc = Document::new("Original finite-clock hold fixture");
        let page = doc.first_page().unwrap();
        let node = doc.create(page, figcraft_doc::NodeKind::Rectangle, "Key owner", 0.0, 0.0, 10.0, 10.0).unwrap();
        doc.get_mut(node).unwrap().interactions.push(
            serde_json::from_value(serde_json::json!({
                "trigger":{"type":"keyDown","key":"A","ms":500},"actions":[{"type":"back"}]
            }))
            .unwrap(),
        );
        let binding = find(&doc, page, page, "A", KeyMods::default(), false, None).unwrap();
        let now = 2.0_f64.powi(53);
        let input = KeyInputs {
            held: vec![Held { input: Input::Keyboard(Key::A), started: now, down: Some(binding), up: None, fired: false }],
            ..Default::default()
        };
        assert_eq!(input.next_due(now), Some(0.5), "the hold has not elapsed merely because the clock is large");
        assert_eq!(input.next_due(now + 2.0), Some(0.0));
        assert!(input.next_due(f64::INFINITY).is_none());
    }

    #[test]
    fn prototype_key_timing_backwards_or_nonfinite_clock_cancels_pending_state() {
        let ctx = Context::default();
        for now in [9.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let mut player = Presenter::new(NodeId(1));
            player.key_inputs.clock = Some(10.0);
            player.key_inputs.held.push(Held { input: Input::Keyboard(Key::A), started: 10.0, down: None, up: None, fired: false });
            player.key_inputs.pad_edges.push((0, true));
            assert!(!enter_frame(&mut player, &ctx, true, now));
            assert!(player.key_inputs.clock.is_none());
            assert!(player.key_inputs.pad_edges.is_empty());
            assert!(player.key_inputs.held.is_empty());
        }
    }
}
