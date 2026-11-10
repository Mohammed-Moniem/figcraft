mod touch;
use super::{FigApp, Point, PointerKind, Pos2, Rect, Ui, json, pointer_event};

/// What a hover was found in: the event (where, the modifiers, the zoom) and what decides what it
/// hits (the file and page, their revision, the selection, the tool and modes). The same again finds
/// the same, so it isn't sent again.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct HoverKey {
    event: figcraft_engine::tools::PointerEvent,
    doc: Option<(u64, figcraft_engine::doc::NodeId, u64)>,
    selection: Vec<figcraft_engine::doc::NodeId>,
    tool: figcraft_engine::ToolId,
    modes: [bool; 3],
}

impl HoverKey {
    fn of(app: &FigApp, event: figcraft_engine::tools::PointerEvent) -> Self {
        let ds = app.session.ds().ok();
        HoverKey {
            event,
            doc: ds.map(|d| (d.uid, d.page, d.revision)),
            selection: ds.map(|d| d.selection.clone()).unwrap_or_default(),
            tool: app.session.tool.tool,
            modes: [app.session.tool.dev_mode, ds.is_some_and(|d| d.vector_edit.is_some()), ds.is_some_and(|d| d.text_edit.is_some())],
        }
    }
}

/// The pointer left the canvas (or something over it took the pointer): nothing is hovered there.
fn left(app: &mut FigApp) {
    app.canvas.last_hover = None;
    super::locked::clear_hover(app);
    app.session.pointer_left();
}

/// The zoom tool while Z is held (Alt: zoom out): a click zooms a step about the pointer, a drag
/// zooms to the dragged area. Takes the pointer from the other tools while it's active.
fn zoom_tool(app: &mut FigApp, ui: &Ui, resp: &egui::Response, pos: Pos2, mods: egui::Modifiers, typing: bool) -> bool {
    let held = !typing && !mods.command && ui.input(|i| i.key_down(egui::Key::Z));
    app.canvas.zoom_tool = held.then_some(mods.alt);
    let Some(start) = app.canvas.zoom_drag.or_else(|| (held && resp.hovered() && ui.input(|i| i.pointer.primary_pressed())).then_some(pos)) else {
        return held;
    };
    app.canvas.zoom_drag = Some(start);
    let area = Rect::from_two_pos(start, pos);
    if ui.input(|i| i.pointer.primary_down()) {
        return true;
    }
    app.canvas.zoom_drag = None;
    let (a, b) = (app.to_page(area.min), app.to_page(area.max));
    let params = if area.width() > 4.0 && area.height() > 4.0 {
        json!({"x": a.x, "y": a.y, "width": b.x - a.x, "height": b.y - a.y})
    } else {
        let p = app.to_page(pos);
        json!({"x": p.x, "y": p.y, "out": mods.alt})
    };
    app.run("view.zoomAt", params);
    true
}

/// The wheel's scroll and the zoom for this frame. With Preferences ▸ Scroll wheel zoom on, a
/// mouse wheel zooms and Cmd/Ctrl+wheel scrolls ([`swap_wheel`]); Shift+wheel still scrolls
/// sideways, and trackpads, pinches and touch screens are left as they are.
fn wheel_zoom(app: &mut FigApp, ctx: &egui::Context, zoom: f32, shift: bool) -> (egui::Vec2, f32) {
    let (wheel, latest, pinch) = ctx.input(|i| {
        let latest = i.events.iter().rev().find_map(|e| match e {
            egui::Event::MouseWheel { unit, source, .. } => Some(is_mouse_wheel(*source, *unit)),
            _ => None,
        });
        let pinch = i.multi_touch().is_some() || i.events.iter().any(|e| matches!(e, egui::Event::Zoom(_)));
        (i.smooth_scroll_delta, latest, pinch)
    });
    let speed = ctx.options(|o| o.input_options.scroll_zoom_speed);
    // egui spreads a wheel step over a few frames: the kind holds until the next scroll event.
    if let Some(mouse) = latest {
        app.canvas.wheel_is_mouse = mouse;
    }
    if !app.session.prefs.scroll_wheel_zoom || !app.canvas.wheel_is_mouse || pinch {
        return (wheel, zoom);
    }
    swap_wheel(wheel, zoom, shift, speed)
}

