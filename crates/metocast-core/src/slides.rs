//! Read models for the PowerPoint file, folder and slide-generation endpoints.
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// The PPT endpoints wrap their result: `{"success": true, "data": …}`.
#[derive(Debug, Clone, Deserialize)]
pub struct PptEnvelope<T> {
    pub data: T,
}

/// One deck from `GET /api/ppt/files`. The search fields default, so an older server's
/// answer (file names only) still reads.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PptFile {
    pub id: String,
    pub name: String,
    pub path: String,
    pub folder_id: String,
    /// The file name without its extension.
    #[serde(default)]
    pub title: String,
    /// The hymn number, from the last "(47)" in the title.
    #[serde(default)]
    pub number: Option<i32>,
    /// Why the deck matched; absent when nothing was searched for.
    #[serde(default)]
    pub matched_in: Option<DeckMatch>,
    #[serde(default)]
    pub title_parts: Vec<HighlightPart>,
    /// Slide text around the match; empty unless `matched_in` is `Text`.
    #[serde(default)]
    pub snippet_parts: Vec<HighlightPart>,
}

/// What a search matched in a deck.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DeckMatch {
    Number,
    Title,
    Text,
}

/// A run of text in a search result; `hit` marks the words the search matched.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HighlightPart {
    pub text: String,
    pub hit: bool,
}

/// One watched folder from `GET /api/ppt/folders`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PptFolder {
    pub id: Uuid,
    pub path: String,
    pub name: String,
    pub sort_order: i32,
}

/// `POST /api/ppt/song` body.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateSongSlides {
    pub title: String,
    pub lyrics: String,
}

/// What `POST /api/ppt/song` wrote.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SongSlides {
    pub file_path: String,
    pub slide_count: u32,
}

/// The decks `POST /api/events/{id}/slides` wrote, one per Bible reference.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventSlides {
    pub files: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slide_models_decode_the_server_shapes() {
        // Shapes from server/ppt.rs and server/routes.rs.
        // Both PPT lists arrive inside the `data` envelope.
        let files: PptEnvelope<Vec<PptFile>> = serde_json::from_str(
            r#"{"success":true,"data":[{"id":"a1","name":"Song.pptx","path":"/Decks/Song.pptx","folderId":"f1"}]}"#,
        )
        .unwrap();
        assert_eq!(files.data[0].folder_id, "f1");

        let hits: PptEnvelope<Vec<PptFile>> = serde_json::from_str(
            r#"{"success":true,"data":[{"id":"a1","name":"Hála (47).pptx","path":"/Decks/Hála (47).pptx","folderId":"f1","title":"Hála (47)","number":47,"matchedIn":"text","titleParts":[{"text":"Hála (47)","hit":false}],"snippetParts":[{"text":"ki ","hit":false},{"text":"hálát","hit":true}]}]}"#,
        )
        .unwrap();
        assert_eq!(hits.data[0].number, Some(47));
        assert_eq!(hits.data[0].matched_in, Some(DeckMatch::Text));
        assert!(hits.data[0].snippet_parts[1].hit);

        let folders: PptEnvelope<Vec<PptFolder>> = serde_json::from_str(
            r#"{"success":true,"data":[{"id":"6f2c1f8e-9a3e-4a8b-9f6e-2d8a1b7c3e10","path":"/Decks","name":"Decks","sortOrder":0}]}"#,
        )
        .unwrap();
        assert_eq!(folders.data[0].name, "Decks");

        let song: SongSlides =
            serde_json::from_str(r#"{"filePath":"/Decks/Song.pptx","slideCount":7}"#).unwrap();
        assert_eq!(
            (song.file_path.as_str(), song.slide_count),
            ("/Decks/Song.pptx", 7)
        );

        let generated: EventSlides =
            serde_json::from_str(r#"{"files":["/Slides/textus.pptx","/Slides/lekcio.pptx"]}"#)
                .unwrap();
        assert_eq!(generated.files.len(), 2);

        assert_eq!(
            serde_json::to_value(CreateSongSlides {
                title: "Amazing Grace".to_string(),
                lyrics: "line".to_string(),
            })
            .unwrap(),
            serde_json::json!({"title": "Amazing Grace", "lyrics": "line"})
        );
    }
}

/// `GET`/`PUT /api/presenter/welcome`: the welcome screen a text Order of Service step shows.
/// `has_logo` and `replaced_photos` come from the uploads and are ignored on `PUT`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct WelcomeSettings {
    /// The greeting a new event's welcome step is named after.
    pub title: String,
    /// Shown under the title when the step has no notes of its own.
    pub subtitle: String,
    /// Shown above the event's date at the bottom.
    pub church_name: String,
    /// How long each photo stays, 5–3600 seconds.
    pub interval_seconds: u32,
    #[serde(skip_deserializing)]
    pub has_logo: bool,
    /// Which photos (1–12) were replaced, so a client can offer to reset them.
    #[serde(skip_deserializing)]
    pub replaced_photos: Vec<u32>,
}

impl Default for WelcomeSettings {
    fn default() -> Self {
        Self {
            title: "Isten hozott!".to_string(),
            subtitle: "Az ÚR áldása legyen életeden!".to_string(),
            church_name: String::new(),
            interval_seconds: 20,
            has_logo: false,
            replaced_photos: Vec::new(),
        }
    }
}
