use axum::Json;
use axum::response::{Html, IntoResponse};
use serde_json::{Value, json};

const DOCS_HTML: &str = r#"<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>Metocast API Reference</title>
  </head>
  <body>
    <script id="api-reference" data-url="/openapi.json"></script>
    <script src="https://cdn.jsdelivr.net/npm/@scalar/api-reference"></script>
  </body>
</html>"#;

pub async fn serve_spec() -> impl IntoResponse {
    Json(spec())
}

pub async fn serve_docs() -> Html<&'static str> {
    Html(DOCS_HTML)
}

pub fn spec() -> Value {
    json!({
        "openapi": "3.1.0",
        "info": {
            "title": "Metocast API",
            "version": env!("CARGO_PKG_VERSION"),
            "description": "REST API and WebSocket interface for the Metocast desktop application.\n\n## Authentication\n\nAll `/api/*` endpoints require a **Bearer token** in the `Authorization` header:\n```\nAuthorization: Bearer <token>\n```\nThe token is displayed in the app's *Connection Guide* screen. It rotates on every server restart.\n\n## WebSocket — real-time push stream\n\n> **Note:** WebSocket is not an HTTP operation and cannot be tested from this page. Use a WebSocket client (e.g. [Hoppscotch](https://hoppscotch.io), [websocat](https://github.com/vi/websocat), or Bruno's socket type).\n\n**Endpoint:** `ws://<host>/ws?token=<token>`\n\nAuthentication uses the same bearer token passed as a **query parameter** (headers are not available during the WebSocket handshake).\n\n### Initial messages (sent immediately on connect)\n\n```json\n{ \"type\": \"connected\", \"serverId\": \"<uuid>\" }\n{ \"type\": \"connector.status\", \"connector\": \"obs\",  \"status\": { \"type\": \"connected\" } }\n{ \"type\": \"connector.status\", \"connector\": \"vmix\", \"status\": { \"type\": \"disconnected\" } }\n```\n\n### Push messages (broadcast on change)\n\n| `type` | Trigger | Schema |\n|---|---|---|\n| `connector.status` | OBS or VMix connection state changes | `WsConnectorStatusMessage` |\n| `event.changed` | Event created, updated, or deleted | `WsEventChangedMessage` |\n| `recording.changed` | Recording created or updated | `WsRecordingChangedMessage` |\n\n```json\n{ \"type\": \"connector.status\", \"connector\": \"obs\", \"status\": { \"type\": \"error\", \"message\": \"connection refused\" } }\n{ \"type\": \"event.changed\",     \"data\": { \"operation\": \"INSERT\", \"record\": { ...Event } } }\n{ \"type\": \"recording.changed\", \"data\": { \"operation\": \"UPDATE\", \"record\": { ...Recording } } }\n```\n\nFull payload definitions are in the `Ws*Message` schemas below."
        },
        "servers": [
            {
                "url": "/",
                "description": "Current server — replace host and port as needed (default port: 3737)"
            }
        ],
        "security": [
            { "bearerAuth": [] }
        ],
        "tags": [
            { "name": "Events",     "description": "Service events" },
            { "name": "Recordings", "description": "Video recording files linked to events" },
            { "name": "Connectors", "description": "Connector status, configuration and control" },
            { "name": "Bible",      "description": "Bible passage lookups and reference autocomplete" },
            { "name": "Presenter",  "description": "Web presenter — parse .pptx files and push slide changes to all connected browsers" },
            { "name": "WebSocket",  "description": "Real-time push stream — requires a WebSocket client, not HTTP" }
        ],
        "components": {
            "securitySchemes": {
                "bearerAuth": {
                    "type": "http",
                    "scheme": "bearer",
                    "description": "Token shown in the app Connection Guide. Rotates on every server restart."
                }
            },
            "schemas": {
                "SlideContent": {
                    "type": "object",
                    "description": "Text content extracted from a single slide.",
                    "required": ["index", "paragraphs"],
                    "properties": {
                        "index": { "type": "integer", "minimum": 1, "description": "1-based slide number" },
                        "paragraphs": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "required": ["lines", "align", "fontSizePt"],
                                "properties": {
                                    "lines": { "type": "array", "items": { "type": "string" } },
                                    "align": { "type": "string", "enum": ["left", "center", "right", "justify"] },
                                    "fontSizePt": { "type": "number" }
                                }
                            }
                        }
                    }
                },
                "SvgSlideContent": {
                    "type": "object",
                    "description": "Self-contained SVG rendering of a single slide.",
                    "required": ["index", "svg", "widthPx", "heightPx"],
                    "properties": {
                        "index": { "type": "integer", "minimum": 1 },
                        "svg": { "type": "string", "description": "Inline SVG document. Embedded images are data URIs." },
                        "widthPx": { "type": "integer", "minimum": 1 },
                        "heightPx": { "type": "integer", "minimum": 1 }
                    }
                },
                "PresenterTheme": {
                    "type": "string",
                    "enum": ["classic", "editorial"],
                    "description": "Persisted design returned as presenterTheme in presentation.settings and changed with presentation.set_presenter_theme. Classic shows source SVG whenever it is available; Editorial renders the extracted song or Bible text in the shared warm-black design."
                },
                "PresenterState": {
                    "type": "object",
                    "required": ["loaded", "filePath", "currentSlide", "totalSlides", "renderMode", "slides", "svgSlides", "muted"],
                    "properties": {
                        "loaded": { "type": "boolean" },
                        "filePath": { "type": ["string", "null"] },
                        "currentSlide": { "type": "integer", "minimum": 0 },
                        "totalSlides": { "type": "integer", "minimum": 0 },
                        "renderMode": { "type": "string", "enum": ["text", "svg"] },
                        "slides": {
                            "type": "array",
                            "items": { "$ref": "#/components/schemas/SlideContent" },
                            "description": "Text slides for text mode and fallback/editing data."
                        },
                        "svgSlides": {
                            "type": "array",
                            "items": { "$ref": "#/components/schemas/SvgSlideContent" },
                            "description": "SVG slides when renderMode is svg."
                        },
                        "muted": { "type": "boolean" },
                        "slideWidthEmu": { "type": "integer" },
                        "slideHeightEmu": { "type": "integer" }
                    }
                },
                "ParsedPresentation": {
                    "type": "object",
                    "description": "Structured text content extracted from a parsed .pptx file.",
                    "required": ["filePath", "totalSlides", "slides", "slideWidthEmu", "slideHeightEmu"],
                    "properties": {
                        "filePath":    { "type": "string", "example": "/Users/admin/Presentations/sunday-service.pptx" },
                        "totalSlides": { "type": "integer", "minimum": 0 },
                        "slides": {
                            "type": "array",
                            "items": { "$ref": "#/components/schemas/SlideContent" }
                        },
                        "slideWidthEmu": { "type": "integer" },
                        "slideHeightEmu": { "type": "integer" }
                    }
                },
                "BibleReference": {
                    "type": "object",
                    "required": ["type", "reference", "translation", "verses"],
                    "properties": {
                        "type":        { "type": "string", "enum": ["textus", "leckio"] },
                        "reference":   { "type": "string", "example": "John 3:16" },
                        "translation": { "type": "string", "example": "UF" },
                        "verses": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "required": ["chapter", "verse", "text"],
                                "properties": {
                                    "chapter": { "type": "integer" },
                                    "verse":   { "type": "integer" },
                                    "text":    { "type": "string" }
                                }
                            }
                        }
                    }
                },
                "TitleTemplate": {
                    "type": "object",
                    "description": "Template for the published event title. Placeholders are `{name}`; `{date}` takes an optional pipe (`{date|YYYY.MM.DD.}`). Text in `[ ... ]` is dropped whole when a placeholder inside it is empty.",
                    "required": ["template"],
                    "properties": {
                        "template": {
                            "type": "string",
                            "example": "{date|YYYY.MM.DD.} {title}[ | Textus: {textus}][ Lekció: {leckio}][ | {speaker}]"
                        }
                    }
                },
                "SlideFolder": {
                    "type": "object",
                    "description": "A generated-slide output folder on the core's machine. Empty means not configured.",
                    "required": ["path"],
                    "properties": {
                        "path": { "type": "string", "example": "/Users/me/Documents/Slides" }
                    }
                },
                "Event": {
                    "type": "object",
                    "description": "Full event record including platform connections and bible references.",
                    "required": [
                        "id", "title", "dateTime", "speaker", "description",
                        "autoUploadEnabled", "connections", "bibleReferences", "createdAt", "updatedAt"
                    ],
                    "properties": {
                        "id":               { "type": "string", "format": "uuid" },
                        "title":            { "type": "string", "example": "Sunday Morning Service" },
                        "computedTitle":    { "type": "string", "description": "Composed \"date · reference · title · — speaker\" line published to YouTube. Empty falls back to title.", "example": "Sun, September 6, 2026 · 2Kor 1,1-10 · Worship Service — Pastor Smith" },
                        "dateTime":         { "type": "string", "format": "date-time", "description": "Scheduled date and time (ISO 8601 / UTC)" },
                        "speaker":          { "type": "string", "example": "Pastor Smith" },
                        "description":      { "type": "string" },
                        "autoUploadEnabled":{ "type": "boolean" },
                        "connections":      { "type": "array", "items": { "$ref": "#/components/schemas/EventConnection" } },
                        "bibleReferences":  { "type": "array", "items": { "$ref": "#/components/schemas/BibleReference" } },
                        "createdAt":        { "type": "string", "format": "date-time" },
                        "updatedAt":        { "type": "string", "format": "date-time" }
                    }
                },
                "EventSummary": {
                    "type": "object",
                    "description": "Lightweight event entry returned by the list endpoint. Omits large text fields; adds a recording count.",
                    "required": ["id", "title", "dateTime", "speaker", "recordingCount", "createdAt", "updatedAt"],
                    "properties": {
                        "id":             { "type": "string", "format": "uuid" },
                        "title":          { "type": "string", "example": "Sunday Morning Service" },
                        "computedTitle":  { "type": "string", "description": "Composed title published to YouTube. Empty falls back to title." },
                        "dateTime":       { "type": "string", "format": "date-time" },
                        "speaker":        { "type": "string" },
                        "recordingCount": { "type": "integer", "format": "int64", "description": "Number of recording files attached to this event" },
                        "createdAt":      { "type": "string", "format": "date-time" },
                        "updatedAt":      { "type": "string", "format": "date-time" }
                    }
                },
                "CreateEventRequest": {
                    "type": "object",
                    "description": "Request body for creating or fully replacing an event. Field names are **snake_case**.",
                    "required": ["title", "date_time"],
                    "properties": {
                        "title":             { "type": "string", "example": "Sunday Morning Service" },
                        "computed_title":    { "type": "string", "default": "", "description": "Composed title to publish to YouTube. Omit or leave empty to publish `title` instead." },
                        "date_time":         { "type": "string", "format": "date-time", "description": "Scheduled date and time" },
                        "speaker":           { "type": "string", "default": "" },
                        "description":       { "type": "string", "default": "" },
                        "auto_upload_enabled": { "type": "boolean", "default": false },
                        "bible_references":  {
                            "type": "array",
                            "description": "Bible readings for the event. Each entry has a type (textus or leckio). An empty reference string removes the existing entry.",
                            "items": {
                                "type": "object",
                                "required": ["type"],
                                "properties": {
                                    "type":        { "type": "string", "enum": ["textus", "leckio"] },
                                    "reference":   { "type": "string" },
                                    "translation": { "type": "string", "default": "UF" },
                                    "verses":      { "type": "array", "items": { "$ref": "#/components/schemas/BibleReference/properties/verses/items" } }
                                }
                            }
                        },
                        "connections": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "required": ["platform"],
                                "properties": {
                                    "platform":       { "type": "string" },
                                    "privacy_status": { "type": "string" }
                                }
                            }
                        }
                    }
                },
                "Recording": {
                    "type": "object",
                    "description": "Video or audio recording file linked to an event.",
                    "required": [
                        "id", "eventId", "filePath", "fileName", "fileSize",
                        "durationSeconds", "mediaKind", "source", "metadata", "detectedAt", "whitelisted", "uploaded",
                        "createdAt", "updatedAt"
                    ],
                    "properties": {
                        "id":              { "type": "string", "format": "uuid" },
                        "eventId":         { "type": "string", "format": "uuid" },
                        "filePath":        { "type": "string", "description": "Absolute path on the server's filesystem" },
                        "fileName":        { "type": "string", "example": "service-2025-01-19.mp4" },
                        "fileSize":        { "type": "integer", "format": "int64", "description": "File size in bytes" },
                        "durationSeconds": { "type": "number",  "format": "double",  "description": "Duration in seconds" },
                        "mediaKind":      { "type": "string", "description": "Media kind such as video, deviceMix, deviceTrack, derivedMix, or deviceMultichannel" },
                        "source":         { "type": "string", "description": "Originating recorder, such as obs or rodecaster" },
                        "metadata":       { "type": "object", "additionalProperties": true, "description": "Recorder-specific source mapping and provenance snapshot" },
                        "detectedAt":      { "type": "string",  "format": "date-time", "description": "When the file was detected or added" },
                        "whitelisted":     { "type": "boolean", "description": "Approved for YouTube upload" },
                        "uploaded":        { "type": "boolean", "description": "Whether the file has been uploaded to YouTube" },
                        "uploadedAt":      { "type": ["string", "null"], "format": "date-time" },
                        "videoId":         { "type": ["string", "null"], "description": "YouTube video ID (set after upload)" },
                        "videoUrl":        { "type": ["string", "null"], "description": "YouTube watch URL (set after upload)" },
                        "customTitle":     { "type": ["string", "null"], "description": "Custom YouTube title; falls back to the event title when null" },
                        "createdAt":       { "type": "string", "format": "date-time" },
                        "updatedAt":       { "type": "string", "format": "date-time" }
                    }
                },
                "CreateRecordingRequest": {
                    "type": "object",
                    "description": "Request body for registering a new recording. Field names are **snake_case**.",
                    "required": ["file_path", "file_name"],
                    "properties": {
                        "file_path":        { "type": "string", "description": "Absolute path to the recording file" },
                        "file_name":        { "type": "string", "example": "service-2025-01-19.mp4" },
                        "file_size":        { "type": "integer", "format": "int64", "default": 0, "description": "File size in bytes" },
                        "duration_seconds": { "type": "number",  "format": "double",  "default": 0.0 },
                        "custom_title":     { "type": "string",  "description": "Optional custom YouTube title" }
                    }
                },
                "ConnectorStatus": {
                    "description": "Discriminated union representing the current connection state of a streaming connector. Discriminator field: `type`.",
                    "oneOf": [
                        {
                            "type": "object",
                            "title": "Disconnected",
                            "required": ["type"],
                            "properties": {
                                "type": { "type": "string", "enum": ["disconnected"] }
                            }
                        },
                        {
                            "type": "object",
                            "title": "Connecting",
                            "description": "A connection attempt is in progress.",
                            "required": ["type"],
                            "properties": {
                                "type": { "type": "string", "enum": ["connecting"] }
                            }
                        },
                        {
                            "type": "object",
                            "title": "Connected",
                            "required": ["type"],
                            "properties": {
                                "type": { "type": "string", "enum": ["connected"] }
                            }
                        },
                        {
                            "type": "object",
                            "title": "Error",
                            "description": "The last connection attempt failed.",
                            "required": ["type", "message"],
                            "properties": {
                                "type":    { "type": "string", "enum": ["error"] },
                                "message": { "type": "string", "description": "Human-readable error description" }
                            }
                        }
                    ],
                    "discriminator": {
                        "propertyName": "type"
                    }
                },
                "ConnectorStatuses": {
                    "type": "object",
                    "description": "Current status of all connectors.",
                    "required": ["obs", "blackmagic-camera", "middlecontrol", "rodecaster", "vmix", "atem", "broadlink", "youtube", "facebook", "discord", "szentiras"],
                    "properties": {
                        "obs":       { "$ref": "#/components/schemas/ConnectorStatus" },
                        "blackmagic-camera": { "$ref": "#/components/schemas/ConnectorStatus" },
                        "middlecontrol": { "$ref": "#/components/schemas/ConnectorStatus" },
                        "rodecaster": { "$ref": "#/components/schemas/ConnectorStatus" },
                        "vmix":      { "$ref": "#/components/schemas/ConnectorStatus" },
                        "atem":      { "$ref": "#/components/schemas/ConnectorStatus" },
                        "broadlink": { "$ref": "#/components/schemas/ConnectorStatus" },
                        "youtube":   { "$ref": "#/components/schemas/ConnectorStatus" },
                        "facebook":  { "$ref": "#/components/schemas/ConnectorStatus" },
                        "discord":   { "$ref": "#/components/schemas/ConnectorStatus" },
                        "szentiras": { "$ref": "#/components/schemas/ConnectorStatus" }
                    }
                },
                "BiblePassage": {
                    "type": "object",
                    "description": "A Bible passage normalised across both upstream APIs.",
                    "required": ["label", "verses"],
                    "properties": {
                        "label": { "type": "string", "example": "János 3,16" },
                        "verses": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "required": ["chapter", "verse", "text"],
                                "properties": {
                                    "chapter": { "type": "integer" },
                                    "verse":   { "type": "integer" },
                                    "text":    { "type": "string", "description": "Verse text with all markup stripped" }
                                }
                            }
                        }
                    }
                },
                "BibleSuggestion": {
                    "type": "object",
                    "required": ["cat", "label", "link"],
                    "properties": {
                        "cat":   { "type": "string", "example": "ref" },
                        "label": { "type": "string", "example": "1Móz 1" },
                        "link":  { "type": "string", "description": "Reference to pass back as the `reference` parameter" }
                    }
                },
                "ConnectorConfig": {
                    "description": "Configuration for one connector. The shape depends on the connector: `obs` (enabled, host, port, password), `blackmagic-camera` (enabled, host, fingerprint, username, password), `middlecontrol` (enabled, host, port; default 11584), `vmix`/`atem` (enabled, host, port), `broadlink` (enabled), `rodecaster` (enabled, notifyOnMute, audioRecording), `youtube` (enabled, clientId, clientSecret), `facebook` (enabled, appId, appSecret, pageId), `discord` (enabled, webhookUrl), `szentiras` (enabled, apiKey).",
                    "type": "object",
                    "required": ["enabled"],
                    "properties": {
                        "enabled":      { "type": "boolean" },
                        "notifyOnMute": { "type": "boolean", "description": "RODECaster: raise a notification when a channel mute state changes on the device." },
                        "audioRecording": { "type": "object", "description": "RØDECaster FLAC recording configuration: schemaVersion, enabled, server directory, processingMode, outputMode, and typed sources." },
                        "host":         { "type": "string" },
                        "port":         { "type": "integer" },
                        "fingerprint":  { "type": "string", "description": "Blackmagic camera: pinned SHA-256 of the camera's self-signed certificate. Blank means trust-on-first-use." },
                        "username":     { "type": "string" },
                        "password":     { "type": "string", "nullable": true, "description": "Write-only: reads return an empty string" },
                        "clientId":     { "type": "string" },
                        "clientSecret": { "type": "string", "description": "Write-only: reads return an empty string" },
                        "appId":        { "type": "string" },
                        "appSecret":    { "type": "string", "description": "Write-only: reads return an empty string" },
                        "pageId":       { "type": "string" },
                        "webhookUrl":   { "type": "string", "description": "Write-only: reads return an empty string" },
                        "apiKey":       { "type": "string", "description": "szentiras.eu API key, sent upstream as X-API-Key. Write-only: reads return an empty string." },
                        "passwordSet":    { "type": "boolean", "readOnly": true, "description": "Whether a password is stored" },
                        "clientSecretSet":{ "type": "boolean", "readOnly": true, "description": "Whether a client secret is stored" },
                        "appSecretSet":   { "type": "boolean", "readOnly": true, "description": "Whether an app secret is stored" },
                        "webhookUrlSet":  { "type": "boolean", "readOnly": true, "description": "Whether a webhook URL is stored" },
                        "apiKeySet":      { "type": "boolean", "readOnly": true, "description": "Whether an API key is stored. Send false to clear it." }
                    }
                },
                "CameraStreamTarget": {
                    "type": "object",
                    "description": "The camera's livestream destination after YouTube settings were pushed.",
                    "required": ["rtmpUrl", "platform", "server", "quality"],
                    "properties": {
                        "rtmpUrl":  { "type": "string", "description": "Ingestion address and stream key joined — what the camera pushes to", "example": "rtmp://a.rtmp.youtube.com/live2/abcd-1234-efgh-5678" },
                        "platform": { "type": "string", "example": "YouTube" },
                        "server":   { "type": "string", "description": "The platform's server name, or `Custom` when the URL was overridden" },
                        "quality":  { "type": "string", "description": "Quality profile the camera will encode at" },
                        "url":      { "type": "string", "nullable": true, "description": "Set only when the destination is a custom URL rather than a platform entry" }
                    }
                },
                "DiscoveredAtem": {
                    "type": "object",
                    "description": "An ATEM switcher advertising `_switcher_ctrl._udp` over mDNS, once per address it answered on.",
                    "required": ["name", "host", "port", "usb"],
                    "properties": {
                        "name": { "type": "string", "description": "The switcher's name as set in ATEM Setup", "example": "MetoDeck" },
                        "host": { "type": "string", "example": "172.25.69.185" },
                        "port": { "type": "integer", "minimum": 1, "maximum": 65535, "example": 9910 },
                        "usb":  { "type": "boolean", "description": "True when the address is on the switcher's own USB network link (a /30 USB Ethernet adapter on the server), false on the LAN" }
                    }
                },
                "AtemStreamTarget": {
                    "type": "object",
                    "description": "The switcher's streaming service after YouTube settings were pushed. The stream key is written to the switcher but never returned.",
                    "required": ["service", "url"],
                    "properties": {
                        "service": { "type": "string", "example": "YouTube RTMP" },
                        "url":     { "type": "string", "example": "rtmp://a.rtmp.youtube.com/live2" }
                    }
                },
                "CameraResolution": {
                    "type": "object",
                    "required": ["width", "height"],
                    "properties": {
                        "width":  { "type": "integer", "example": 6048 },
                        "height": { "type": "integer", "example": 4032 }
                    }
                },
                "CameraPlatformEntry": {
                    "type": "object",
                    "description": "The camera's active livestream platform entry.",
                    "required": ["platform", "server", "quality"],
                    "properties": {
                        "platform":   { "type": "string", "example": "YouTube RTMP" },
                        "server":     { "type": "string", "example": "Primary" },
                        "quality":    { "type": "string", "example": "Streaming High" },
                        "key":        { "type": "string", "nullable": true, "description": "Stream key" },
                        "passphrase": { "type": "string", "nullable": true, "description": "SRT streams only" },
                        "url":        { "type": "string", "nullable": true, "description": "Set only when the platform's URL is customizable" }
                    }
                },
                "CameraSettings": {
                    "type": "object",
                    "description": "The camera's own payloads, forwarded unchanged: record format, media working set and livestream settings.",
                    "required": ["recording", "record", "storage", "stream"],
                    "properties": {
                        "recording": { "type": "boolean", "description": "Whether the camera is recording now" },
                        "record": {
                            "type": "object",
                            "description": "`/system/format` and `/system/supportedFormats` as the camera returns them",
                            "required": ["format", "supported"],
                            "properties": {
                                "format":    { "type": "object", "additionalProperties": true },
                                "supported": { "type": "object", "additionalProperties": true }
                            }
                        },
                        "storage": {
                            "type": "object",
                            "description": "`/media/slots`, `/media/workingset` and `/media/active`",
                            "required": ["slots", "workingset", "active"],
                            "properties": {
                                "slots":      { "type": "array", "items": { "type": "object", "additionalProperties": true } },
                                "workingset": { "type": "object", "additionalProperties": true },
                                "active":     { "type": "object", "nullable": true, "additionalProperties": true }
                            }
                        },
                        "stream": {
                            "type": "object",
                            "description": "Livestream status, availability, the platform list, the active platform and that platform's servers and quality profiles",
                            "required": ["status", "available", "platforms", "active", "platform"],
                            "properties": {
                                "status":    { "type": "object", "additionalProperties": true },
                                "available": { "type": "object", "additionalProperties": true },
                                "platforms": { "type": "array", "items": { "type": "string" } },
                                "active":    { "$ref": "#/components/schemas/CameraPlatformEntry" },
                                "platform":  { "type": "object", "additionalProperties": true }
                            }
                        }
                    }
                },
                "CameraSettingsUpdate": {
                    "type": "object",
                    "description": "What to write. Both halves are optional; whichever is present is sent to the camera.",
                    "properties": {
                        "record": {
                            "type": "object",
                            "description": "The camera validates a format as a whole, so all four fields travel together.",
                            "required": ["codec", "frameRate", "recordResolution", "sensorResolution"],
                            "properties": {
                                "codec":            { "type": "string", "example": "BRaw:8_1" },
                                "frameRate":        { "type": "string", "example": "24" },
                                "recordResolution": { "$ref": "#/components/schemas/CameraResolution" },
                                "sensorResolution": { "$ref": "#/components/schemas/CameraResolution" }
                            }
                        },
                        "stream": { "$ref": "#/components/schemas/CameraPlatformEntry" }
                    }
                },
                "DiscoveredCamera": {
                    "type": "object",
                    "description": "A Blackmagic camera found by mDNS. `host` is what the connector config's `host` field takes.",
                    "required": ["host", "hostname", "addresses", "port", "deviceName", "productName", "uniqueId", "softwareVersion"],
                    "properties": {
                        "host":            { "type": "string", "example": "http://Cinema-Camera-6K.local" },
                        "hostname":        { "type": "string", "example": "Cinema-Camera-6K.local." },
                        "addresses":       { "type": "array", "items": { "type": "string" } },
                        "port":            { "type": "integer" },
                        "deviceName":      { "type": "string" },
                        "productName":     { "type": "string", "example": "Cinema Camera 6K" },
                        "uniqueId":        { "type": "string", "description": "Stable across renames and DHCP" },
                        "softwareVersion": { "type": "string", "example": "10.2.2" }
                    }
                },
                "DiscoveredMiddlecontrol": {
                    "type": "object",
                    "description": "A Middle Control endpoint that sent a valid feedback frame during discovery.",
                    "required": ["host", "port"],
                    "properties": {
                        "host": { "type": "string", "example": "localhost" },
                        "port": { "type": "integer", "minimum": 1, "maximum": 65535, "example": 11584 }
                    }
                },
                "ObsStreamSettings": {
                    "type": "object",
                    "description": "The RTMP destination OBS streams to.",
                    "required": ["server", "key"],
                    "properties": {
                        "serviceType": { "type": "string", "description": "OBS service type, e.g. `rtmp_custom` (response only)" },
                        "server":      { "type": "string", "example": "rtmp://a.rtmp.youtube.com/live2" },
                        "key":         { "type": "string" }
                    }
                },
                "WsConnectedMessage": {
                    "type": "object",
                    "description": "Sent once immediately after a WebSocket connection is established.",
                    "required": ["type", "serverId"],
                    "properties": {
                        "type":     { "type": "string", "enum": ["connected"] },
                        "serverId": { "type": "string", "format": "uuid", "description": "Unique server ID — regenerated on every server restart" }
                    }
                },
                "WsConnectorStatusMessage": {
                    "type": "object",
                    "description": "Pushed on connect (initial snapshot) and whenever a connector's state changes.",
                    "required": ["type", "connector", "status"],
                    "properties": {
                        "type":      { "type": "string", "enum": ["connector.status"] },
                        "connector": { "type": "string", "enum": ["obs", "vmix", "middlecontrol"] },
                        "status":    { "$ref": "#/components/schemas/ConnectorStatus" }
                    }
                },
                "WsEventChangedMessage": {
                    "type": "object",
                    "description": "Broadcast when an event row is inserted, updated, or deleted.",
                    "required": ["type", "data"],
                    "properties": {
                        "type": { "type": "string", "enum": ["event.changed"] },
                        "data": {
                            "type": "object",
                            "required": ["operation", "record"],
                            "properties": {
                                "operation": { "type": "string", "enum": ["INSERT", "UPDATE", "DELETE"] },
                                "record":    { "$ref": "#/components/schemas/Event" }
                            }
                        }
                    }
                },
                "WsRecordingChangedMessage": {
                    "type": "object",
                    "description": "Broadcast when a recording row is inserted or updated.",
                    "required": ["type", "data"],
                    "properties": {
                        "type": { "type": "string", "enum": ["recording.changed"] },
                        "data": {
                            "type": "object",
                            "required": ["operation", "record"],
                            "properties": {
                                "operation": { "type": "string", "enum": ["INSERT", "UPDATE", "DELETE"] },
                                "record":    { "$ref": "#/components/schemas/Recording" }
                            }
                        }
                    }
                }
            }
        },
        "paths": {
            "/api/logs": {
                "get": {
                    "tags": ["Settings"],
                    "summary": "Read the core's application log",
                    "description": "Returns the tail of the log file this core writes, plus its path on the host. Lets a client device read the server's log rather than its own.",
                    "operationId": "getApplicationLog",
                    "responses": {
                        "200": {
                            "description": "Log path and contents",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "type": "object",
                                        "required": ["path", "content"],
                                        "properties": {
                                            "path": { "type": "string" },
                                            "content": { "type": "string" }
                                        }
                                    }
                                }
                            }
                        },
                        "401": { "description": "Unauthorized — missing or invalid token" },
                        "503": { "description": "This core has no application log" }
                    }
                },
                "delete": {
                    "tags": ["Settings"],
                    "summary": "Clear the core's application log",
                    "operationId": "clearApplicationLog",
                    "responses": {
                        "204": { "description": "Log cleared" },
                        "401": { "description": "Unauthorized — missing or invalid token" },
                        "503": { "description": "This core has no application log" }
                    }
                }
            },
            "/api/settings/title-template": {
                "get": {
                    "tags": ["Settings"],
                    "summary": "Get the event title template",
                    "description": "The template the editor renders `computedTitle` from. Never 404s — an unset key returns the built-in default.",
                    "operationId": "getTitleTemplate",
                    "responses": {
                        "200": {
                            "description": "The stored template, or the default when unset",
                            "content": {
                                "application/json": {
                                    "schema": { "$ref": "#/components/schemas/TitleTemplate" }
                                }
                            }
                        },
                        "401": { "description": "Unauthorized — missing or invalid token" }
                    }
                },
                "put": {
                    "tags": ["Settings"],
                    "summary": "Replace the event title template",
                    "operationId": "setTitleTemplate",
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": { "$ref": "#/components/schemas/TitleTemplate" }
                            }
                        }
                    },
                    "responses": {
                        "200": {
                            "description": "The stored template",
                            "content": {
                                "application/json": {
                                    "schema": { "$ref": "#/components/schemas/TitleTemplate" }
                                }
                            }
                        },
                        "401": { "description": "Unauthorized — missing or invalid token" },
                        "500": { "description": "Database error" }
                    }
                }
            },
            "/api/settings/slide-folder": {
                "get": {
                    "tags": ["Settings"],
                    "summary": "Get the Bible slide output folder",
                    "description": "Never 404s — an unset key returns an empty path.",
                    "operationId": "getSlideFolder",
                    "responses": {
                        "200": {
                            "description": "The stored folder, or an empty path when unset",
                            "content": {
                                "application/json": {
                                    "schema": { "$ref": "#/components/schemas/SlideFolder" }
                                }
                            }
                        },
                        "401": { "description": "Unauthorized — missing or invalid token" }
                    }
                },
                "put": {
                    "tags": ["Settings"],
                    "summary": "Set the Bible slide output folder",
                    "description": "The core checks the folder exists on its own machine; an empty path clears the setting.",
                    "operationId": "setSlideFolder",
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": { "$ref": "#/components/schemas/SlideFolder" }
                            }
                        }
                    },
                    "responses": {
                        "200": {
                            "description": "The stored folder",
                            "content": {
                                "application/json": {
                                    "schema": { "$ref": "#/components/schemas/SlideFolder" }
                                }
                            }
                        },
                        "400": { "description": "The folder does not exist on the core's machine" },
                        "401": { "description": "Unauthorized — missing or invalid token" },
                        "500": { "description": "Database error" }
                    }
                }
            },
            "/api/settings/song-slide-folder": {
                "get": {
                    "tags": ["Settings"],
                    "summary": "Get the song slide output folder",
                    "description": "Never 404s — an unset key returns an empty path.",
                    "operationId": "getSongSlideFolder",
                    "responses": {
                        "200": {
                            "description": "The stored folder, or an empty path when unset",
                            "content": {
                                "application/json": {
                                    "schema": { "$ref": "#/components/schemas/SlideFolder" }
                                }
                            }
                        },
                        "401": { "description": "Unauthorized — missing or invalid token" }
                    }
                },
                "put": {
                    "tags": ["Settings"],
                    "summary": "Set the song slide output folder",
                    "description": "The core checks the folder exists on its own machine; an empty path clears the setting.",
                    "operationId": "setSongSlideFolder",
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": { "$ref": "#/components/schemas/SlideFolder" }
                            }
                        }
                    },
                    "responses": {
                        "200": {
                            "description": "The stored folder",
                            "content": {
                                "application/json": {
                                    "schema": { "$ref": "#/components/schemas/SlideFolder" }
                                }
                            }
                        },
                        "400": { "description": "The folder does not exist on the core's machine" },
                        "401": { "description": "Unauthorized — missing or invalid token" },
                        "500": { "description": "Database error" }
                    }
                }
            },
            "/api/ppt/song": {
                "post": {
                    "tags": ["Presentations"],
                    "summary": "Create a song PowerPoint",
                    "description": "Splits pasted lyrics only at two empty lines, preserves every entered line without wrapping it onto another line, adds a title slide and a final blank slide, and writes the `.pptx` into the configured song slide output folder.",
                    "operationId": "createSongPresentation",
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": {
                                    "type": "object",
                                    "required": ["title", "lyrics"],
                                    "properties": {
                                        "title": { "type": "string", "maxLength": 200 },
                                        "lyrics": { "type": "string", "maxLength": 50000 }
                                    }
                                }
                            }
                        }
                    },
                    "responses": {
                        "200": {
                            "description": "The written file and slide count",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "type": "object",
                                        "required": ["filePath", "slideCount"],
                                        "properties": {
                                            "filePath": { "type": "string" },
                                            "slideCount": { "type": "integer", "minimum": 1 }
                                        }
                                    }
                                }
                            }
                        },
                        "400": { "description": "Missing content or no song slide output folder configured" },
                        "401": { "description": "Unauthorized — missing or invalid token" },
                        "500": { "description": "Could not write the deck" }
                    }
                }
            },
            "/api/events/{id}/slides": {
                "post": {
                    "tags": ["Events"],
                    "summary": "Generate Bible slide decks for an event",
                    "description": "Writes one `.pptx` per Bible reference into the configured slide folder, paginated the same way the web presenter shows them. Names are fixed — `textus.pptx` and `lekcio.pptx` — so regenerating replaces the previous deck.",
                    "operationId": "createEventSlides",
                    "parameters": [
                        {
                            "name": "id",
                            "in": "path",
                            "required": true,
                            "schema": { "type": "string", "format": "uuid" }
                        }
                    ],
                    "responses": {
                        "200": {
                            "description": "Paths of the written files",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "type": "object",
                                        "required": ["files"],
                                        "properties": {
                                            "files": { "type": "array", "items": { "type": "string" } }
                                        }
                                    }
                                }
                            }
                        },
                        "400": { "description": "No slide folder configured, or the event has no Bible references with verses" },
                        "401": { "description": "Unauthorized — missing or invalid token" },
                        "404": { "description": "Event not found" },
                        "500": { "description": "Could not write the deck" }
                    }
                }
            },
            "/api/events": {
                "get": {
                    "tags": ["Events"],
                    "summary": "List events",
                    "description": "Returns all events ordered by date (newest first). Each item includes a recording count but omits large text fields — use *Get event* to fetch the full record.",
                    "operationId": "listEvents",
                    "responses": {
                        "200": {
                            "description": "Array of event summaries",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "type": "array",
                                        "items": { "$ref": "#/components/schemas/EventSummary" }
                                    }
                                }
                            }
                        },
                        "401": { "description": "Unauthorized — missing or invalid token" },
                        "500": { "description": "Database error" }
                    }
                },
                "post": {
                    "tags": ["Events"],
                    "summary": "Create event",
                    "operationId": "createEvent",
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": { "$ref": "#/components/schemas/CreateEventRequest" },
                                "example": {
                                    "title": "Sunday Service",
                                    "date_time": "2025-01-19T10:00:00Z",
                                    "speaker": "Pastor Smith",
                                    "bible_references": [
                                        { "type": "textus", "reference": "John 3:16", "translation": "UF" }
                                    ]
                                }
                            }
                        }
                    },
                    "responses": {
                        "201": {
                            "description": "Event created — returns the full event record",
                            "content": {
                                "application/json": {
                                    "schema": { "$ref": "#/components/schemas/Event" }
                                }
                            }
                        },
                        "401": { "description": "Unauthorized" },
                        "500": { "description": "Database error" }
                    }
                }
            },
            "/api/events/{id}": {
                "parameters": [
                    {
                        "name": "id",
                        "in": "path",
                        "required": true,
                        "description": "Event UUID",
                        "schema": { "type": "string", "format": "uuid" }
                    }
                ],
                "get": {
                    "tags": ["Events"],
                    "summary": "Get event",
                    "description": "Returns the complete event record including all text fields.",
                    "operationId": "getEvent",
                    "responses": {
                        "200": {
                            "description": "Full event record",
                            "content": {
                                "application/json": {
                                    "schema": { "$ref": "#/components/schemas/Event" }
                                }
                            }
                        },
                        "401": { "description": "Unauthorized" },
                        "404": { "description": "Event not found" },
                        "500": { "description": "Database error" }
                    }
                },
                "put": {
                    "tags": ["Events"],
                    "summary": "Update event",
                    "description": "Replaces all fields of an existing event. This is a full replacement — omitted optional fields revert to their defaults (empty string, `\"UF\"`, `\"private\"`, `false`).",
                    "operationId": "updateEvent",
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": { "$ref": "#/components/schemas/CreateEventRequest" }
                            }
                        }
                    },
                    "responses": {
                        "200": {
                            "description": "Updated event record",
                            "content": {
                                "application/json": {
                                    "schema": { "$ref": "#/components/schemas/Event" }
                                }
                            }
                        },
                        "401": { "description": "Unauthorized" },
                        "404": { "description": "Event not found" },
                        "500": { "description": "Database error" }
                    }
                }
            },
            "/api/events/{id}/recordings": {
                "parameters": [
                    {
                        "name": "id",
                        "in": "path",
                        "required": true,
                        "description": "Event UUID",
                        "schema": { "type": "string", "format": "uuid" }
                    }
                ],
                "get": {
                    "tags": ["Recordings"],
                    "summary": "List recordings",
                    "description": "Returns all recording files attached to an event, ordered by detection time (newest first).",
                    "operationId": "listRecordings",
                    "responses": {
                        "200": {
                            "description": "Array of recordings",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "type": "array",
                                        "items": { "$ref": "#/components/schemas/Recording" }
                                    }
                                }
                            }
                        },
                        "401": { "description": "Unauthorized" },
                        "500": { "description": "Database error" }
                    }
                },
                "post": {
                    "tags": ["Recordings"],
                    "summary": "Add recording",
                    "description": "Registers a new recording file for an event. The file must already exist on the server's filesystem.",
                    "operationId": "createRecording",
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": { "$ref": "#/components/schemas/CreateRecordingRequest" },
                                "example": {
                                    "file_path": "/recordings/service-2025-01-19.mp4",
                                    "file_name": "service-2025-01-19.mp4",
                                    "file_size": 1073741824,
                                    "duration_seconds": 3600.0
                                }
                            }
                        }
                    },
                    "responses": {
                        "201": {
                            "description": "Recording registered — returns the full recording record",
                            "content": {
                                "application/json": {
                                    "schema": { "$ref": "#/components/schemas/Recording" }
                                }
                            }
                        },
                        "401": { "description": "Unauthorized" },
                        "500": { "description": "Database error" }
                    }
                }
            },
            "/api/presenter/parse": {
                "post": {
                    "tags": ["Presenter"],
                    "summary": "Parse a .pptx file",
                    "description": "Opens a `.pptx` file from the local filesystem, extracts text content from every slide, and returns the structured data. Only Open XML (`.pptx`) format is supported; legacy binary `.ppt` files must be re-saved as `.pptx` first.",
                    "operationId": "parsePresentation",
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": {
                                    "type": "object",
                                    "required": ["filePath"],
                                    "properties": {
                                        "filePath": { "type": "string", "example": "/Users/admin/Presentations/sunday-service.pptx" }
                                    }
                                }
                            }
                        }
                    },
                    "responses": {
                        "200": {
                            "description": "Parsed presentation data",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "type": "object",
                                        "properties": {
                                            "success": { "type": "boolean", "example": true },
                                            "data": { "$ref": "#/components/schemas/ParsedPresentation" }
                                        }
                                    }
                                }
                            }
                        },
                        "401": { "description": "Unauthorized" },
                        "422": { "description": "File not found, not a valid .pptx, or no slides found" }
                    }
                }
            },
            "/api/bible/verses": {
                "get": {
                    "tags": ["Bible"],
                    "summary": "Look up a Bible passage",
                    "description": "Fetches a passage from the upstream Bible API that matches the translation (`*_v2` codes use the V2 API, everything else szentiras.eu) and returns it in one normalised shape. The core performs the upstream request, so UIs need no CORS workaround. szentiras.eu lookups send the API key from the `szentiras` connector config; without a valid key they fail with 502.",
                    "operationId": "getBiblePassage",
                    "parameters": [
                        {
                            "name": "reference",
                            "in": "query",
                            "required": true,
                            "description": "Passage reference, e.g. `Jn 3,16`",
                            "schema": { "type": "string" }
                        },
                        {
                            "name": "translation",
                            "in": "query",
                            "required": true,
                            "description": "Translation code",
                            "schema": { "type": "string", "enum": ["UF_v2", "RUF_v2", "RUF", "KG", "KNB", "SZIT", "BD", "STL"] }
                        }
                    ],
                    "responses": {
                        "200": {
                            "description": "The passage",
                            "content": {
                                "application/json": {
                                    "schema": { "$ref": "#/components/schemas/BiblePassage" }
                                }
                            }
                        },
                        "400": { "description": "Missing reference or translation" },
                        "401": { "description": "Unauthorized" },
                        "502": { "description": "The upstream Bible API failed or returned something unparseable" }
                    }
                }
            },
            "/api/bible/suggest": {
                "get": {
                    "tags": ["Bible"],
                    "summary": "Autocomplete Bible references",
                    "description": "Reference suggestions from szentiras.eu. This endpoint is public upstream, so it works without an API key. Terms shorter than 2 characters return an empty list without calling upstream.",
                    "operationId": "getBibleSuggestions",
                    "parameters": [
                        {
                            "name": "term",
                            "in": "query",
                            "required": true,
                            "description": "Partial reference the user has typed",
                            "schema": { "type": "string" }
                        }
                    ],
                    "responses": {
                        "200": {
                            "description": "Matching references",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "type": "array",
                                        "items": { "$ref": "#/components/schemas/BibleSuggestion" }
                                    }
                                }
                            }
                        },
                        "400": { "description": "Missing term" },
                        "401": { "description": "Unauthorized" },
                        "502": { "description": "The upstream Bible API failed or returned something unparseable" }
                    }
                }
            },
            "/api/connectors/status": {
                "get": {
                    "tags": ["Connectors"],
                    "summary": "Get connector statuses",
                    "description": "Returns the current connection state of every connector. Each value is a discriminated union on `type` — see the `ConnectorStatus` schema.",
                    "operationId": "getConnectorStatuses",
                    "responses": {
                        "200": {
                            "description": "Current connector statuses",
                            "content": {
                                "application/json": {
                                    "schema": { "$ref": "#/components/schemas/ConnectorStatuses" },
                                    "example": {
                                        "obs":  { "type": "connected" },
                                        "vmix": { "type": "disconnected" }
                                    }
                                }
                            }
                        },
                        "401": { "description": "Unauthorized" }
                    }
                }
            },
            "/api/connectors/{name}/config": {
                "parameters": [
                    {
                        "name": "name",
                        "in": "path",
                        "required": true,
                        "description": "Connector name",
                        "schema": { "type": "string", "enum": ["obs", "blackmagic-camera", "middlecontrol", "rodecaster", "vmix", "atem", "broadlink", "youtube", "facebook", "discord", "szentiras"] }
                    }
                ],
                "get": {
                    "tags": ["Connectors"],
                    "summary": "Get connector configuration",
                    "description": "Returns the stored configuration for one connector, or its defaults when nothing has been saved yet.\n\n**Secrets are never returned here.** `password`, `clientSecret`, `appSecret`, `apiKey` and `webhookUrl` always come back empty, with a companion boolean (`apiKeySet`, `passwordSet`, …) telling you whether one is stored. The host running the server can read them back through `/api/connectors/{name}/config/secrets`.",
                    "operationId": "getConnectorConfig",
                    "responses": {
                        "200": {
                            "description": "Stored configuration",
                            "content": {
                                "application/json": {
                                    "schema": { "$ref": "#/components/schemas/ConnectorConfig" },
                                    "example": { "enabled": true, "host": "localhost", "port": 4455, "password": null }
                                }
                            }
                        },
                        "401": { "description": "Unauthorized" },
                        "404": { "description": "Unknown connector" }
                    }
                },
                "put": {
                    "tags": ["Connectors"],
                    "summary": "Save connector configuration",
                    "description": "Persists the configuration and applies it: OBS, Blackmagic Camera, and Middle Control reconnect (or disconnect when `enabled` is false); YouTube and Facebook refresh the config used by the OAuth routes and stop when disabled.\n\n**Secret handling:** send a non-empty secret to replace it, leave it empty or omit it to keep the stored one, or send `\"<field>Set\": false` to clear it. This lets a client save a config whose secrets it was never allowed to read.",
                    "operationId": "putConnectorConfig",
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": { "$ref": "#/components/schemas/ConnectorConfig" }
                            }
                        }
                    },
                    "responses": {
                        "204": { "description": "Saved" },
                        "400": { "description": "Body does not match the connector's config shape" },
                        "401": { "description": "Unauthorized" },
                        "404": { "description": "Unknown connector" }
                    }
                }
            },
            "/api/connectors/{name}/config/secrets": {
                "get": {
                    "tags": ["Connectors"],
                    "summary": "Read a connector's stored secrets (host only)",
                    "description": "Returns the connector config **including** its secrets.\n\nRestricted to the desktop app hosting this server: it requires the normal auth token, an `X-Admin-Token` header matching the running server's admin token (regenerated every run, delivered to the host window over Tauri IPC, never over the network), and a request originating from loopback. Remote clients cannot obtain the admin token, and a leaked one is unusable off-host.\n\nConnectors that store no credentials (`vmix`, `atem`, `middlecontrol`, `broadlink`, `rodecaster`) return 204.",
                    "operationId": "revealConnectorSecrets",
                    "parameters": [
                        {
                            "name": "name",
                            "in": "path",
                            "required": true,
                            "schema": { "type": "string", "enum": ["obs", "blackmagic-camera", "youtube", "facebook", "discord", "szentiras"] }
                        },
                        {
                            "name": "X-Admin-Token",
                            "in": "header",
                            "required": true,
                            "description": "The running server's admin token",
                            "schema": { "type": "string" }
                        }
                    ],
                    "responses": {
                        "200": {
                            "description": "The config with secrets in the clear",
                            "content": {
                                "application/json": {
                                    "schema": { "$ref": "#/components/schemas/ConnectorConfig" }
                                }
                            }
                        },
                        "204": { "description": "This connector stores no secrets" },
                        "401": { "description": "Unauthorized" },
                        "403": { "description": "Missing/invalid admin token, or the request did not come from loopback" },
                        "404": { "description": "Unknown connector" }
                    }
                }
            },
            "/api/connectors/blackmagic-camera/stream/youtube": {
                "post": {
                    "tags": ["Connectors"],
                    "summary": "Push YouTube settings to the camera",
                    "description": "Copies the channel's RTMP ingestion address and stream key into the camera's livestream settings, preferring the camera's own YouTube platform entry (which needs only the key) and falling back to a custom RTMP URL on models that list no YouTube platform.\n\nSets the destination only — the camera does not go live. Requires a connected camera (409 otherwise) and a linked YouTube account.",
                    "operationId": "pushBlackmagicCameraYouTubeSettings",
                    "responses": {
                        "200": {
                            "description": "Where the camera's livestream now points",
                            "content": {
                                "application/json": {
                                    "schema": { "$ref": "#/components/schemas/CameraStreamTarget" }
                                }
                            }
                        },
                        "409": { "description": "No camera connected" },
                        "502": { "description": "The camera or the YouTube API rejected the request" }
                    }
                }
            },
            "/api/connectors/atem/discover": {
                "post": {
                    "tags": ["Connectors"],
                    "summary": "Find ATEM switchers",
                    "description": "Browses mDNS for `_switcher_ctrl._udp` for three seconds. Switchers plugged into the server by USB are found through the USB Ethernet link they create; switchers on the LAN are found on the network. The scan does not save or connect anything; put the returned host and port in the `atem` config. Networks that block multicast need manual host entry.",
                    "operationId": "discoverAtem",
                    "responses": {
                        "200": {
                            "description": "Switchers found, possibly none",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "type": "object",
                                        "required": ["devices"],
                                        "properties": {
                                            "devices": {
                                                "type": "array",
                                                "items": { "$ref": "#/components/schemas/DiscoveredAtem" }
                                            }
                                        }
                                    }
                                }
                            }
                        },
                        "502": { "description": "mDNS could not be started on this machine" }
                    }
                }
            },
            "/api/connectors/atem/stream/youtube": {
                "post": {
                    "tags": ["Connectors"],
                    "summary": "Push YouTube settings to the ATEM switcher",
                    "description": "Writes the channel's RTMP ingestion address and stream key into the switcher's streaming service.\n\nSets the destination only — the switcher does not go live. Requires a connected switcher with a streaming engine (409 when none is connected) and a linked YouTube account.",
                    "operationId": "pushAtemYouTubeSettings",
                    "responses": {
                        "200": {
                            "description": "Where the switcher's stream now points",
                            "content": {
                                "application/json": {
                                    "schema": { "$ref": "#/components/schemas/AtemStreamTarget" }
                                }
                            }
                        },
                        "409": { "description": "No switcher connected" },
                        "502": { "description": "The switcher or the YouTube API rejected the request" }
                    }
                }
            },
            "/api/connectors/blackmagic-camera/settings": {
                "get": {
                    "tags": ["Connectors"],
                    "summary": "Read the camera's storage, record format and livestream settings",
                    "description": "One pass over the camera's own control API: record format plus every supported format, the media slots and working set, and the livestream status, platform list, active platform and that platform's servers and quality profiles.\n\nPayloads are the camera's, forwarded unchanged. Only the active platform's profile list is fetched — the others cost a round trip each.",
                    "operationId": "getBlackmagicCameraSettings",
                    "responses": {
                        "200": {
                            "description": "Camera settings",
                            "content": {
                                "application/json": {
                                    "schema": { "$ref": "#/components/schemas/CameraSettings" }
                                }
                            }
                        },
                        "409": { "description": "No camera connected" },
                        "502": { "description": "The camera rejected the request" }
                    }
                },
                "put": {
                    "tags": ["Connectors"],
                    "summary": "Write the camera's record format, livestream platform, or both",
                    "description": "Both halves are optional; whichever is present is written. `record` is the camera's own `FormatRequest` — codec, frameRate and both resolutions travel together, as the camera validates the combination as a whole. `stream` is the full active-platform entry, so send back the one from GET with the fields you changed (dropping `key` clears the stream key).\n\nAnswers with the settings read back from the camera.",
                    "operationId": "putBlackmagicCameraSettings",
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": { "$ref": "#/components/schemas/CameraSettingsUpdate" }
                            }
                        }
                    },
                    "responses": {
                        "200": {
                            "description": "Settings after the write",
                            "content": {
                                "application/json": {
                                    "schema": { "$ref": "#/components/schemas/CameraSettings" }
                                }
                            }
                        },
                        "409": { "description": "No camera connected" },
                        "502": { "description": "The camera rejected the format or platform" }
                    }
                }
            },
            "/api/connectors/blackmagic-camera/discover": {
                "post": {
                    "tags": ["Connectors"],
                    "summary": "Scan the LAN for Blackmagic cameras",
                    "description": "mDNS scan (5s). Every connected WebSocket client also receives the result as `blackmagic-camera.discovered`.\n\nWhen no camera is configured yet, the first one found is stored and connected — a scan is how a camera gets connected. An already-configured camera is never repointed.\n\nAn empty list is a normal outcome: mDNS does not cross VLANs, and on macOS the app needs Local Network permission. Adding a camera by host works without discovery.",
                    "operationId": "discoverBlackmagicCameras",
                    "responses": {
                        "200": {
                            "description": "Cameras found, possibly none",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "type": "object",
                                        "required": ["cameras"],
                                        "properties": {
                                            "cameras": {
                                                "type": "array",
                                                "items": { "$ref": "#/components/schemas/DiscoveredCamera" }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            },
            "/api/connectors/middlecontrol/discover": {
                "post": {
                    "tags": ["Connectors"],
                    "summary": "Find Middle Control",
                    "description": "Checks localhost and the Metocast server's local /24 network on ports 11584, 11581 and 11580. A TCP listener is returned only after it sends a valid Middle Control feedback frame. The scan does not save or reconnect the connector; use the returned host and port in its config. Routed and VLAN networks may require manual host entry.",
                    "operationId": "discoverMiddlecontrol",
                    "responses": {
                        "200": {
                            "description": "Middle Control endpoints found, possibly none",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "type": "object",
                                        "required": ["devices"],
                                        "properties": {
                                            "devices": {
                                                "type": "array",
                                                "items": { "$ref": "#/components/schemas/DiscoveredMiddlecontrol" }
                                            }
                                        }
                                    }
                                }
                            }
                        },
                        "401": { "description": "Unauthorized" }
                    }
                }
            },
            "/api/connectors/blackmagic-camera/connect": {
                "post": {
                    "tags": ["Connectors"],
                    "summary": "Connect the Blackmagic camera",
                    "description": "Starts the Blackmagic camera connector using the stored configuration.",
                    "operationId": "connectBlackmagicCamera",
                    "responses": {
                        "204": { "description": "Connection attempt started" },
                        "401": { "description": "Unauthorized" }
                    }
                }
            },
            "/api/connectors/obs/connect": {
                "post": {
                    "tags": ["Connectors"],
                    "summary": "Connect OBS",
                    "description": "Starts the OBS connector using the stored configuration.",
                    "operationId": "connectObs",
                    "responses": {
                        "204": { "description": "Connection attempt started" },
                        "401": { "description": "Unauthorized" }
                    }
                }
            },
            "/api/connectors/obs/disconnect": {
                "post": {
                    "tags": ["Connectors"],
                    "summary": "Disconnect OBS",
                    "operationId": "disconnectObs",
                    "responses": {
                        "204": { "description": "Disconnected" },
                        "401": { "description": "Unauthorized" }
                    }
                }
            },
            "/api/connectors/obs/stream-settings": {
                "get": {
                    "tags": ["Connectors"],
                    "summary": "Get OBS stream destination",
                    "operationId": "getObsStreamSettings",
                    "responses": {
                        "200": {
                            "description": "Current RTMP destination",
                            "content": {
                                "application/json": {
                                    "schema": { "$ref": "#/components/schemas/ObsStreamSettings" }
                                }
                            }
                        },
                        "401": { "description": "Unauthorized" },
                        "409": { "description": "OBS is not connected" },
                        "502": { "description": "OBS rejected the request" }
                    }
                },
                "put": {
                    "tags": ["Connectors"],
                    "summary": "Set OBS stream destination",
                    "description": "Applies a custom RTMP destination (`rtmp_custom`) to OBS.",
                    "operationId": "setObsStreamSettings",
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": { "$ref": "#/components/schemas/ObsStreamSettings" }
                            }
                        }
                    },
                    "responses": {
                        "204": { "description": "Applied" },
                        "401": { "description": "Unauthorized" },
                        "409": { "description": "OBS is not connected" },
                        "502": { "description": "OBS rejected the request" }
                    }
                }
            },
            "/ws": {
                "get": {
                    "tags": ["WebSocket"],
                    "summary": "WebSocket live stream",
                    "description": "**This endpoint performs a WebSocket upgrade — it cannot be tested with the HTTP \"Send\" button.**\n\nUse a dedicated WebSocket client instead:\n- [Hoppscotch](https://hoppscotch.io) → New request → WebSocket\n- [websocat](https://github.com/vi/websocat): `websocat 'ws://<host>/ws?token=<token>'`\n- Bruno: add a request with type `socket`\n\n---\n\n**Connection URL:** `ws://<host>/ws?token=<token>`\n\nAuthentication uses the same bearer token passed as a **query parameter** (the `Authorization` header is not available during the WebSocket handshake).\n\n### Initial messages (pushed immediately on connect)\n\n```json\n{ \"type\": \"connected\", \"serverId\": \"<uuid>\" }\n{ \"type\": \"connector.status\", \"connector\": \"obs\",  \"status\": { \"type\": \"connected\" } }\n{ \"type\": \"connector.status\", \"connector\": \"vmix\", \"status\": { \"type\": \"disconnected\" } }\n```\n\n### Broadcast messages (sent when data changes)\n\n| `type` | Trigger | Schema |\n|---|---|---|\n| `connector.status` | OBS or VMix connection state changes | `WsConnectorStatusMessage` |\n| `event.changed` | Event created, updated, or deleted | `WsEventChangedMessage` |\n| `recording.changed` | Recording created or updated | `WsRecordingChangedMessage` |\n| `presenter.state` | Presentation loaded or unloaded | `{ type, state: PresenterState }` with `renderMode: \"text\" | \"svg\"` |\n| `presenter.slide_changed` | Slide navigation | `{ type, currentSlide, totalSlides }` |\n\n### Presenter WS commands\n\n| Command | Fields | Description |\n|---|---|---|\n| `presenter.load` | `file_path`, optional `render_mode` | Load a .pptx into the presenter; defaults to text mode |\n| `presenter.load_bible_reference` | optional `event_id`, `reference_type` | Load an event Textus/Lekció Bible reference into the text presenter. If `event_id` is omitted, the backend-selected presenter event is used. |\n| `presenter.unload` | — | Clear the active presentation |\n| `presenter.next` | — | Advance one slide |\n| `presenter.prev` | — | Go back one slide |\n| `presenter.first` | — | Jump to slide 1 |\n| `presenter.last` | — | Jump to last slide |\n| `presenter.goto` | `slide` | Jump to a specific slide number |\n| `presenter.status` | — | Reply to requesting client with `presenter.state` |\n\n### Event WS commands for presenter controls\n\n| Command | Fields | Description |\n|---|---|---|\n| `events.presenter_list` | — | Return backend-ordered presenter event choices plus `selectedEventId` for the current/next event |\n\n`presentation.open` uses SVG mode by default in web-presenter mode. Send `render_mode: \"text\"` to force the text renderer.",
                    "operationId": "connectWebSocket",
                    "security": [],
                    "parameters": [
                        {
                            "name": "token",
                            "in": "query",
                            "required": true,
                            "description": "Bearer auth token",
                            "schema": { "type": "string" }
                        }
                    ],
                    "responses": {
                        "101": { "description": "Switching Protocols — WebSocket handshake accepted (only reachable via a WebSocket client)" },
                        "426": {
                            "description": "Upgrade Required — returned when this endpoint is called as a plain HTTP request instead of a WebSocket upgrade",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "type": "object",
                                        "properties": {
                                            "error":       { "type": "string", "example": "upgrade_required" },
                                            "description": { "type": "string" },
                                            "connect":     { "type": "string", "example": "ws://<host>/ws?token=<your-token>" },
                                            "auth":        { "type": "string" }
                                        }
                                    }
                                }
                            }
                        },
                        "401": { "description": "Unauthorized — token missing or invalid" }
                    }
                }
            }
        }
    })
}
