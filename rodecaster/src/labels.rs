//! Input source names adapted from `Holfz/rodecaster-routing` (`crates/rcp-model/src/labels.rs`),
//! MIT licensed, Copyright (c) Holfz. The console stores no input names, so they come from here;
//! a source with no supportable label keeps `None` and renders as its number.

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Shape {
    pub input_sources: usize,
    pub channels: usize,
}

const PRO_II: Shape = Shape {
    input_sources: 30,
    channels: 10,
};

const PRO_II_SOURCES: [Option<&str>; 21] = [
    Some("Combo 1"),
    Some("Combo 2"),
    Some("Combo 3"),
    Some("Combo 4"),
    Some("Wireless 1"),
    Some("Wireless 2"),
    Some("Bluetooth"),
    Some("USB 1 Main"),
    Some("USB 1 Comms"),
    Some("USB 2 Main"),
    None,
    Some("Smart Pads"),
    Some("RC Game"),
    Some("RC Music"),
    Some("RC Virtual A"),
    Some("RC Virtual B"),
    Some("Call Me 1"),
    Some("Call Me 2"),
    Some("Call Me 3"),
    Some("SMART Pads"),
    None,
];

#[must_use]
pub fn source_label(shape: Shape, source: usize) -> String {
    if shape == PRO_II
        && let Some(Some(label)) = PRO_II_SOURCES.get(source)
    {
        return (*label).to_string();
    }
    format!("source {source}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn confirmed_anchors_are_carried() {
        assert_eq!(source_label(PRO_II, 0), "Combo 1");
        assert_eq!(source_label(PRO_II, 12), "RC Game");
        assert_eq!(source_label(PRO_II, 13), "RC Music");
    }

    #[test]
    fn unidentified_sources_are_not_given_invented_names() {
        assert_eq!(source_label(PRO_II, 10), "source 10");
        assert_eq!(source_label(PRO_II, 20), "source 20");
        assert_eq!(source_label(PRO_II, 25), "source 25");
    }

    #[test]
    fn an_unrecognised_shape_yields_no_labels() {
        let duo_ish = Shape {
            input_sources: 30,
            channels: 8,
        };
        assert_eq!(source_label(duo_ish, 0), "source 0");
    }
}
