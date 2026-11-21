//! Local library references keep the origin of their document, independently of Save As.
//! All resolution here is lexical; the library cache still checks read authority before I/O.

use std::path::{Component, Path, PathBuf};

use figcraft_doc::Document;

use crate::{EngineError, Result, Session};

/// The explicit native document path, made absolute without reading the filesystem.
pub(crate) fn document_origin(path: Option<&str>) -> Option<String> {
    let path = path.filter(|p| craft_docpath::is_local(p))?;
    let absolute = craft_docpath::resolve_absolute(path, None).ok().or_else(|| {
        let absolute = std::path::absolute(path).ok()?;
        craft_docpath::resolve_absolute(absolute.to_str()?, None).ok()
    })?;
    craft_docpath::is_local(absolute.to_str()?).then(|| absolute.to_string_lossy().into_owned())
}

/// Resolve a reference only on this computer and only against an explicit document origin.
pub(crate) fn resolve(path: &str, origin: Option<&str>) -> Result<PathBuf> {
    // A document must never make stat/read contact a network share or Windows device path.
    if !craft_docpath::is_local(path) {
        return Err(EngineError::Other(format!("{path}: a library is opened only from this computer")));
    }
    // An explicit relative input filename is an origin; absence is never a CWD fallback.
    let origin = document_origin(origin);
    let resolved = craft_docpath::resolve_absolute(path, origin.as_deref().map(Path::new))
        .map_err(|_| EngineError::Other(format!("{path}: the library path needs a saved local document origin or an absolute local path")))?;
    if !resolved.to_str().is_some_and(craft_docpath::is_local) {
        return Err(EngineError::Other(format!("{path}: a library is opened only from this computer")));
    }
    Ok(resolved)
}

/// Preserve a copied link's meaning before it joins another document. Blocked network/device
/// references remain blocked verbatim; local relative links require their source's retained origin.
/// No filesystem access, destination-origin or CWD fallback occurs here.
pub(crate) fn for_transfer(path: &str, origin: Option<&str>) -> Result<String> {
    if !craft_docpath::is_local(path) {
        return Ok(path.to_string());
    }
    resolve(path, origin)?.to_str().map(str::to_string).ok_or_else(|| EngineError::Other("the library path isn't UTF-8".into()))
}

/// A local reference relative to `destination`, preserving an absolute path on another root.
pub fn for_destination(path: &str, origin: Option<&str>, destination: &str) -> Option<String> {
    let absolute = resolve(path, origin).ok()?;
    let destination = document_origin(Some(destination))?;
    let folder = Path::new(&destination).parent()?;
    let from: Vec<_> = folder.components().collect();
    let to: Vec<_> = absolute.components().collect();
    // Different Windows drives cannot have a relative reference. No filesystem probing occurs.
    if from.first() != to.first() {
        return absolute.to_str().map(str::to_string);
    }
    let common = from.iter().zip(&to).take_while(|(a, b)| a == b).count();
    let mut relative = PathBuf::new();
    for component in from.iter().skip(common) {
        if !matches!(component, Component::Normal(_)) {
            return absolute.to_str().map(str::to_string);
        }
        relative.push("..");
    }
    for component in to.iter().skip(common) {
        relative.push(component.as_os_str());
    }
    relative.to_str().map(str::to_string)
}

/// An input document read without a tab keeps its references when converted by the CLI.
pub fn make_absolute(doc: &mut Document, origin: &str) {
    let origin = document_origin(Some(origin));
    let replacements: Vec<_> = doc.libraries.enabled.iter().map(|link| resolve(&link.path, origin.as_deref()).ok()).collect();
    if replacements.iter().zip(&doc.libraries.enabled).all(|(p, l)| p.as_ref().and_then(|p| p.to_str()).is_none_or(|p| p == l.path)) {
        return;
    }
    for (link, path) in doc.libraries_mut().enabled.iter_mut().zip(replacements) {
        if let Some(path) = path.and_then(|p| p.to_str().map(str::to_string)) {
            link.path = path;
        }
    }
}

impl Session {
    /// Read the already-cached library using this document's retained origin (no file access).
    pub fn cached_library(&self, path: &str) -> Option<std::result::Result<&std::sync::Arc<figcraft_doc::PublishedLibrary>, &str>> {
        self.libraries.cached_from(path, self.library_origin())
    }

    /// Branch tabs use their main file's retained origin; Save As changes neither one's meaning.
    pub fn library_origin(&self) -> Option<&str> {
        self.docs.get(crate::branches::file_tab(self))?.library_origin.as_deref()
    }
}
