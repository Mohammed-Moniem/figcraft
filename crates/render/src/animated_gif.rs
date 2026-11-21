//! Bounded timeline export. The camera is fixed to the selected frame's unanimated bounds.
//! GIF has a 256-colour palette per frame, binary alpha and centisecond timing; these losses
//! are reported instead of implying that this is an editable document or lossless animation.

use std::collections::HashSet;
use std::io::{self, Write};

use figcraft_doc::anim::motion::{MotionPlayer, TimelineSettings};
use figcraft_doc::{Document, NodeId, NodeKind};
use image::codecs::gif::{GifEncoder, Repeat};
use image::{Delay, Frame, RgbaImage};

pub const MAX_FRAMES: u32 = 1200;
pub const MAX_FRAME_PIXELS: u64 = 16_777_216;
pub const MAX_RENDER_PIXELS: u64 = 1_000_000_000;
pub const MAX_NODE_FRAMES: u64 = 20_000_000;
pub const MAX_OUTPUT_BYTES: usize = 64 * 1024 * 1024;
/// Below Document::subtree's traversal ceiling; the player cannot silently truncate the page.
pub const MAX_MOTION_NODES: usize = 1_000_000;

#[derive(Clone, Copy, Debug)]
pub struct Options {
    pub duration_ms: u32,
    pub fps: u32,
    pub looping: bool,
    pub scale: f64,
}

impl From<TimelineSettings> for Options {
    fn from(value: TimelineSettings) -> Self {
        Self { duration_ms: value.duration_ms, fps: value.fps, looping: value.looping, scale: 1.0 }
    }
}

#[derive(Debug)]
pub struct Output {
    pub bytes: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub frames: u32,
    pub duration_ms: u32,
    pub warnings: Vec<String>,
}

/// Cumulative rounding prevents the 60 fps → 10 ms truncation that shortens a GIF by 40%.
fn end_cs(index: u32, options: Options) -> u32 {
    let numerator = (u64::from(index) * 1000).min(u64::from(options.duration_ms) * u64::from(options.fps));
    ((numerator + 5 * u64::from(options.fps)) / (10 * u64::from(options.fps))) as u32
}

struct BoundedWriter {
    bytes: Vec<u8>,
    limit: usize,
    failed: bool,
}