/// Whether a scroll event came from a mouse wheel. Only macOS says which device scrolled; elsewhere
/// a wheel scrolls by lines and a trackpad by points, which is the best guess there is.
pub(crate) fn is_mouse_wheel(source: egui::MouseWheelSource, unit: egui::MouseWheelUnit) -> bool {
    match source {
        egui::MouseWheelSource::Wheel => true,
        egui::MouseWheelSource::Unknown => unit != egui::MouseWheelUnit::Point,
        _ => false,
    }
}

/// Scroll wheel zoom on a mouse wheel's frame: egui reads Cmd/Ctrl+wheel as `zoom`, which becomes
/// a scroll down (across with Shift: `ViewState::scroll`), and a plain
/// `wheel` scroll becomes a zoom at egui's own Cmd/Ctrl+wheel rate (`speed`). Shift+wheel is
/// left a sideways scroll.
pub(crate) fn swap_wheel(wheel: egui::Vec2, zoom: f32, shift: bool, speed: f32) -> (egui::Vec2, f32) {
    if zoom > 0.0 && (zoom - 1.0).abs() > 1e-6 && speed > 0.0 {
        return (egui::vec2(0.0, zoom.ln() / speed), 1.0);
    }
    if !shift && wheel != egui::Vec2::ZERO {
        return (egui::Vec2::ZERO, (speed * (wheel.x + wheel.y)).exp());
    }
    (wheel, zoom)
}

fn import_dropped(app: &mut FigApp, ctx: &egui::Context, hover_pos: Option<egui::Pos2>) {
    let dropped = ctx.input(|i| i.raw.dropped_files.clone());
    let mut refused = false;
    // The first image goes to the colour picker that shows an image; the rest to the canvas.
    let mut well = crate::colorpicker::image_drop_target(app, ctx).map(|(_, params)| params);
    for f in dropped {
        let path = f.path().to_path_buf();
        if crate::is_document(&path) {
            crate::pending_open::open_dropped(app, &f);
            continue;
        }
        if refused || crate::menus::dev_mode_refuses(app, "file.import") {
            refused = true;
            continue;
        }
        let pt = hover_pos.map(|h| app.to_page(h)).unwrap_or_default();
        let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "dropped.png".into());
        let (command, mut params) = match well.take() {
            Some(params) => ("image.replace", params),
            None => ("file.import", json!({"name": name, "x": pt.x, "y": pt.y})),
        };
        let read = if path.exists() {
            params["path"] = json!(path.to_string_lossy());
            params.as_object_mut().map(|o| o.remove("name"));
            None
        } else {
            Some(crate::dropped_bytes(&f, &name))
        };
        let Some(target) =
            app.session.ds().ok().map(|doc| crate::pending_open::ImportTarget { uid: doc.uid, page: doc.page, name: doc.doc.name.clone() })
        else {
            continue;
        };
        app.ui.pending_opens.push(crate::pending_open::PendingOpen { label: name, params, command, target: Some(target), read });
    }
}

fn touch_navigation_frame_id() -> egui::Id {
    egui::Id::unique("canvas_touch_navigation_frame")
}

/// A claimed navigation episode owns even its final release before late canvas buttons paint.
pub(super) fn touch_navigating(ctx: &egui::Context) -> bool {
    ctx.data(|d| d.get_temp::<(egui::ViewportId, u64)>(touch_navigation_frame_id())) == Some((ctx.viewport_id(), ctx.cumulative_frame_nr()))
}

/// Prototype gestures and withheld background touches run before ordinary tool input.
fn handled_gesture(app: &mut FigApp, ui: &Ui, resp: &egui::Response) -> touch::Verdict {
    touch::observe_contacts(ui.ctx());
    let continuation = if touch::owns_gesture(ui.ctx()) {
        let verdict = touch::input(app, ui, resp);
        if matches!(verdict, touch::Verdict::Navigation | touch::Verdict::Swallow) {
            return verdict;
        }
        Some(verdict)
    } else {
        None
    };
    if crate::proto_canvas::input(app, ui, resp) {
        // A prototype gesture is a press on the canvas too: the Pages list gives up the keyboard.
        app.ui.pages_focus = None;
        return touch::Verdict::Swallow;
    }
    continuation.unwrap_or_else(|| touch::input(app, ui, resp))
}

