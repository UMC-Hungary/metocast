//! Read models for the PowerPoint file, folder and slide-generation endpoints.
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// The PPT endpoints wrap their result: `{"success": true, "data": …}`.
#[derive(Debug, Clone, Deserialize)]
pub struct PptEnvelope<T> {
    pub data: T,
}

/// One file from `GET /api/ppt/files`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PptFile {
    pub id: String,
    pub name: String,
    pub path: String,
    pub folder_id: String,
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