impl Write for BoundedWriter {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        if data.len() > self.limit.saturating_sub(self.bytes.len()) || self.bytes.try_reserve(data.len()).is_err() {
            self.failed = true;
            return Err(io::Error::other("animated GIF exceeds the output byte budget"));
        }
        self.bytes.extend_from_slice(data);
        Ok(data.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// MotionPlayer visits hidden descendants too. Validate that exact tree before it builds its ID
/// list: a visible-only render preflight cannot bound a hidden cycle or duplicate child links.
/// One iterator per ancestor bounds the stack even when an invalid node repeats many children.
fn check_motion_tree(doc: &Document, page: NodeId, limit: usize) -> Result<(), String> {
    let mut seen = HashSet::new();
    let mut stack = vec![(std::slice::from_ref(&page).iter(), 0usize, None)];
    while let Some((siblings, depth, parent)) = stack.last_mut() {
        let Some(&id) = siblings.next() else {
            stack.pop();
            continue;
        };
        let (depth, parent) = (*depth, *parent);
        if depth > crate::MAX_EXPORT_DEPTH {
            return Err(format!("motion page exceeds the maximum export depth of {} (including hidden layers)", crate::MAX_EXPORT_DEPTH));
        }
        if seen.contains(&id) {
            return Err("motion page has cyclic or repeated child references (including hidden layers)".into());
        }
        if seen.len() >= limit {
            return Err(format!("motion page exceeds {limit} nodes; reduce the page size"));
        }
        let node = doc.node(id).ok_or("motion page refers to a missing layer")?;
        if node.parent != parent || node.is_page() != (depth == 0) {
            return Err("motion page has inconsistent parent links".into());
        }
        seen.try_reserve(1).map_err(|_| "could not allocate motion traversal")?;
        seen.insert(id);
        if !node.children.is_empty() {
            stack.push((node.children.iter(), depth + 1, Some(id)));
        }
    }
    Ok(())
}

/// Export a top-level frame without changing the base document. Frames are streamed to the
/// encoder rather than retained in memory. Every rendering budget is checked before cloning or
/// allocating a framebuffer; the writer separately caps compressed data, including the trailer.
pub fn export(doc: &Document, frame: NodeId, options: Options) -> Result<Output, String> {
    export_with_limit(doc, frame, options, MAX_OUTPUT_BYTES)
}

fn export_with_limit(doc: &Document, frame: NodeId, options: Options, limit: usize) -> Result<Output, String> {
    if !(10..=TimelineSettings::MAX_DURATION_MS).contains(&options.duration_ms) {
        return Err("GIF duration must be between 10 and 600000 ms".into());
    }
    if !(1..=100).contains(&options.fps) {
        return Err("GIF frame rate must be between 1 and 100 fps (10 ms timing resolution)".into());
    }
    if !options.scale.is_finite() || !(0.01..=64.0).contains(&options.scale) {
        return Err("GIF scale must be finite and between 0.01 and 64".into());
    }
    let node = doc.node(frame).ok_or("export frame does not exist")?;
    let page = node.parent.filter(|p| doc.pages.contains(p)).ok_or("select a top-level frame to export animation")?;
    if !matches!(node.kind, NodeKind::Frame) || !node.visible {
        return Err("select a visible top-level frame to export animation".into());
    }
    let frames = (u64::from(options.duration_ms) * u64::from(options.fps)).div_ceil(1000) as u32;
    if frames > MAX_FRAMES {
        return Err(format!("GIF exceeds {MAX_FRAMES} frames; reduce duration or frame rate"));
    }
    let node_frames = (doc.node_count() as u64).checked_mul(u64::from(frames)).ok_or("GIF work budget overflow")?;
    if node_frames > MAX_NODE_FRAMES {
        return Err(format!("GIF exceeds {MAX_NODE_FRAMES} node-frames; reduce duration or document size"));
    }
    check_motion_tree(doc, page, MAX_MOTION_NODES)?;
    let mut previous = 0;
    for i in 1..=frames {
        let end = end_cs(i, options);
        if end <= previous {
            return Err("GIF duration produces a frame shorter than 10 ms; adjust duration or frame rate".into());
        }
        previous = end;
    }
    let area = doc.world_bounds(frame).ok_or("export frame has no bounds")?;
    let width = (area.width() * options.scale - 1e-6).ceil();
    let height = (area.height() * options.scale - 1e-6).ceil();
    if !area.x0.is_finite()
        || !area.y0.is_finite()
        || !width.is_finite()
        || !height.is_finite()
        || !(1.0..=16_384.0).contains(&width)
        || !(1.0..=16_384.0).contains(&height)
    {
        return Err("GIF dimensions must be finite, positive and at most 16384 pixels per side".into());
    }
    let (width, height) = (width as u32, height as u32);
    let pixels = u64::from(width) * u64::from(height);
    if pixels > MAX_FRAME_PIXELS || pixels * u64::from(frames) > MAX_RENDER_PIXELS {
        return Err(format!("GIF exceeds the pixel budget ({MAX_FRAME_PIXELS} per frame, {MAX_RENDER_PIXELS} total); reduce scale or duration"));
    }
    let mut writer = BoundedWriter { bytes: Vec::new(), limit, failed: false };
    let mut player = MotionPlayer::new(doc, page);
    {
        // The pinned image encoder restores the background between whole frames, so transparent
        // pixels in the next frame cannot leave a trail from the preceding frame.
        let mut encoder = GifEncoder::new_with_speed(&mut writer, 10);
        if options.looping {
            encoder.set_repeat(Repeat::Infinite).map_err(|e| e.to_string())?;
        }
        let mut previous = 0;
        for i in 0..frames {
            let time_ms = f64::from(i) * 1000.0 / f64::from(options.fps);
            let evaluated = player.seek(doc, time_ms);
            let mut pix = crate::area_at(evaluated, &[frame], area, options.scale, time_ms / 1000.0).ok_or("could not render GIF frame")?;
            crate::to_srgb(&mut pix, doc.color_profile);
            let mut rgba = crate::to_rgba8(&pix);
            for [r, g, b, a] in rgba.as_chunks_mut::<4>().0 {
                if *a < 128 {
                    *r = 0;
                    *g = 0;
                    *b = 0;
                    *a = 0;
                } else {
                    *a = 255;
                }
            }
            let image = RgbaImage::from_raw(width, height, rgba).ok_or("could not allocate GIF frame")?;
            let end = end_cs(i + 1, options);
            let delay = Delay::from_numer_denom_ms((end - previous) * 10, 1);
            encoder.encode_frame(Frame::from_parts(image, 0, 0, delay)).map_err(|e| format!("could not encode GIF: {e}"))?;
            previous = end;
        }
        // Drop writes the trailer. BoundedWriter records failures even when the encoder's Drop
        // implementation cannot return them, so an incomplete stream is never returned as success.
    }
    if writer.failed || writer.bytes.last() != Some(&0x3b) {
        return Err("could not finish GIF within the output byte budget".into());
    }
    Ok(Output {
        bytes: writer.bytes,
        width,
        height,
        frames,
        duration_ms: previous * 10,
        warnings: vec![
            "GIF quantizes each frame to 256 colours and sRGB; alpha below 50% becomes transparent, other alpha becomes opaque.".into(),
            "GIF delays are rounded cumulatively to 10 ms; playback timing may vary between viewers.".into(),
        ],
    })
}

#[cfg(test)]
#[path = "animated_gif_tests.rs"]
mod tests;