/// Leaving vector edit consumes the double-click, but its press still needs the actual release.
fn finish_vector_exit(app: &mut FigApp, ctx: &egui::Context, pos: Pos2, mods: &egui::Modifiers, was_editing: bool) {
    if was_editing
        && app.session.ds().is_ok_and(|d| d.vector_edit.is_none())
        && app.session.tool.busy()
        && ctx.input(|i| i.pointer.button_released(egui::PointerButton::Primary))
    {
        send(app, PointerKind::Up, pos, mods);
    }
}

/// Apply the existing pointer-anchored zoom or scroll pan before tool input.
fn navigate_view(app: &mut FigApp, resp: &egui::Response, pos: Pos2, mods: egui::Modifiers, scroll: egui::Vec2, zoom_delta: f32) {
    if resp.hovered() {
        if (zoom_delta - 1.0).abs() > 1e-4 {
            let anchor = app.to_page(pos);
            if let Ok(ds) = app.session.ds_mut() {
                let z = ds.view.zoom * f64::from(zoom_delta);
                ds.view.set_zoom_at(z, (anchor.x, anchor.y));
                ds.revision += 1;
            }
        } else if scroll != egui::Vec2::ZERO
            && let Ok(ds) = app.session.ds_mut()
        {
            ds.view.scroll(f64::from(scroll.x), f64::from(scroll.y), mods.shift);
            ds.revision += 1;
        }
    }
}

/// The pointer and keys on the canvas. False when a comment pin, guide or handle owns its cursor.
fn pan_held_drag(app: &mut FigApp, ctx: &egui::Context, mods: &egui::Modifiers) {
    // A drag held past the edge pans toward it, the layer or box going with the pointer.
    if super::autopan::wanted(app, ctx)
        && let Some(at) = ctx.input(|i| i.pointer.latest_pos())
        && super::autopan::pan(app, ctx, at)
    {
        send(app, PointerKind::Move, at, mods);
    }
}

/// Apply the existing middle-button or space camera pan at its original input phase.
fn pan_view(app: &mut FigApp, ctx: &egui::Context, resp: &egui::Response, pos: Pos2) -> bool {
    // Middle-button / space panning.
    let middle_down = ctx.input(|i| i.pointer.button_down(egui::PointerButton::Middle));
    if (middle_down || (app.canvas.space_pan && ctx.input(|i| i.pointer.primary_down()))) && resp.hovered() || app.canvas.middle_pan.is_some() {
        if let Ok(ds) = app.session.ds_mut() {
            match app.canvas.middle_pan {
                None => app.canvas.middle_pan = Some((pos, ds.view.cx, ds.view.cy)),
                Some((p0, cx, cy)) => {
                    let z = ds.view.zoom.max(1e-6);
                    ds.view.cx = cx - f64::from(pos.x - p0.x) / z;
                    ds.view.cy = cy - f64::from(pos.y - p0.y) / z;
                    ds.revision += 1;
                }
            }
        }
        if !middle_down && !ctx.input(|i| i.pointer.primary_down()) {
            app.canvas.middle_pan = None;
        }
        return true;
    }
    false
}

/// Retire the captured press before it can pan or send a Move into another document.
fn retire_foreign_drag(app: &mut FigApp) -> bool {
    let Some(document) = app.canvas.drag_document else { return false };
    if app.session.ds().is_ok_and(|ds| ds.uid == document) {
        return false;
    }
    app.canvas.dragging = false;
    app.canvas.drag_document = None;
    app.canvas.pressure = None;
    app.canvas.last_pointer = None;
    app.canvas.last_hover = None;
    if let Err(error) = app.session.drop_gesture_in_document(document) {
        app.toast(error.to_string());
    }
    true
}

