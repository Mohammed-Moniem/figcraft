//! Original icons for the starter templates: each is one outline drawn for FigCraft from straight
//! lines, arcs and simple curves on a 24 px grid (2 px margin, 2 px round strokes).

/// An icon: its name and its outline as SVG path data on the 24 px grid.
pub(super) struct Glyph {
    pub name: &'static str,
    pub path: &'static str,
}

macro_rules! glyphs {
    ($($id:ident $name:literal $path:literal)*) => {
        $(pub(super) const $id: Glyph = Glyph { name: $name, path: $path };)*
        /// Every icon, in the order the icon set lays them out.
        pub(super) const ALL: &[Glyph] = &[$($id),*];
    };
}

glyphs! {
    HOME "Home" "M4 11 L12 4 L20 11 V20 H4 Z M10 20 V14 H14 V20"
    SEARCH "Search" "M5 11 a6 6 0 1 0 12 0 a6 6 0 1 0 -12 0 M15.5 15.5 L20 20"
    USER "User" "M8 8 a4 4 0 1 0 8 0 a4 4 0 1 0 -8 0 M4 20 C4 16.5 7.5 14.5 12 14.5 C16.5 14.5 20 16.5 20 20"
    BELL "Bell" "M6 17 V11 C6 7.5 8.5 5 12 5 C15.5 5 18 7.5 18 11 V17 M4 17 H20 M10.5 20 H13.5"
    HEART "Heart" "M12 20 L4.5 12.5 C2.5 10.5 2.5 7.5 4.5 5.8 C6.5 4 9.5 4.5 12 7.5 C14.5 4.5 17.5 4 19.5 5.8 C21.5 7.5 21.5 10.5 19.5 12.5 Z"
    STAR "Star" "M12 3.5 L14.6 9 L20.5 9.8 L16.2 14 L17.3 20 L12 17.1 L6.7 20 L7.8 14 L3.5 9.8 L9.4 9 Z"
    PIN "Pin" "M12 21 C9 18 5 14 5 10 C5 6 8 3 12 3 C16 3 19 6 19 10 C19 14 15 18 12 21 Z M9.5 10 a2.5 2.5 0 1 0 5 0 a2.5 2.5 0 1 0 -5 0"
    MOUNTAIN "Mountain" "M3 19 L9 7 L13 14 L16 10 L21 19 Z"
    CLOCK "Clock" "M4 12 a8 8 0 1 0 16 0 a8 8 0 1 0 -16 0 M12 7.5 V12 L15 14"
    CALENDAR "Calendar" "M4 6 H20 V20 H4 Z M4 10.5 H20 M8 4 V8 M16 4 V8"
    MAIL "Mail" "M3 6 H21 V18 H3 Z M3 7 L12 13.5 L21 7"
    CHART "Chart" "M4 20 H20 M7 20 V12 M12 20 V6 M17 20 V10"
    LAYERS "Layers" "M12 4 L20 8.5 L12 13 L4 8.5 Z M4 12.5 L12 17 L20 12.5 M4 16.5 L12 21 L20 16.5"
    SLIDERS "Sliders" "M4 7 H10 M14 7 H20 M10 7 a2 2 0 1 0 4 0 a2 2 0 1 0 -4 0 M4 17 H8 M12 17 H20 M8 17 a2 2 0 1 0 4 0 a2 2 0 1 0 -4 0"
    BOOKMARK "Bookmark" "M7 4 H17 V20 L12 16 L7 20 Z"
    DOWNLOAD "Download" "M12 4 V15 M7 10.5 L12 15.5 L17 10.5 M5 20 H19"
    PLUS "Plus" "M12 5 V19 M5 12 H19"
    CHECK "Check" "M5 12.5 L10 17.5 L19 7"
    CLOSE "Close" "M6 6 L18 18 M18 6 L6 18"
    MENU "Menu" "M4 7 H20 M4 12 H20 M4 17 H20"
    ARROW_LEFT "Arrow left" "M19 12 H5 M11 6 L5 12 L11 18"
    ARROW_RIGHT "Arrow right" "M5 12 H19 M13 6 L19 12 L13 18"
    CHEVRON_RIGHT "Chevron right" "M9 6 L15 12 L9 18"
    CHEVRON_DOWN "Chevron down" "M6 9 L12 15 L18 9"
    FOLDER "Folder" "M3 8 V5 H9 L12 8 H21 V20 H3 Z"
    FILE "File" "M6 3 H14 L19 8 V21 H6 Z M14 3 V8 H19 M9 12 H16 M9 16 H14"
    ADD_FILE "Add file" "M6 3 H14 L19 8 V21 H6 Z M14 3 V8 H19 M9 14 H16 M12.5 10.5 V17.5"
    COPY "Copy" "M8 8 H20 V21 H8 Z M4 16 V3 H16"
    EDIT "Edit" "M4 20 L5 15 L16.5 3.5 L20.5 7.5 L9 19 Z M13.5 6.5 L17.5 10.5"
    TRASH "Trash" "M4 7 H20 M6 7 L7 21 H17 L18 7 M9 7 V3 H15 V7 M10 11 V17 M14 11 V17"
    UPLOAD "Upload" "M12 16 V4 M7 9 L12 4 L17 9 M5 16 V20 H19 V16"
    EXTERNAL_LINK "External link" "M13 4 H20 V11 M20 4 L10 14 M9 5 H4 V20 H19 V15"
    LINK "Link" "M10 7 L12 5 C14 3 17 3 19 5 C21 7 21 10 19 12 L17 14 M14 17 L12 19 C10 21 7 21 5 19 C3 17 3 14 5 12 L7 10 M8 16 L16 8"
    UNDO "Undo" "M8 5 L3 10 L8 15 M3 10 H14 C18 10 20 12.5 20 16 C20 18 19 20 17 21"
    REDO "Redo" "M16 5 L21 10 L16 15 M21 10 H10 C6 10 4 12.5 4 16 C4 18 5 20 7 21"
    FILTER "Filter" "M3 4 H21 L14 12 V19 L10 21 V12 Z"
    GRID "Grid" "M4 4 H10 V10 H4 Z M14 4 H20 V10 H14 Z M4 14 H10 V20 H4 Z M14 14 H20 V20 H14 Z"
    LIST "List" "M4 6 H5 M9 6 H20 M4 12 H5 M9 12 H20 M4 18 H5 M9 18 H20"
    MORE "More" "M4 12 a1 1 0 1 0 2 0 a1 1 0 1 0 -2 0 M11 12 a1 1 0 1 0 2 0 a1 1 0 1 0 -2 0 M18 12 a1 1 0 1 0 2 0 a1 1 0 1 0 -2 0"
    ARROW_UP "Arrow up" "M12 20 V4 M6 10 L12 4 L18 10"
    ARROW_DOWN "Arrow down" "M12 4 V20 M6 14 L12 20 L18 14"
    CHEVRON_LEFT "Chevron left" "M15 6 L9 12 L15 18"
    CHEVRON_UP "Chevron up" "M6 15 L12 9 L18 15"
    REFRESH "Refresh" "M20 10 C19 6 16 4 12 4 C7.5 4 4 7.5 4 12 C4 16.5 7.5 20 12 20 C15 20 17.5 18.5 19 16 M20 4 V10 H14"
    PLAY "Play" "M7 4 L20 12 L7 20 Z"
    PAUSE "Pause" "M6 4 H10 V20 H6 Z M14 4 H18 V20 H14 Z"
    STOP "Stop" "M5 5 H19 V19 H5 Z"
    VOLUME "Volume" "M3 9 H7 L12 4 V20 L7 15 H3 Z M16 8 C19 10 19 14 16 16 M19 4.5 C21.5 8.5 21.5 15.5 19 19.5"
    CAMERA "Camera" "M3 7 H8 L10 4 H15 L17 7 H21 V20 H3 Z M8 13.5 a4 4 0 1 0 8 0 a4 4 0 1 0 -8 0"
    MICROPHONE "Microphone" "M9 6 a3 3 0 0 1 6 0 V12 a3 3 0 0 1 -6 0 Z M6 11 V12 a6 6 0 0 0 12 0 V11 M12 18 V21 M9 21 H15"
    HEADPHONES "Headphones" "M4 14 V11 a8 8 0 0 1 16 0 V14 M4 13 H8 V21 H4 Z M16 13 H20 V21 H16 Z"
    IMAGE "Image" "M3 4 H21 V20 H3 Z M3 16 L8 11 L13 16 L16 13 L21 18 M14 8 a1.5 1.5 0 1 0 3 0 a1.5 1.5 0 1 0 -3 0"
    MONITOR "Monitor" "M3 4 H21 V16 H3 Z M12 16 V21 M7 21 H17"
    PHONE "Phone" "M7 3 H17 V21 H7 Z M10 6 H14 M11 18 H13"
    MESSAGE "Message" "M3 4 H21 V17 H10 L5 21 V17 H3 Z M7 9 H17 M7 13 H13"
    SEND "Send" "M3 4 L21 12 L3 20 L6 12 Z M6 12 H15"
    PAPERCLIP "Paperclip" "M9 13 L15 7 a2 2 0 0 1 3 3 L10 18 a4 4 0 0 1 -5.5 -5.5 L12 4.25 a5 5 0 0 1 7 7 L11 19"
    GLOBE "Globe" "M3 12 a9 9 0 1 0 18 0 a9 9 0 1 0 -18 0 M3 12 H21 M12 3 C6 8 6 16 12 21 C18 16 18 8 12 3 Z"
    CLOUD "Cloud" "M6 19 C2 19 2 13 6 12 C5 7 9 4 13 5 C16 5 18 8 18 10 C22 10 22 19 18 19 Z"
    LOCK "Lock" "M5 10 H19 V21 H5 Z M8 10 V7 a4 4 0 0 1 8 0 V10 M12 14 V17"
    UNLOCK "Unlock" "M5 10 H19 V21 H5 Z M8 10 V7 a4 4 0 0 1 8 0 M12 14 V17"
    SHIELD "Shield" "M12 3 L20 6 V12 C20 16 17 19 12 21 C7 19 4 16 4 12 V6 Z M8 12 L11 15 L16 9"
    SHOPPING_CART "Shopping cart" "M3 4 H5 L8 15 H18 L21 7 H6 M9 19 a1.5 1.5 0 1 0 3 0 a1.5 1.5 0 1 0 -3 0 M16 19 a1.5 1.5 0 1 0 3 0 a1.5 1.5 0 1 0 -3 0"
    CREDIT_CARD "Credit card" "M3 5 H21 V19 H3 Z M3 9 H21 M7 15 H11 M15 15 H17"
}

