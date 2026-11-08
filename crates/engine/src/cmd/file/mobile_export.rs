//! Mobile asset folders, from public platform specifications (#1689):
//! https://docs.flutter.dev/ui/assets/assets-and-images#resolution-aware-image-assets
//! https://developer.android.com/training/multiscreen/screendensities
//! https://developer.apple.com/library/archive/documentation/Xcode/Reference/xcode_ref-Asset_Catalog_Format/ImageSetType.html

use crate::{Result, bad};
use figcraft_doc::{Document, ExportFormat, ExportSetting, ExportSize, NodeId};
use serde_json::json;
use std::collections::BTreeSet;

/// Folder convention for one export action. This does not change stored export rows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ExportLayout {
    #[default]
    Flat,
    Flutter,
    Ios,
    Android,
}

impl ExportLayout {
    pub const ALL: [Self; 4] = [Self::Flat, Self::Flutter, Self::Ios, Self::Android];

    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|l| l.key() == key)
    }

    pub fn key(self) -> &'static str {
        match self {
            Self::Flat => "flat",
            Self::Flutter => "flutter",
            Self::Ios => "ios",
            Self::Android => "android",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Flat => "Flat",
            Self::Flutter => "Flutter",
            Self::Ios => "iOS",
            Self::Android => "Android",
        }
    }

    pub fn supports(self, format: ExportFormat) -> bool {
        match self {
            Self::Flat => true,
            Self::Flutter => format.is_raster(),
            Self::Ios => matches!(format, ExportFormat::Png | ExportFormat::Jpg),
            Self::Android => format.is_raster() || format == ExportFormat::VectorDrawable,
        }
    }
}

/// One planned image or generated platform manifest. Planning finishes before anything writes.
pub struct MobileExport {
    pub id: NodeId,
    pub file: String,
    pub content: MobileExportContent,
}

pub enum MobileExportContent {
    Image { setting: ExportSetting, scale: f64 },
    Manifest(Vec<u8>),
}