pub(super) fn input(app: &mut FigApp, ui: &mut Ui, resp: &egui::Response) -> bool {
    if retire_foreign_drag(app) {
        return true;
    }
    let gesture = handled_gesture(app, ui, resp);
    if matches!(gesture, touch::Verdict::Swallow) {
        return true;
    }
    let navigating = matches!(gesture, touch::Verdict::Navigation);
    let ctx = ui.ctx().clone();
    if navigating {
        let frame = (ctx.viewport_id(), ctx.cumulative_frame_nr());
        ctx.data_mut(|d| d.insert_temp(touch_navigation_frame_id(), frame));
    }
    // Two fingers pan as well as pinch (a touch screen has no scroll wheel).
    let (mods, pan, zoom_delta, hover_pos, interact_pos, space) = ctx.input(|i| {
        let pan = i.multi_touch().map_or(egui::Vec2::ZERO, |m| m.translation_delta);
        (i.modifiers, pan, i.zoom_delta(), i.pointer.hover_pos(), i.pointer.interact_pos(), i.key_down(egui::Key::Space))
    });
    let (wheel, zoom_delta) = wheel_zoom(app, &ctx, zoom_delta, mods.shift);
    let scroll = wheel + pan;
    let editing_text = app.session.ds().map(|d| d.text_edit.is_some()).unwrap_or(false);
    let typing_elsewhere = crate::a11y::field_focused(&ctx);
    app.canvas.space_pan = space && !editing_text && !typing_elsewhere;
    if !navigating && !app.canvas.dragging && crate::selection_spacing::input(app, &ctx, hover_pos, resp.hovered(), app.canvas.space_pan) {
        return false;
    }
    // Dropped files import onto the canvas, behind a card while they do ([`crate::pending_open`]);
    // a dropped document opens as a file of its own, as on the home screen. Dev Mode imports
    // nothing into the design, and says so once for the whole drop.
    import_dropped(app, &ctx, hover_pos);
    // Touch End also sends PointerGone: hover is already absent while this response still
    // carries its click. egui retains the interaction position for this release frame.
    // A press accepted by the canvas owns its drag until release (#2023), including sidebars.
    let Some(pos) = interact_pos.filter(|p| p.is_finite() && (app.canvas.dragging || app.canvas.rect.contains(*p))) else {
        // A drag released off the canvas, or off the window, ends where the pointer was last seen.
        if app.canvas.dragging && !ctx.input(|i| i.pointer.primary_down()) {
            app.canvas.dragging = false;
            if let Some(at) = interact_pos.or_else(|| app.canvas.last_pointer.map(|p| app.to_screen(p.x, p.y))) {
                send(app, PointerKind::Up, at, &mods);
            }
        }
        pan_held_drag(app, &ctx, &mods);
        // Leaving the canvas: nothing is hovered there any more.
        if !app.canvas.dragging && std::mem::take(&mut app.canvas.hovering) {
            left(app);
        }
        // A pan whose button was released off the canvas is over.
        if !ctx.input(|i| i.pointer.button_down(egui::PointerButton::Middle) || i.pointer.primary_down()) {
            app.canvas.middle_pan = None;
        }
        // So is a handle drag.
        crate::handles::end_released_drag(app, &ctx);
        return true;
    };
    // A visible lock button owns its press/release; it must not start a canvas gesture below it.
    if !navigating && super::locked::blocks_pointer(app, ui, pos) {
        return false;
    }
    // A press on the canvas gives the keyboard back to it (arrows nudge again).
    if resp.hovered() && ctx.input(|i| i.pointer.any_pressed()) {
        app.ui.layers_focus = false;
        app.ui.pages_focus = None;
        app.ui.key_box = None;
    }
    // Zoom (pinch, Cmd/Ctrl+scroll; a plain mouse wheel with Scroll wheel zoom) about the pointer;
    // other scrolls pan.
    navigate_view(app, resp, pos, mods, scroll, zoom_delta);
    // Background multi-contact ownership permits only the existing camera block above.
    // Its duplicate PrimaryDown/Up and click flags never reach tools, handles or menus.
    if navigating {
        return true;
    }
    if zoom_tool(app, ui, resp, pos, mods, editing_text || typing_elsewhere) {
        return true;
    }
    if pan_view(app, &ctx, resp, pos) {
        return true;
    }
    // A handle drag whose button went up ends first, wherever on the canvas the pointer is: over a
    // ruler or a comment pin, which take the pointer below, or over something drawn above the
    // canvas (the colour picker, a card). The handle had the press, so it has the release too.
    if crate::handles::end_released_drag(app, &ctx) {
        return false;
    }
    if !app.canvas.dragging && (resp.hovered() || app.ui.comments.dragging()) && crate::comments::input(app, &ctx, pos) {
        return false;
    }
    if !app.canvas.dragging && resp.hovered() && crate::guides::input(app, &ctx, pos) {
        return false;
    }
    if !app.canvas.dragging && resp.hovered() && crate::handles::input(app, &ctx, pos) {
        return false;
    }
    if resp.secondary_clicked() {
        open_context_menu(app, pos, mods.command);
        return true;
    }
    // A third quick click is counted as a triple, not a double (a click elsewhere in between still
    // leaves its time in the sequence): it means the same here — edit, enter, open — unless a
    // vector or text is being edited, where a triple already has its own meaning. A triple on
    // empty canvas still means a double in vector edit mode: the double-click that entered edit
    // mode counts on, so leaving it right away would otherwise take a fourth click or a pause.
    let ds = app.session.ds().ok();
    let editing = ds.is_some_and(|d| d.vector_edit.is_some() || d.text_edit.is_some());
    let vector_editing = ds.is_some_and(|d| d.vector_edit.is_some());
    // Only a triple click asks (it walks the parts' render bounds).
    let empty_canvas = || {
        ds.is_some_and(|d| {
            let Some(ve) = d.vector_edit.as_ref().filter(|v| v.anchor.is_none() && d.text_edit.is_none()) else {
                return false;
            };
            let p = app.to_page(pos);
            let tol = 6.0 / d.view.zoom.max(1e-6);
            ve.parts().iter().all(|part| d.doc.world_render_bounds(part.id).is_none_or(|b| !b.inflate(tol * 4.0, tol * 4.0).contains(p)))
        })
    };
    if resp.double_clicked() || (resp.triple_clicked() && (!editing || empty_canvas())) {
        let ev = pointer_event(app, PointerKind::DoubleClick, pos, &mods);
        if let Err(e) = app.session.pointer(&ev) {
            app.toast(e.to_string());
        }
        // Leaving vector edit handles the double-click but does not consume its press's
        // vector gesture. Deliver the actual release after the mode change, without replaying
        // ordinary widget clicks or changing double-clicks that stay in an editing mode.
        finish_vector_exit(app, &ctx, pos, &mods, vector_editing);
        app.canvas.dragging = false;
        return true;
    }
    let primary_pressed = ctx.input(|i| i.pointer.button_pressed(egui::PointerButton::Primary));
    let primary_down = ctx.input(|i| i.pointer.primary_down());
    let primary_released = ctx.input(|i| i.pointer.button_released(egui::PointerButton::Primary));
    // The press and the release carry their own positions: both in one frame (a quick click or a
    // slow frame) would otherwise send the press from the hover position the pointer ended at.
    let (press_pos, release_pos) = ctx.input(|i| {
        let (mut press, mut release) = (None, None);
        for e in &i.events {
            if let egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, .. } = e {
                if *pressed {
                    press = Some(*pos);
                } else {
                    release = Some(*pos);
                }
            }
        }
        (press, release)
    });
    let kind = if primary_pressed && resp.hovered() {
        app.canvas.dragging = true;
        app.ui.context_menu = None;
        app.ui.tool_flyout = None;
        // Clicking the canvas takes focus from panel fields.
        ctx.memory_mut(|m| {
            if let Some(f) = m.focused() {
                m.surrender_focus(f);
            }
        });
        Some(PointerKind::Down)
    } else if app.canvas.dragging && primary_released {
        app.canvas.dragging = false;
        Some(PointerKind::Up)
    } else if app.canvas.dragging && primary_down {
        Some(PointerKind::Move)
    } else if resp.hovered() {
        app.canvas.hovering = true;
        Some(PointerKind::Hover)
    } else {
        // Something over the canvas took the pointer.
        if std::mem::take(&mut app.canvas.hovering) {
            left(app);
        }
        None
    };
    // Pen pressure comes with touch events (a pen on Windows, a stylus in a browser); a mouse,
    // macOS and Linux tablets report none and draw at the stroke weight.
    let force = ctx.input(|i| {
        i.events.iter().rev().find_map(|e| match e {
            egui::Event::Touch { force: Some(f), .. } if f.is_finite() && *f > 0.0 => Some(f.clamp(0.0, 1.0)),
            _ => None,
        })
    });
    if force.is_some() {
        app.canvas.pressure = force;
    }
    // A drag held near the edge pans before the move goes out, so it lands on the document point
    // now under the pointer (and the pan goes on while the pointer is still: it asks for frames).
    if kind == Some(PointerKind::Move) && super::autopan::wanted(app, &ctx) {
        super::autopan::pan(app, &ctx, pos);
    }
    if let Some(k) = kind {
        let at = if matches!(k, PointerKind::Down) { press_pos.unwrap_or(pos) } else { pos };
        send(app, k, at, &mods);
        // The pointer also let go within this frame (a quick click, or a slow frame): the tools get
        // the release too, so the press's gesture ends — a quick click with a shape tool still
        // draws and returns to Move, and a double-click right after drawing finds the Move tool.
        if matches!(k, PointerKind::Down) && primary_released {
            app.canvas.dragging = false;
            send(app, PointerKind::Up, release_pos.unwrap_or(pos), &mods);
        }
    }
    true
}