#[cfg(test)]
mod tests {
    use super::ALL;
    use std::collections::BTreeSet;

    #[test]
    fn starter_icons_preserve_original_names_and_expand_to_64() {
        let names: Vec<_> = ALL.iter().map(|glyph| glyph.name).collect();
        assert_eq!(names.len(), 64, "the starter includes forty additional original icons");
        assert_eq!(
            &names[..24],
            [
                "Home",
                "Search",
                "User",
                "Bell",
                "Heart",
                "Star",
                "Pin",
                "Mountain",
                "Clock",
                "Calendar",
                "Mail",
                "Chart",
                "Layers",
                "Sliders",
                "Bookmark",
                "Download",
                "Plus",
                "Check",
                "Close",
                "Menu",
                "Arrow left",
                "Arrow right",
                "Chevron right",
                "Chevron down",
            ]
        );
        assert_eq!(
            &names[24..],
            [
                "Folder",
                "File",
                "Add file",
                "Copy",
                "Edit",
                "Trash",
                "Upload",
                "External link",
                "Link",
                "Undo",
                "Redo",
                "Filter",
                "Grid",
                "List",
                "More",
                "Arrow up",
                "Arrow down",
                "Chevron left",
                "Chevron up",
                "Refresh",
                "Play",
                "Pause",
                "Stop",
                "Volume",
                "Camera",
                "Microphone",
                "Headphones",
                "Image",
                "Monitor",
                "Phone",
                "Message",
                "Send",
                "Paperclip",
                "Globe",
                "Cloud",
                "Lock",
                "Unlock",
                "Shield",
                "Shopping cart",
                "Credit card",
            ]
        );
        assert_eq!(names.iter().copied().collect::<BTreeSet<_>>().len(), 64, "stable unique component names");
        assert_eq!(ALL.iter().map(|glyph| glyph.path).collect::<BTreeSet<_>>().len(), 64, "no repeated placeholder paths");
        assert!(ALL.iter().all(|glyph| !glyph.path.trim().is_empty()));
    }

    #[test]
    fn starter_icons_have_distinct_finite_geometry_inside_the_stroke_margin() {
        use figcraft_geom::{BezPath, Shape};

        let mut paths = BTreeSet::new();
        for glyph in ALL {
            let path = BezPath::from_svg(glyph.path).unwrap_or_else(|e| panic!("{}: {e}", glyph.name));
            let bounds = path.bounding_box();
            assert!([bounds.x0, bounds.y0, bounds.x1, bounds.y1].iter().all(|v| v.is_finite()), "{}", glyph.name);
            assert!(bounds.width() > 0.0 && bounds.height() > 0.0, "{}: real two-dimensional geometry", glyph.name);
            assert!(path.segments().count() > 0, "{}: drawable segments", glyph.name);
            // The 2 px round stroke extends one unit beyond its centreline, leaving a 2 px
            // margin in the 24 px component. Curve control points may sit outside this box.
            assert!(bounds.x0 >= 3.0 && bounds.y0 >= 3.0 && bounds.x1 <= 21.0 && bounds.y1 <= 21.0, "{}: {bounds:?}", glyph.name);
            assert!(paths.insert(path.to_svg()), "{}: duplicate geometry", glyph.name);
        }
    }
}
