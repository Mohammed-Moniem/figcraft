use super::{Arc, BTreeMap, Deserialize, Document, NodeId, Serialize, Value, cmd, live, online, plugins};

/// The smallest and largest interface size, relative to the system's display scale (1.0 is the
/// system's own size).
pub const UI_SCALE_MIN: f64 = 0.5;

pub const UI_SCALE_MAX: f64 = 3.0;

/// Default localhost port for the desktop app's control and MCP server (`127.0.0.1:47800`).
pub const DEFAULT_MCP_PORT: u16 = 47800;

/// `scale` kept between [`UI_SCALE_MIN`] and [`UI_SCALE_MAX`]; not a finite number: `None`.
/// The interface sizes Preferences offers, relative to the system's scale (the keys step through
/// them; `pref.uiScale` takes any size up to [`UI_SCALE_MAX`]).
pub const UI_SCALE_SIZES: [f64; 9] = [0.5, 0.75, 0.9, 1.0, 1.1, 1.25, 1.5, 1.75, 2.0];

pub fn clamp_ui_scale(scale: f64) -> Option<f64> {
    scale.is_finite().then(|| scale.clamp(UI_SCALE_MIN, UI_SCALE_MAX))
}

/// Reads the saved interface size. Anything but a number reads as 1 (the system's size) and a
/// number out of range is clamped, so a settings file edited by hand never costs the other
/// preferences.
fn read_ui_scale<'de, D: serde::Deserializer<'de>>(d: D) -> std::result::Result<f64, D::Error> {
    Ok(Value::deserialize(d)?.as_f64().and_then(clamp_ui_scale).unwrap_or(1.0))
}

pub(crate) const DEFAULT_SELECTION_OUTLINE_OPACITY: f64 = 100.0;

pub(crate) fn valid_selection_outline_opacity(opacity: f64) -> Option<f64> {
    (opacity.is_finite() && (0.0..=100.0).contains(&opacity)).then_some(opacity)
}

fn read_selection_outline_opacity<'de, D: serde::Deserializer<'de>>(d: D) -> std::result::Result<f64, D::Error> {
    Ok(Value::deserialize(d)?.as_f64().and_then(valid_selection_outline_opacity).unwrap_or(DEFAULT_SELECTION_OUTLINE_OPACITY))
}