/// Plan a complete mobile export, replacing row sizes with platform densities. Repeated density
/// rows of the same asset coalesce; explicit variants and differing render options stay distinct.
/// The input and output caps prevent truncation and bound duplicate/name searches.
pub fn mobile_export_plan(
    doc: &Document,
    items: impl IntoIterator<Item = (NodeId, Vec<ExportSetting>)>,
    layout: ExportLayout,
) -> Result<Vec<MobileExport>> {
    if layout == ExportLayout::Flat {
        return Err(bad("file.exportLayers", "mobile export planning needs flutter, ios or android"));
    }
    let mut count = 0usize;
    let mut assets: Vec<(NodeId, ExportSetting)> = Vec::new();
    for (id, settings) in items {
        if doc.node(id).is_none() {
            continue;
        }
        for mut setting in settings {
            count = count.saturating_add(1);
            if count > super::export::MAX_EXPORT_FILES {
                return Err(bad("file.exportLayers", "mobile export has more than 1000 source settings; export fewer layers"));
            }
            if !layout.supports(setting.format) {
                return Err(bad(
                    "file.exportLayers",
                    format!(
                        "{} layout does not support {}; use PNG/JPG{} or flat layout",
                        layout.label(),
                        setting.format.label(),
                        if layout == ExportLayout::Ios {
                            ""
                        } else if layout == ExportLayout::Android {
                            "/WebP/XML"
                        } else {
                            "/WebP"
                        }
                    ),
                ));
            }
            if setting.suffix == format!("@{}", setting.size.label()) {
                setting.suffix.clear();
            }
            setting.size = ExportSize::default();
            if !assets.iter().any(|(i, e)| *i == id && *e == setting) {
                assets.push((id, setting));
            }
        }
    }
    let mut out = Vec::new();
    let mut used = BTreeSet::new();
    for (id, setting) in assets {
        let Some(node) = doc.node(id) else { continue };
        let xml = setting.format == ExportFormat::VectorDrawable;
        let scales: &[f64] = if xml {
            &[1.0]
        } else if layout == ExportLayout::Android {
            &[1.0, 1.5, 2.0, 3.0, 4.0]
        } else {
            &[1.0, 2.0, 3.0]
        };
        let nfiles = scales.len() + usize::from(layout == ExportLayout::Ios);
        if out.len().saturating_add(nfiles) > super::export::MAX_EXPORT_FILES {
            return Err(bad("file.exportLayers", "mobile layout would create more than 1000 files; export fewer layers"));
        }
        let input: String = node.name.chars().chain(setting.suffix.chars()).take(400).collect();
        let name = mobile_name(&input, layout);
        let mut unique = name.clone();
        for suffix in 2..=1001 {
            if used.insert(unique.to_lowercase()) {
                break;
            }
            unique = format!("{name}_{suffix}");
        }
        let ext = setting.format.ext();
        let mut images = Vec::new();
        for (index, &scale) in scales.iter().enumerate() {
            let leaf = if layout == ExportLayout::Ios && scale > 1.0 { format!("{unique}@{scale}x.{ext}") } else { format!("{unique}.{ext}") };
            let file = match layout {
                ExportLayout::Flutter if scale > 1.0 => format!("{scale:.1}x/{leaf}"),
                ExportLayout::Ios => format!("{unique}.imageset/{leaf}"),
                ExportLayout::Android if xml => format!("drawable/{leaf}"),
                ExportLayout::Android => {
                    let density = ["mdpi", "hdpi", "xhdpi", "xxhdpi", "xxxhdpi"].get(index).copied().unwrap_or("mdpi");
                    format!("drawable-{density}/{leaf}")
                }
                _ => leaf.clone(),
            };
            if layout == ExportLayout::Ios {
                images.push(json!({"filename":leaf,"idiom":"universal","scale":format!("{scale}x")}));
            }
            out.push(MobileExport { id, file, content: MobileExportContent::Image { setting: setting.clone(), scale } });
        }
        if layout == ExportLayout::Ios {
            let manifest = json!({"images":images,"info":{"author":"FigCraft","version":1}});
            let bytes = serde_json::to_vec_pretty(&manifest).map_err(|e| crate::EngineError::Other(e.to_string()))?;
            out.push(MobileExport { id, file: format!("{unique}.imageset/Contents.json"), content: MobileExportContent::Manifest(bytes) });
        }
    }
    Ok(out)
}

/// Each mobile asset is one bounded leaf: slashes cannot add directories and Windows device
/// names cannot escape ordinary file behavior. Android resources use ASCII lower snake case;
/// Flutter uses the same predictable identifiers. iOS transliterates to readable ASCII so
/// canonically equivalent Unicode names cannot overwrite each other on macOS filesystems.
fn mobile_name(input: &str, layout: ExportLayout) -> String {
    let mut out = String::new();
    if layout == ExportLayout::Ios {
        for c in deunicode::deunicode(input).chars().take(100) {
            let c = if !c.is_ascii() || c.is_control() || super::FORBIDDEN_FILE_NAME_CHARS.contains(c) { '_' } else { c };
            if out.len() + c.len_utf8() > 180 {
                break;
            }
            out.push(c);
        }
        out = out.trim_matches(|c: char| c == '.' || c.is_whitespace()).to_owned();
    } else {
        let mut separator = false;
        for c in input.chars().take(400) {
            if c.is_ascii_alphanumeric() {
                if separator && !out.is_empty() && out.len() < 100 {
                    out.push('_');
                }
                if out.len() >= 100 {
                    break;
                }
                out.push(c.to_ascii_lowercase());
                separator = false;
            } else {
                separator = true;
            }
        }
    }
    if out.is_empty() {
        out.push_str("asset");
    }
    if out.starts_with(|c: char| c.is_ascii_digit()) {
        out.insert_str(0, "asset_");
    }
    let device = out.split('.').next().unwrap_or_default().trim_end_matches(' ').to_ascii_lowercase();
    let numbered_device = device
        .strip_prefix("com")
        .or_else(|| device.strip_prefix("lpt"))
        .is_some_and(|number| matches!(number, "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"));
    if matches!(device.as_str(), "con" | "prn" | "aux" | "nul" | "conin$" | "conout$") || numbered_device {
        out.insert_str(0, "asset_");
    }
    out
}

