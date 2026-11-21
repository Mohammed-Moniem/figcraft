//! Rewrite library reference paths without materializing lazy version documents. Other stored
//! JSON values remain raw, including unknown fields; damaged versions remain lazily unreadable.

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::io::{Seek, Write};

use figcraft_doc::Document;
use serde_json::value::RawValue;

use crate::{FormatError, put_json};

pub(crate) type PathMap<'a> = Option<&'a dyn Fn(&str) -> Option<String>>;

#[cfg(test)]
mod review_tests;

pub(crate) fn put_document<W: Write + Seek>(
    archive: &mut zip::ZipWriter<W>,
    name: &str,
    doc: &Document,
    paths: PathMap<'_>,
) -> Result<(), FormatError> {
    let Some(paths) = paths else { return put_json(archive, name, doc) };
    let mapped: Vec<_> = doc.libraries.enabled.iter().map(|link| paths(&link.path)).collect();
    if mapped.iter().zip(&doc.libraries.enabled).all(|(path, link)| path.as_ref().is_none_or(|path| *path == link.path)) {
        return put_json(archive, name, doc);
    }
    let mut doc = doc.clone();
    for (link, path) in doc.libraries_mut().enabled.iter_mut().zip(mapped) {
        if let Some(path) = path {
            link.path = path;
        }
    }
    // Keep the existing finite serializer for authored documents.
    put_json(archive, name, &doc)
}

type RawObject<'a> = BTreeMap<String, Cow<'a, RawValue>>;

fn object(raw: &RawValue) -> Result<Option<RawObject<'_>>, ()> {
    if !raw.get().trim_start().starts_with('{') {
        return Ok(None);
    }
    struct UniqueObject;
    impl<'de> serde::de::Visitor<'de> for UniqueObject {
        type Value = RawObject<'de>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("an object without duplicate fields")
        }

        fn visit_map<M: serde::de::MapAccess<'de>>(self, mut map: M) -> Result<Self::Value, M::Error> {
            let mut fields = BTreeMap::new();
            while let Some((key, value)) = map.next_entry::<String, &'de RawValue>()? {
                // Decoded keys catch escaped aliases too. Never repair a damaged lazy version.
                if fields.insert(key, Cow::Borrowed(value)).is_some() {
                    return Err(serde::de::Error::custom("duplicate object field"));
                }
            }
            Ok(fields)
        }
    }
    let mut deserializer = serde_json::Deserializer::from_str(raw.get());
    serde::Deserializer::deserialize_map(&mut deserializer, UniqueObject).map(Some).map_err(|_| ())
}

/// None preserves the exact old bytes when no paths change, or when the snapshot was already
/// damaged. Parsing nodes/styles/history is deferred to Version::document exactly as before.
pub(crate) fn stored(json: &[u8], paths: &dyn Fn(&str) -> Option<String>) -> Option<Vec<u8>> {
    let raw: &RawValue = serde_json::from_slice(json).ok()?;
    let mut root = object(raw).ok()??;
    let libraries = root.get("libraries")?;
    let mut library_object = object(libraries).ok()??;
    let enabled = library_object.get("enabled")?;
    let entries: Vec<&RawValue> = serde_json::from_str(enabled.get()).ok()?;
    let mut changed = false;
    let mut mapped = Vec::with_capacity(entries.len());
    for entry in entries {
        let Some(mut fields) = object(entry).ok()? else {
            mapped.push(Cow::Borrowed(entry));
            continue;
        };
        let replacement = (|| {
            let old: String = serde_json::from_str(fields.get("path")?.get()).ok()?;
            let new = paths(&old).filter(|new| *new != old)?;
            fields.insert("path".into(), Cow::Owned(serde_json::value::to_raw_value(&new).ok()?));
            serde_json::value::to_raw_value(&fields).ok()
        })();
        match replacement {
            Some(value) => {
                changed = true;
                mapped.push(Cow::Owned(value));
            }
            None => mapped.push(Cow::Borrowed(entry)),
        }
    }
    if !changed {
        return None;
    }
    let enabled = serde_json::value::to_raw_value(&mapped).ok()?;
    library_object.insert("enabled".into(), Cow::Owned(enabled));
    let libraries = serde_json::value::to_raw_value(&library_object).ok()?;
    // End the borrow of root before replacing its libraries field.
    drop(library_object);
    root.insert("libraries".into(), Cow::Owned(libraries));
    serde_json::to_vec(&root).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rebasing_preserves_unparsed_version_content_and_unknown_fields() {
        // This snapshot cannot become a Document: path rebasing must not load/repair its nodes.
        let json = br#"{"nodes":"damaged nodes stay lazy","future":{"large":12345678901234567890,"spelling":1.2300},"libraries":{"enabled":[{"id":"stable-id","version":7,"path":"../lib/A.figcraft","futureLink":[3,2,1]}],"futureLibrary":{"raw":9.8700}}}"#;
        let bytes = stored(json, &|path| (path == "../lib/A.figcraft").then(|| "../../lib/A.figcraft".into())).unwrap();
        let before: BTreeMap<String, &RawValue> = serde_json::from_slice(json).unwrap();
        let after: BTreeMap<String, &RawValue> = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(after["nodes"].get(), before["nodes"].get());
        assert_eq!(after["future"].get(), before["future"].get());
        let library: BTreeMap<String, &RawValue> = serde_json::from_str(after["libraries"].get()).unwrap();
        assert_eq!(library["futureLibrary"].get(), r#"{"raw":9.8700}"#);
        let entries: serde_json::Value = serde_json::from_str(library["enabled"].get()).unwrap();
        assert_eq!(entries[0]["path"], "../../lib/A.figcraft");
        assert_eq!(entries[0]["id"], "stable-id");
        assert_eq!(entries[0]["version"], 7);
        assert_eq!(entries[0]["futureLink"], serde_json::json!([3, 2, 1]));
        assert!(crate::from_json(std::str::from_utf8(&bytes).unwrap()).is_err());
    }

    #[test]
    fn unchanged_and_malformed_versions_keep_the_original_raw_bytes() {
        for raw in [br#"{"libraries":{"enabled":[{"path":"wss://server/view"}]},"future": 1.2300}"#.as_slice(), b"{ broken", b"[]"] {
            assert!(stored(raw, &|_| None).is_none());
        }
        assert!(stored(b"{ broken", &|_| Some("different".into())).is_none());
    }

    #[test]
    fn only_the_selected_library_path_changes_in_a_mixed_enabled_list() {
        let raw =
            br#"{"libraries":{"enabled":[{"path":"local.figcraft","id":"a","version":4},{"path":"wss://server/view","id":"b","version":8},null]}}"#;
        let bytes = stored(raw, &|path| (path == "local.figcraft").then(|| "../local.figcraft".into())).unwrap();
        let after: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(after["libraries"]["enabled"][0], serde_json::json!({"path":"../local.figcraft","id":"a","version":4}));
        assert_eq!(after["libraries"]["enabled"][1], serde_json::json!({"path":"wss://server/view","id":"b","version":8}));
        assert!(after["libraries"]["enabled"][2].is_null());
    }
}
