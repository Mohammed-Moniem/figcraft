use egui::{Pos2, Ui};

use super::{FigApp, PointerKind, send};
use figcraft_engine::ToolId;

/// What a finger press turned out to mean, latched for the rest of the press (a pan must not
/// turn back into a click because the finger later paused).
#[derive(Clone, Copy, Debug, PartialEq)]
enum Kind {
    /// Not settled yet: still a candidate for a click or a pan.
    Undecided,
    /// The finger moved past the click distance before the long-press: the view pans.
    Panning,
    /// The long-press fired: the press belongs to the menu.
    Menu,
    /// The active document/page/tool changed while the withheld press was still down.
    Cancelled,
}

/// A finger press on the canvas and what it means, latched in temp data.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Press {
    /// Where the finger went down.
    origin: Pos2,
    /// egui's start time of *this* press, so a latch never outlives it.
    started: f64,
    /// What the press means.
    kind: Kind,
    document: u64,
    page: figcraft_doc::NodeId,
    tool: ToolId,
}

fn press_id() -> egui::Id {
    egui::Id::unique("figcraft-canvas-touch-press")
}

type Owner = (u64, figcraft_doc::NodeId, ToolId);

#[derive(Clone, Copy)]
struct Navigation {
    owner: Owner,
    cancelled: bool,
}

const MAX_CONTACTS: usize = 32;

#[derive(Clone, Default)]
struct Contacts {
    frame: Option<u64>,
    active: Vec<(egui::TouchDeviceId, egui::TouchId)>,
    overlap: bool,
    overflowed: bool,
    origin: Option<Pos2>,
}

fn contacts_id() -> egui::Id {
    egui::Id::unique("figcraft-canvas-touch-contacts")
}

/// Observe every raw contact before prototype/handle arbitration can return early. The peak
/// count preserves overlap even when a second finger also ends in the same input frame.
pub(super) fn observe_contacts(ctx: &egui::Context) {
    let frame = ctx.cumulative_frame_nr();
    let mut contacts = ctx.data(|d| d.get_temp::<Contacts>(contacts_id())).unwrap_or_default();
    if contacts.frame == Some(frame) {
        return;
    }
    contacts.frame = Some(frame);
    contacts.overlap = contacts.overflowed || contacts.active.len() > 1;
    contacts.origin = None;
    ctx.input(|input| {
        for event in &input.events {
            if let egui::Event::Touch { device_id, id, phase, pos, .. } = event {
                let contact = (*device_id, *id);
                match phase {
                    egui::TouchPhase::Start => {
                        contacts.origin.get_or_insert(*pos);
                        if !contacts.active.contains(&contact) {
                            if contacts.active.len() < MAX_CONTACTS {
                                contacts.active.push(contact);
                            } else {
                                contacts.overflowed = true;
                            }
                        }
                        contacts.overlap |= contacts.overflowed || contacts.active.len() > 1;
                    }
                    egui::TouchPhase::End | egui::TouchPhase::Cancel => contacts.active.retain(|id| *id != contact),
                    egui::TouchPhase::Move => {}
                }
            }
        }
        // End/Cancel may have been processed while Home hid the canvas observer. Egui's
        // authoritative zero-contact state releases stale/saturated IDs; preserve only
        // this frame's overlap so a fully batched gesture is still consumed once.
        if !input.any_touches() {
            contacts.active.clear();
            contacts.overflowed = false;
        }
    });
    ctx.data_mut(|d| d.insert_temp(contacts_id(), contacts));
}

/// Only an already claimed press/navigation gets precedence over prototype first-press input.
pub(super) fn owns_gesture(ctx: &egui::Context) -> bool {
    ctx.data(|d| d.get_temp::<Navigation>(navigation_id()).is_some() || d.get_temp::<Option<Press>>(press_id()).flatten().is_some())
}

fn navigation_id() -> egui::Id {
    egui::Id::unique("figcraft-canvas-touch-navigation")
}

