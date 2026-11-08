//! Auto-pan: a drag that holds the pointer near the canvas's edge, or past it, pans the view
//! toward it, steadily while the pointer stays still (#2015). The rule lives in
//! [`figcraft_engine::ViewState::autopan_velocity`]; this applies it once a frame.

use egui::{Context, Pos2};

use crate::FigApp;

/// The longest frame the pan counts, so a stall doesn't jump the view.
const MAX_DT: f64 = 0.1;

/// Pan the view for this frame with a drag's pointer at screen point `pos`. True when it panned,
/// so the caller feeds the engine the pointer again: the dragged layer, box, shape or connection
/// follows the document point now under it. Asks for another frame while the pointer is in the
/// zone, so the pan goes on when nothing else moves.
pub(crate) fn pan(app: &mut FigApp, ctx: &Context, pos: Pos2) -> bool {
    let dt = f64::from(ctx.input(|i| i.unstable_dt)).clamp(0.0, MAX_DT);
    let min = app.canvas.rect.min;
    let (px, py) = (f64::from(pos.x - min.x), f64::from(pos.y - min.y));
    let Ok(ds) = app.session.ds_mut() else { return false };
    let (vx, vy) = figcraft_engine::ViewState::autopan_velocity(px, py, ds.view.vw, ds.view.vh);
    if vx == 0.0 && vy == 0.0 {
        return false;
    }
    // In the zone: the next frame pans too, even if the pointer never moves.
    ctx.request_repaint();
    // A discarded egui pass sees the same frame; side effects must run only once.
    let frame = ctx.cumulative_frame_nr();
    let key = egui::Id::unique("canvas_autopan_frame").with((ds.uid, ds.page, ctx.viewport_id()));
    if ctx.data(|d| d.get_temp::<u64>(key)) == Some(frame) {
        return false;
    }
    if !ds.view.autopan(px, py, dt) {
        return false;
    }
    ctx.data_mut(|d| d.insert_temp(key, frame));
    ds.revision += 1;
    true
}

/// The drag under way pans at the edge: a primary drag of the Move or a drawing tool that
/// [`figcraft_engine::tools::ToolState::autopans`] allows, with no text being edited.
pub(super) fn wanted(app: &FigApp, ctx: &Context) -> bool {
    let Ok(ds) = app.session.ds() else { return false };
    app.canvas.dragging
        && ds.text_edit.is_none()
        && ds.vector_edit.is_none()
        && ctx.input(|i| i.focused && i.pointer.primary_down())
        && app.session.tool.autopans(ds.view.zoom)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::NoServices;
    use figcraft_engine::Session;

    fn app() -> FigApp {
        let mut app = FigApp::new(Session::new(), Box::new(NoServices));
        app.canvas.rect = egui::Rect::from_min_size(Pos2::ZERO, egui::vec2(1000.0, 600.0));
        let v = &mut app.session.ds_mut().unwrap().view;
        (v.vw, v.vh, v.zoom, v.cx, v.cy) = (1000.0, 600.0, 1.0, 0.0, 0.0);
        app
    }

    #[test]
    fn one_positive_dt_pass_pans_each_interleaved_owner_only_once() {
        let ctx = Context::default();
        let (mut a, mut b) = (app(), app());
        assert_ne!(a.session.ds().unwrap().uid, b.session.ds().unwrap().uid);
        let mut output = ctx.run_ui(egui::RawInput { time: Some(1.0), ..Default::default() }, |ui| {
            let ctx = ui.ctx();
            assert!(ctx.input(|i| i.unstable_dt) > 0.0, "same actual pass has positive frame time");
            let at = egui::pos2(998.0, 300.0);
            assert!(pan(&mut a, ctx, at));
            let once = a.session.ds().unwrap().view.cx;
            assert!(once > 0.0);
            assert!(!pan(&mut a, ctx, at), "same owner cannot move twice in one pass");
            assert_eq!(a.session.ds().unwrap().view.cx, once);
            assert!(pan(&mut b, ctx, at), "a different document owns its own pan");
            assert!(!pan(&mut a, ctx, at), "interleaving another owner cannot erase the first marker");
            assert_eq!(a.session.ds().unwrap().view.cx, once);
        });
        output.textures_delta.clear();
    }
}