/// App-wide preferences that commands read (nudge amounts, behaviours).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Prefs {
    pub nudge_small: f64,
    pub nudge_big: f64,
    pub keep_tool: bool,
    /// Keep the in-app main menu open after toggling a checked option. Off by default.
    pub keep_menu_open: bool,
    /// Keyboard shortcut replacements, separately for macOS and Windows/Linux (#2036).
    /// Missing commands retain the canonical defaults; an empty list unbinds a command.
    pub shortcuts: crate::keymap::ShortcutOverrides,
    pub rename_duplicates: bool,
    pub highlight_on_hover: bool,
    pub number_keys_opacity: bool,
    pub show_dimensions: bool,
    /// Opacity of selection chrome only, in percent. Documents and their rendered artwork are unchanged.
    #[serde(deserialize_with = "read_selection_outline_opacity")]
    pub selection_outline_opacity: f64,
    /// A mouse wheel zooms the canvas about the pointer, and Cmd/Ctrl+wheel scrolls it (off: the
    /// other way round). Trackpads are unchanged.
    pub scroll_wheel_zoom: bool,
    /// Step a focused numeric field with the wheel over it. Off by default, so panels scroll.
    pub number_wheel: bool,
    /// "system" by default; an explicit "light" or "dark" choice stays saved.
    pub theme: String,
    /// Opaque sRGB interface accent. The default preserves the existing green palettes.
    #[serde(deserialize_with = "read_accent")]
    pub accent: String,
    /// The widths of the main window's left and right panels (the interface keeps them within its
    /// limits; other windows have their own while they are open).
    pub panel_widths: (f32, f32),
    /// The height asked for the main window's pages list, dragged at its lower edge (`None`: a
    /// share of the panel; the interface keeps it within its limits).
    pub pages_height: Option<f32>,
    /// The interface language: a language code ("en", "es", "pt-BR") or "system" (the OS or browser
    /// language). Languages without a catalog show English.
    pub language: String,
    /// The name comments are signed with (and whose comments count as yours).
    pub user_name: String,
    /// Reduce motion: the interface's own animations are off (the system's setting turns them off
    /// too). Prototype transitions still play.
    pub reduce_motion: bool,
    /// Draw the canvas with the GPU rasterizer (`figcraft-gpu`) on desktop where it can, leaving
    /// the frames it can't draw exactly to the CPU renderer. Off by default: the CPU renderer is
    /// the reference, and a device without a usable adapter or textures large enough stays on it.
    pub gpu_canvas: bool,
    /// How large the whole interface is drawn, relative to the system's display scale: 1.0 is the
    /// system's own size, 1.25 a quarter larger (kept between [`UI_SCALE_MIN`] and
    /// [`UI_SCALE_MAX`]). The canvas zoom is separate.
    #[serde(deserialize_with = "read_ui_scale")]
    pub ui_scale: f64,
    /// Files that have a path are saved by themselves a few seconds after they change (the app's
    /// timer; off: only File ▸ Save writes them).
    pub auto_save: bool,
    /// The desktop app starts on the Home screen instead of in a new file.
    pub start_on_home: bool,
    /// Whether the desktop app listens on localhost for MCP / control connections (`pref.mcp`).
    pub mcp_enabled: bool,
    /// Localhost TCP port for the desktop app's MCP / control server (`127.0.0.1:<mcp_port>`).
    pub mcp_port: u16,
    /// `figcraft://` invite links open this app (`collab.registerLinks`); `None`: on for installed
    /// builds, off for development builds.
    pub invite_links: Option<bool>,
    /// Offline spell check while editing text. The bundled checker uses English.
    pub spell_check: bool,
    /// User-approved spellings, independent of any system dictionary.
    pub dictionary: Vec<String>,
    /// Installed plugins (Plugins menu), in menu order.
    #[serde(serialize_with = "plugins::persisted")]
    pub plugins: Vec<plugins::InstalledPlugin>,
    /// Plugins' own settings (`setting_set`), by plugin id, then key: JSON values, capped per
    /// plugin.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub plugin_settings: BTreeMap<String, BTreeMap<String, Value>>,
    /// The plugin directory browsed last (`ws://host:port/`, never with a key).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plugin_directory: Option<String>,
    /// The server files are shared through online (Share ▸ Online, join codes; Publish to server
    /// suggests it too): `wss://host/`, never with a key. [`live::DEFAULT_SERVER`] unless the user
    /// chose another; empty: online sharing is off (saved as empty, so it stays off).
    pub publish_server: String,
    /// Recently opened or saved files, newest first (File ▸ Open recent; at most
    /// [`cmd::file::MAX_RECENT`]).
    pub recent_files: Vec<String>,
    /// The files you put on a server without its key, with their owner tokens (passwords: see
    /// [`online`]), most recently opened first, at most [`online::MAX_ONLINE_FILES`].
    #[serde(skip_serializing_if = "Vec::is_empty", deserialize_with = "online::read")]
    pub online_files: Vec<online::OnlineFile>,
    /// Files successfully joined here, newest first. Their private capabilities stay in local
    /// preferences, never in documents or command summaries.
    #[serde(skip_serializing_if = "Vec::is_empty", deserialize_with = "crate::joined::read")]
    pub joined_files: Vec<crate::joined::JoinedFile>,
    /// The Getting started tips on the home screen were closed (Help ▸ Getting started shows them
    /// again).
    pub getting_started_dismissed: bool,
    pub restore_session: bool,
    pub session: crate::restore::SavedSession,
    /// A prototype's Open link — to a website or to the mail app — that the viewer's own click,
    /// press, drag or key fired opens without first telling them that it leaves the prototype
    /// (the notice's "Don't show this again"; `proto.linkNotice` shows it again). A link a delay
    /// or the pointer passing over a layer fired still shows the notice (#1096).
    pub link_notice_skipped: bool,
    /// How each home screen section shows its files, by section id (`"recents"`): `"grid"` (cards
    /// with the picture each file carries) or `"list"` (a table). Missing: the grid.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub home_views: BTreeMap<String, String>,
    /// What the home screen's recent files are ordered by: `"modified"`, `"name"` or `"created"`.
    pub home_sort: String,
    /// Which way: `"desc"` (newest first, Z to A) or `"asc"`.
    pub home_order: String,
    /// The View menu's toggles turned on or off (rulers, pixel grid, snapping…), by command id:
    /// files opened or made later start with them. Missing: the toggle's default.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub view: BTreeMap<String, bool>,
    /// Page-unit spacing for the canvas pixel grid and pixel snapping, independent of visibility.
    #[serde(deserialize_with = "crate::pixel_grid::read_size")]
    pub pixel_grid_size: f64,
    /// Design review ignores, by document path: whole rule ids and `rule@layer` entries.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub design_review_ignored: BTreeMap<String, Vec<String>>,
    /// The board canvas's background grid (`board.grid`).
    #[serde(deserialize_with = "crate::board_grid::read")]
    pub board_grid: crate::board_grid::BoardGrid,
    /// Recently run actions (the Actions palette's Recents), newest first, at most
    /// [`MAX_RECENT_ACTIONS`]. Entries are `command:<id>`, `plugin:<plugin>/<command>` or
    /// `widget:<plugin>`.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub recent_actions: Vec<String>,
    /// The team libraries you added (`library.add`) or checked (`library.check`), as hashes of
    /// their links (which carry tokens), newest first, at most
    /// [`MAX_TEAM_LIBRARIES`](crate::library_server::MAX_TEAM_LIBRARIES): only these are checked
    /// for updates by themselves. A file naming another one doesn't contact its server (#1097).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub team_libraries: Vec<String>,
}