/// Deliver the complete mobile plan through the same confined writes and ZIP budget as flat
/// layer export. Generated manifests are ordinary members of every delivery mode.
pub(super) fn export_mobile_layers(s: &mut super::Session, p: &serde_json::Value, layout: ExportLayout) -> Result<serde_json::Value> {
    use super::{boolean, string, targets};
    use base64::Engine as _;
    let cmd = "file.exportLayers";
    if boolean(p, "assets").unwrap_or(false) {
        return Err(bad(cmd, "mobile layouts use layer export settings; select the layers and omit assets"));
    }
    let doc = s.doc()?;
    let mut ids = targets(s, p);
    if ids.is_empty() {
        ids = super::exportable_layers(doc, s.page()?);
    }
    let jobs = mobile_export_plan(doc, ids.iter().filter_map(|id| Some((*id, doc.node(*id)?.exports.clone()))), layout)?;
    if jobs.is_empty() {
        return Err(crate::EngineError::Disabled(cmd.into(), "no export settings (add one with export.add)".into()));
    }
    let dir = string(p, "dir");
    let zip = dir.is_none() && boolean(p, "zip").unwrap_or(false);
    let print = super::export::has_print_param(p).then(|| super::export::print_param(cmd, p, Default::default())).transpose()?;
    let compat = super::vd_compat_param(cmd, p)?;
    let mut files = Vec::with_capacity(jobs.len());
    let mut members = Vec::new();
    let mut response_bytes = 0usize;
    for job in &jobs {
        let (bytes, warnings) = match &job.content {
            MobileExportContent::Manifest(bytes) => (bytes.clone(), None),
            MobileExportContent::Image { setting, scale } => {
                let setting =
                    ExportSetting { print: print.unwrap_or(setting.print), vd_compat: compat.unwrap_or(setting.vd_compat), ..setting.clone() };
                let (bytes, warnings) = super::export_file(doc, &[job.id], &setting, *scale)?;
                (bytes, (setting.format == ExportFormat::VectorDrawable).then_some(warnings))
            }
        };
        let mut f = json!({"name":job.file,"id":job.id,"bytes":bytes.len()});
        if let Some(warnings) = warnings {
            f["warnings"] = json!(warnings);
        }
        // Without a folder, all image bytes survive together in the response. Apply the ZIP
        // budget to individual base64 members too, before encoding another retained copy.
        if dir.is_none() {
            reserve_response_bytes(&mut response_bytes, bytes.len(), super::export::MAX_ZIP_BYTES)?;
        }
        match &dir {
            Some(dir) => {
                let path = std::path::Path::new(dir).join(&job.file);
                if let Some(parent) = path.parent() {
                    crate::file_access::create_dir_all(parent)?;
                }
                let path = path.to_string_lossy().into_owned();
                super::write(&path, &bytes)?;
                f["path"] = json!(path);
            }
            None if zip => super::export::add_member(&mut members, job.file.as_str(), bytes, super::export::MAX_ZIP_BYTES)?,
            None => f["data"] = json!(base64::engine::general_purpose::STANDARD.encode(bytes)),
        }
        files.push(f);
    }
    let with_settings: BTreeSet<NodeId> = jobs.iter().map(|j| j.id).collect();
    let mut result = json!({"files":files,"skipped":ids.len().saturating_sub(with_settings.len())});
    if zip {
        let archive = figcraft_format::zip_files(members.iter().map(|(name, bytes)| (*name, bytes.as_slice())))
            .map_err(|e| crate::EngineError::Other(e.to_string()))?;
        result["data"] = json!(base64::engine::general_purpose::STANDARD.encode(archive));
    }
    Ok(result)
}

pub(super) fn reserve_response_bytes(total: &mut usize, bytes: usize, limit: usize) -> Result<()> {
    let next = total.checked_add(bytes).filter(|n| *n <= limit).ok_or_else(|| {
        bad(
            "file.exportLayers",
            format!("mobile export files would pass {} MB in one response; export fewer layers or write to a folder", limit >> 20),
        )
    })?;
    *total = next;
    Ok(())
}