/// What the caller does with the frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Verdict {
    /// No arbitrated press: the canvas's ordinary pointer paths run, as on the desktop.
    Unhandled,
    /// A background multi-contact gesture owns pointer input, but camera pan/pinch still runs.
    Navigation,
    /// The finger's press is the canvas's: do nothing else this frame.
    Swallow,
    /// Let the ordinary paths run (the long-press frame's secondary click, or a tap's double
    /// click), with the press already replayed as the tool expects.
    FallThrough,
}

/// Arbitrate a press on the canvas: see the module comment.
pub(super) fn input(app: &mut FigApp, ui: &Ui, resp: &egui::Response) -> Verdict {
    let ctx = ui.ctx().clone();
    let contacts = ctx.data(|d| d.get_temp::<Contacts>(contacts_id()));
    let observed_overlap = contacts.as_ref().is_some_and(|c| c.overlap);
    let observed_origin = contacts.and_then(|c| c.origin);
    let (touch, down, start, origin, pos, multi) = ctx.input(|i| {
        (
            i.any_touches(),
            i.pointer.any_down(),
            i.pointer.press_start_time(),
            i.pointer.press_origin().or(observed_origin),
            i.pointer.interact_pos().or(i.pointer.hover_pos()),
            i.multi_touch().is_some() || observed_overlap,
        )
    });
    let latched = ctx.data(|d| d.get_temp::<Option<Press>>(press_id())).flatten();
    let owner = app.session.ds().ok().map(|doc| (doc.uid, doc.page, app.session.tool.tool));
    if let Some(mut navigation) = ctx.data(|d| d.get_temp::<Navigation>(navigation_id())) {
        // Ownership changes cancel this whole contact episode, even if the original owner
        // returns before the last finger lifts. Pointer start/down flags cannot revive it.
        navigation.cancelled |= owner != Some(navigation.owner);
        clear(&ctx);
        ctx.data_mut(|d| {
            if touch {
                d.insert_temp(navigation_id(), navigation);
            } else {
                d.remove::<Navigation>(navigation_id());
            }
        });
        return if navigation.cancelled { Verdict::Swallow } else { Verdict::Navigation };
    }
    if multi {
        // Claim only a withheld background press (or simultaneous starts on background).
        // A layer/handle-owned first press keeps its existing tool release path.
        let background = latched.is_some()
            || (!app.session.tool.busy()
                && !app.canvas.dragging
                && app.session.ds().is_ok_and(|d| d.interaction.is_none())
                && matches!(app.session.tool.tool, ToolId::Move | ToolId::Scale)
                && origin.is_some_and(|at| {
                    if !resp.rect.contains(at) {
                        return false;
                    }
                    send(app, PointerKind::Hover, at, &ctx.input(|i| i.modifiers));
                    app.session.ds().is_ok_and(|d| d.hover.is_none())
                }));
        clear(&ctx);
        if background && let Some(held_owner) = latched.map(|p| (p.document, p.page, p.tool)).or(owner) {
            let cancelled = owner != Some(held_owner) || latched.is_some_and(|p| p.kind == Kind::Cancelled);
            // A fully batched Start/End episode is consumed now; it cannot own the next press.
            if touch {
                ctx.data_mut(|d| d.insert_temp(navigation_id(), Navigation { owner: held_owner, cancelled }));
            }
            return if cancelled { Verdict::Swallow } else { Verdict::Navigation };
        }
        return Verdict::Unhandled;
    }
    // The latch belongs to the press it was made for: each press has its own start time. On
    // the lift egui has already cleared the start time, so the last press's latch is the
    // one the finger is letting go of.
    let press = match (latched, start, down) {
        (Some(p), Some(t), _) if p.started == t => Some(p),
        (Some(p), None, false) => Some(p),
        (Some(_), ..) => {
            clear(&ctx);
            None
        }
        (None, ..) => None,
    };
    let Some(press) = press else {
        let started_now = touch
            && down
            && resp.hovered()
            && ctx.input(|i| i.pointer.button_pressed(egui::PointerButton::Primary))
            && matches!(app.session.tool.tool, ToolId::Move | ToolId::Scale);
        let (Some(o), Some(t)) = (origin, start) else { return Verdict::Unhandled };
        if !started_now {
            return Verdict::Unhandled;
        }
        // Background or a layer? The engine's own hover answers (it is the tool's hit test
        // at the same point): a layer keeps the press, so a finger can still move or resize
        // it the way a mouse does.
        send(app, PointerKind::Hover, o, &ctx.input(|i| i.modifiers));
        if !app.session.ds().is_ok_and(|d| d.hover.is_none()) {
            return Verdict::Unhandled;
        }
        // The press is the canvas's: close what a mouse press would close, take the keyboard
        // from panel fields, and hold the Down until the finger says what it means.
        app.ui.context_menu = None;
        app.ui.tool_flyout = None;
        app.ui.layers_focus = false;
        app.ui.pages_focus = None;
        app.ui.key_box = None;
        ctx.memory_mut(|m| {
            if let Some(f) = m.focused() {
                m.surrender_focus(f);
            }
        });
        let Some((document, page, tool)) = owner else { return Verdict::Unhandled };
        ctx.data_mut(|d| d.insert_temp(press_id(), Some(Press { origin: o, started: t, kind: Kind::Undecided, document, page, tool })));
        return Verdict::Swallow;
    };
    if press.kind == Kind::Cancelled || owner != Some((press.document, press.page, press.tool)) {
        // Do not replay a delayed Down/Up into another tab or page, including one opened by
        // a provider response. Keep swallowing this gesture until its own release.
        if down {
            ctx.data_mut(|d| d.insert_temp(press_id(), Some(Press { kind: Kind::Cancelled, ..press })));
        } else {
            clear(&ctx);
        }
        return Verdict::Swallow;
    }
    let mods = ctx.input(|i| i.modifiers);
    if !down {
        // The finger lifted: the arbitration is over.
        clear(&ctx);
        return match press.kind {
            Kind::Undecided => {
                // A tap: replay the Down the press swallowed, so the tool's click paths act
                // on the pair as they do for a mouse. A double or triple tap lets the
                // canvas's own branch send its event; the Down still goes first.
                send(app, PointerKind::Down, press.origin, &mods);
                if resp.double_clicked() || resp.triple_clicked() {
                    Verdict::FallThrough
                } else {
                    let at = pos.unwrap_or(press.origin);
                    send(app, PointerKind::Up, at, &mods);
                    Verdict::FallThrough
                }
            }
            // A pan or a menu: nothing more to say.
            Kind::Panning | Kind::Menu | Kind::Cancelled => Verdict::Swallow,
        };
    }
    let Some(pos) = pos else {
        // A `PointerGone` while held (a touch release is such a frame): keep waiting.
        return Verdict::Swallow;
    };
    // Settle the press once: the finger decides what it meant by moving, against the
    // long-press boundary. A still hold past egui's long-press belongs to the menu.
    let click_dist = ctx.options(|o| o.input_options.max_click_dist);
    let kind = match press.kind {
        Kind::Undecided if press.origin.distance(pos) > click_dist => Kind::Panning,
        Kind::Undecided if resp.long_touched() => Kind::Menu,
        settled => settled,
    };
    if kind != press.kind {
        ctx.data_mut(|d| d.insert_temp(press_id(), Some(Press { kind, ..press })));
    }
    match kind {
        Kind::Panning => {
            pan(app, &ctx);
            Verdict::Swallow
        }
        // The menu's own frame is egui's secondary click: let the canvas open the menu over
        // the selection it kept.
        Kind::Menu if resp.secondary_clicked() => Verdict::FallThrough,
        Kind::Menu | Kind::Undecided | Kind::Cancelled => Verdict::Swallow,
    }
}

/// Move the view under the finger by this frame's pointer delta.
fn pan(app: &mut FigApp, ctx: &egui::Context) {
    let delta = ctx.input(|i| i.pointer.delta());
    if delta == egui::Vec2::ZERO {
        return;
    }
    if let Ok(ds) = app.session.ds_mut() {
        let z = ds.view.zoom.max(1e-6);
        ds.view.cx -= f64::from(delta.x) / z;
        ds.view.cy -= f64::from(delta.y) / z;
        ds.revision += 1;
    }
}

fn clear(ctx: &egui::Context) {
    ctx.data_mut(|d| d.remove::<Option<Press>>(press_id()));
}