/// Recently run actions kept for the Actions palette (newest first).
pub const MAX_RECENT_ACTIONS: usize = 5;

impl Default for Prefs {
    fn default() -> Self {
        Self {
            nudge_small: 1.0,
            nudge_big: 10.0,
            keep_tool: false,
            keep_menu_open: false,
            shortcuts: Default::default(),
            rename_duplicates: false,
            highlight_on_hover: true,
            number_keys_opacity: true,
            show_dimensions: true,
            selection_outline_opacity: DEFAULT_SELECTION_OUTLINE_OPACITY,
            scroll_wheel_zoom: false,
            number_wheel: false,
            theme: "system".into(),
            accent: UI_ACCENT_DEFAULT.into(),
            panel_widths: (240.0, 240.0),
            pages_height: None,
            language: "system".into(),
            user_name: cmd::comment::DEFAULT_AUTHOR.into(),
            reduce_motion: false,
            gpu_canvas: false,
            ui_scale: 1.0,
            auto_save: false,
            start_on_home: false,
            mcp_enabled: false,
            mcp_port: DEFAULT_MCP_PORT,
            invite_links: None,
            spell_check: true,
            dictionary: Vec::new(),
            plugins: Vec::new(),
            plugin_settings: BTreeMap::new(),
            plugin_directory: None,
            publish_server: live::DEFAULT_SERVER.into(),
            recent_files: Vec::new(),
            online_files: Vec::new(),
            joined_files: Vec::new(),
            getting_started_dismissed: false,
            restore_session: true,
            session: crate::restore::SavedSession::default(),
            link_notice_skipped: false,
            home_views: BTreeMap::new(),
            home_sort: "modified".into(),
            home_order: "desc".into(),
            view: BTreeMap::new(),
            pixel_grid_size: crate::pixel_grid::DEFAULT_SIZE,
            design_review_ignored: BTreeMap::new(),
            board_grid: Default::default(),
            recent_actions: Vec::new(),
            team_libraries: Vec::new(),
        }
    }
}