/// Pointer event `kind` at screen point `pos`, to the tools.
fn send(app: &mut FigApp, kind: PointerKind, pos: Pos2, m: &egui::Modifiers) {
    let ev = pointer_event(app, kind, pos, m);
    // A hover where nothing changed since the last one finds what that one found (the state is
    // taken after it ran: what it set, such as the hovered layer, is part of it).
    let hover = kind == PointerKind::Hover;
    if hover && app.canvas.last_hover.as_ref().is_some_and(|k| *k == HoverKey::of(app, ev.clone())) {
        return;
    }
    if kind == PointerKind::Down {
        app.canvas.drag_document = app.session.ds().ok().map(|ds| ds.uid);
    } else if kind == PointerKind::Up {
        app.canvas.drag_document = None;
        app.canvas.pressure = None;
    }
    app.canvas.last_pointer = Some(Point::new(ev.x, ev.y));
    if let Err(e) = app.session.pointer(&ev) {
        app.toast(e.to_string());
    }
    if hover {
        super::locked::hover(app, pos);
    }
    app.canvas.last_hover = hover.then(|| HoverKey::of(app, ev));
    app.handle_ui_requests();
}

/// Right-click at screen `pos`: select what's under the pointer (`deep`: the nested layer), then
/// open the context menu there. A frame's name label counts as the frame, as on a press, so a
/// label drawn over its section picks the frame, not the section behind it. A layer that is
/// already selected keeps the whole selection.
fn misspelled_at(app: &mut FigApp, position: Pos2) -> Option<crate::SpellingContext> {
    if !app.session.prefs.spell_check || app.preedit_view().is_some() {
        return None;
    }
    let ds = app.session.ds().ok()?;
    let id = ds.text_edit.as_ref()?.id;
    let p = app.to_page(position);
    if !ds.doc.world_bounds(id).is_some_and(|b| b.contains(p)) {
        return None;
    }
    let index = app.session.text_index_at(id, p).ok()?;
    let uid = ds.uid;
    let doc = ds.doc.clone();
    let r = app.session.execute("text.spellingSuggestions", &json!({"index": index})).ok()?;
    if r["misspelled"] != true {
        return None;
    }
    let suggestions = r["suggestions"].as_array()?.iter().filter_map(|v| v.as_str().map(str::to_string)).collect();
    Some(crate::SpellingContext { position, document_uid: uid, layer: id, snapshot: doc, index, word: r["word"].as_str()?.to_string(), suggestions })
}

pub fn open_context_menu(app: &mut FigApp, pos: Pos2, deep: bool) {
    if super::locked::open_context(app, pos) {
        return;
    }
    let p = app.to_page(pos);
    app.ui.context_spelling = misspelled_at(app, pos);
    if let Ok(ds) = app.session.ds() {
        let (doc, page, sel, zoom) = (ds.doc.clone(), ds.page, ds.selection.clone(), ds.view.zoom.max(1e-6));
        // A frame's name label is the frame, as on a press, even where it sits over a section.
        let label = figcraft_engine::tools::label_at(&doc, page, &app.session.tool.labels, p);
        let hit = app.session.hit(&doc, page, p, 2.0 / zoom);
        let target = label.or_else(|| hit.map(|leaf| figcraft_doc::geometry::select_target(&doc, leaf, &sel, deep)));
        if let Some(target) = target
            && !sel.contains(&target)
        {
            app.session.set_selection(vec![target]);
        }
    }
    app.ui.context_menu = Some((pos, app.session.selection()));
    app.ui.context_menu_shown = false;
}