/// A request from a command to the UI (open a dialog or panel, show a message). Programmatic
/// callers ignore them.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct UiRequest {
    pub kind: String,
    pub data: Value,
}

/// Clipboard contents (internal; the UI mirrors to the system clipboard).
#[derive(Clone, Debug, Default)]
pub struct Clipboard {
    /// Copied subtrees: (root, nodes) in z-order.
    pub nodes: Vec<(NodeId, Vec<figcraft_doc::Node>)>,
    pub images: std::collections::BTreeMap<String, figcraft_doc::ImageData>,
    /// Copied properties (Copy properties / Paste properties).
    pub props: Option<Value>,
    /// A copied fill or stroke row (`fill.copy`, `stroke.copy`): whether it's a fill, and the paint.
    pub paint: Option<(bool, figcraft_color::Paint)>,
    /// The stroke settings (`weight`, alignment, sides…) copied with a stroke's paint.
    pub stroke: Option<figcraft_doc::StrokeProps>,
    /// The image data the copied paint names, carried so a paste into another file has it.
    pub paint_images: std::collections::BTreeMap<String, figcraft_doc::ImageData>,
    /// The file the copied paint came from: a pattern's node id only means anything there.
    pub paint_source_doc: u64,
    /// A copied effect row (`effect.copy`).
    pub effect: Option<figcraft_doc::Effect>,
    /// The page-space bounds of the copied selection.
    pub bounds: Option<figcraft_geom::Rect>,
    pub source_doc: u64,
    /// Copied component/set roots, excluding cuts and originals discarded by undo (ids are reused).
    pub copied_components: std::collections::BTreeSet<NodeId>,
    /// Monotonic generation of the copied layers (contents, images, bounds and source). A UI
    /// clipboard mirror records it, so a later programmatic copy that never touched the system
    /// clipboard is not mistaken for the user's own copy just because the text still matches.
    pub layers_generation: u64,
    /// The document copied from (pastes into other files follow its mains) and its file path.
    pub source: Option<Arc<Document>>,
    pub source_path: Option<String>,
}

/// The existing green interface accent, also used for old or malformed saved settings.
pub const UI_ACCENT_DEFAULT: &str = "#02DA96";

fn read_accent<'de, D: serde::Deserializer<'de>>(d: D) -> std::result::Result<String, D::Error> {
    let value = Value::deserialize(d)?;
    Ok(value.as_str().and_then(Prefs::parse_accent).map(accent_hex).unwrap_or_else(|| UI_ACCENT_DEFAULT.into()))
}

pub(crate) fn accent_hex(rgb: [u8; 3]) -> String {
    format!("#{:02X}{:02X}{:02X}", rgb[0], rgb[1], rgb[2])
}

impl Prefs {
    /// A safe percentage for painting and programmatic queries, including hand-edited preferences.
    pub fn selection_outline_opacity(&self) -> f64 {
        valid_selection_outline_opacity(self.selection_outline_opacity).unwrap_or(DEFAULT_SELECTION_OUTLINE_OPACITY)
    }

    /// Parse an opaque six-digit sRGB hex accent. Shorthand, alpha and named colors are rejected.
    pub fn parse_accent(value: &str) -> Option<[u8; 3]> {
        let value = value.trim().as_bytes();
        if value.len() != 7 || value.first() != Some(&b'#') || !value[1..].iter().all(u8::is_ascii_hexdigit) {
            return None;
        }
        let nibble = |b: u8| if b.is_ascii_digit() { b - b'0' } else { b.to_ascii_uppercase() - b'A' + 10 };
        Some([nibble(value[1]) * 16 + nibble(value[2]), nibble(value[3]) * 16 + nibble(value[4]), nibble(value[5]) * 16 + nibble(value[6])])
    }

    /// The saved accent for UI rendering. A programmatic invalid field value also uses green.
    pub fn accent_rgb(&self) -> [u8; 3] {
        Self::parse_accent(&self.accent).unwrap_or([2, 218, 150])
    }
}
