//! Connector configuration as a form.
//!
//! The server keeps one JSON blob per connector and never returns a stored secret: it blanks the
//! field and adds `"<field>Set": true` beside it. A client only needs to know which fields a
//! connector has, so that knowledge lives here rather than in every app.
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ConfigFieldKind {
    Toggle,
    Text,
    Number,
    /// Write-only: the stored value never leaves the server.
    Secret,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigField {
    pub key: String,
    pub label: String,
    pub kind: ConfigFieldKind,
    /// The stored value; always blank for a secret.
    pub value: String,
    /// A secret is stored, even though its value never leaves the server.
    pub is_set: bool,
}

/// What a client sends back for one field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigValue {
    pub key: String,
    pub value: String,
    /// Forget the stored secret. Ignored unless the field is a secret with an empty `value`.
    pub clear: bool,
}

use ConfigFieldKind::{Number, Secret, Text, Toggle};

type Schema = &'static [(&'static str, &'static str, ConfigFieldKind)];

const HOST_PORT: Schema = &[
    ("enabled", "Enabled", Toggle),
    ("host", "Host", Text),
    ("port", "Port", Number),
];

/// Every connector a client can configure, in display order.
pub const CONNECTORS: &[(&str, &str)] = &[
    ("obs", "OBS Studio"),
    ("atem", "ATEM Switcher"),
    ("middlecontrol", "Middle Control"),
    ("blackmagic-camera", "Blackmagic Camera"),
    ("rodecaster", "RØDECaster"),
    ("vmix", "vMix"),
    ("broadlink", "Broadlink"),
    ("youtube", "YouTube"),
    ("facebook", "Facebook"),
    ("discord", "Discord"),
    ("szentiras", "Szentírás"),
    ("media", "Media Processing"),
    ("openai", "OpenAI Translation"),
    ("mcp", "AI Agents (MCP)"),
];

/// The fields `connector` stores, or none when it isn't one this crate knows.
#[must_use]
pub fn schema(connector: &str) -> Schema {
    match connector {
        "obs" => &[
            ("enabled", "Enabled", Toggle),
            ("host", "Host", Text),
            ("port", "Port", Number),
            ("password", "Password", Secret),
        ],
        "atem" | "middlecontrol" | "vmix" => HOST_PORT,
        "blackmagic-camera" => &[
            ("enabled", "Enabled", Toggle),
            ("host", "Host", Text),
            ("username", "User Name", Text),
            ("password", "Password", Secret),
            ("fingerprint", "Certificate Fingerprint", Text),
        ],
        // A dotted key is a field of a nested object. Recording needs the main mix by default,
        // so the two fields below are all it takes to arm Metocast's audio recording.
        "rodecaster" => &[
            ("enabled", "Enabled", Toggle),
            ("notifyOnMute", "Notify on Mute", Toggle),
            ("audioRecording.enabled", "Record Audio", Toggle),
            (
                "audioRecording.directory",
                "Recording Folder (on the Server's Mac)",
                Text,
            ),
        ],
        "broadlink" => &[("enabled", "Enabled", Toggle)],
        "youtube" => &[
            ("enabled", "Enabled", Toggle),
            ("clientId", "Client ID", Text),
            ("clientSecret", "Client Secret", Secret),
        ],
        "facebook" => &[
            ("enabled", "Enabled", Toggle),
            ("appId", "App ID", Text),
            ("appSecret", "App Secret", Secret),
            ("pageId", "Page ID", Text),
        ],
        "discord" => &[
            ("enabled", "Enabled", Toggle),
            ("webhookUrl", "Webhook URL", Secret),
        ],
        "szentiras" => &[
            ("enabled", "Enabled", Toggle),
            ("apiKey", "API Key", Secret),
        ],
        // Blank or 0 keeps what the clip was shot with; see `media::MediaConfig`.
        "media" => &[
            ("enabled", "Enabled", Toggle),
            ("gamma", "Gamma", Text),
            ("gamut", "Gamut", Text),
            ("iso", "ISO", Number),
            ("kelvin", "White Balance (K)", Number),
            ("tint", "Tint", Text),
            ("exposure", "Exposure (Stops)", Text),
            ("highlightRecovery", "Highlight Recovery", Toggle),
            ("gamutCompression", "Gamut Compression", Toggle),
            ("lutMode", "LUT (embedded, file, sidecar or none)", Text),
            ("lutPath", "LUT File", Text),
            ("sidecarPath", "Resolve Sidecar", Text),
            ("aspect", "Frame (16:9 or native)", Text),
            ("cropShiftY", "Crop Shift Down (px)", Text),
            ("bitrateMbps", "Bitrate (Mbps)", Number),
            ("audioChannels", "Audio Channels", Text),
            ("outputFolder", "Output Folder", Text),
            ("audioTargetLufs", "Audio Loudness (LUFS)", Text),
            ("audioOutputFolder", "Audio Output Folder", Text),
            ("ffmpegPath", "ffmpeg Path", Text),
            ("brawSdkPath", "Blackmagic RAW SDK Folder", Text),
            ("jiveVocalsPath", "jive-vocals Path", Text),
        ],
        // The server's own MCP endpoint, not a device; see `server::mcp::McpConfig`.
        "mcp" => &[
            ("enabled", "Enabled", Toggle),
            (
                "readOnly",
                "Read-only (agents can look, not change)",
                Toggle,
            ),
            ("allowLan", "Allow other devices on the network", Toggle),
        ],
        // The audio source is edited on the Sanctum settings page.
        "openai" => &[
            ("enabled", "Enabled", Toggle),
            ("apiKey", "API Key", Secret),
            ("adminKey", "Admin Key (credit balance)", Secret),
            ("topUps", "Top-ups (2026-10-01 20, …)", Text),
            ("lagMs", "Translation Lag (ms)", Number),
            ("youtubeLiveCaptions", "Live YouTube Captions", Toggle),
            (
                "captionIngestionUrl",
                "Caption Ingestion URL (override)",
                Secret,
            ),
            (
                "autoYoutubeCaptions",
                "Caption Finished YouTube Uploads",
                Toggle,
            ),
        ],
        _ => &[],
    }
}

/// The form for `connector`, filled in from what `GET /api/connectors/{name}/config` returned.
#[must_use]
pub fn form(connector: &str, config: &Value) -> Vec<ConfigField> {
    schema(connector)
        .iter()
        .map(|(key, label, kind)| ConfigField {
            key: (*key).to_string(),
            label: (*label).to_string(),
            kind: *kind,
            value: match kind {
                Secret => String::new(),
                _ => match lookup(config, key) {
                    Some(Value::String(text)) => text.clone(),
                    Some(Value::Bool(flag)) => flag.to_string(),
                    Some(Value::Number(number)) => number.to_string(),
                    Some(Value::Array(top_ups)) if *key == "topUps" => top_ups_text(top_ups),
                    _ => String::new(),
                },
            },
            is_set: matches!(kind, Secret)
                && config
                    .get(format!("{key}Set"))
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
        })
        .collect()
}

/// A field's stored value, following a dotted key into nested objects.
fn lookup<'a>(config: &'a Value, key: &str) -> Option<&'a Value> {
    key.split('.')
        .try_fold(config, |value, part| value.get(part))
}

/// Sets a field in `body`, creating the nested objects a dotted key names.
fn put(body: &mut Map<String, Value>, key: &str, value: Value) {
    match key.split_once('.') {
        Some((head, rest)) => {
            let child = body
                .entry(head)
                .or_insert_with(|| Value::Object(Map::new()));
            if let Value::Object(child) = child {
                put(child, rest, value);
            }
        }
        None => {
            body.insert(key.to_string(), value);
        }
    }
}

/// The body for `PUT /api/connectors/{name}/config`. An untouched secret is left out, which is how
/// the server is told to keep the one it has; a cleared one is sent as `"<field>Set": false`.
#[must_use]
pub fn body(connector: &str, values: &[ConfigValue]) -> Value {
    let mut body = Map::new();
    for (key, _, kind) in schema(connector) {
        let Some(entry) = values.iter().find(|value| value.key == *key) else {
            continue;
        };
        let text = entry.value.trim();
        match kind {
            Toggle => {
                put(&mut body, key, json!(text == "true"));
            }
            Number => {
                put(&mut body, key, json!(text.parse::<u16>().unwrap_or(0)));
            }
            Text if *key == "topUps" => {
                // Unreadable text keeps the stored top-ups instead of wiping them.
                if let Some(top_ups) = parse_top_ups(text) {
                    put(&mut body, key, top_ups);
                }
            }
            Text => {
                put(&mut body, key, json!(text));
            }
            Secret if !text.is_empty() => {
                put(&mut body, key, json!(text));
            }
            Secret if entry.clear => {
                body.insert(format!("{key}Set"), json!(false));
            }
            Secret => {}
        }
    }
    Value::Object(body)
}

/// `[{"amountUsd": 20, "date": "2026-10-01"}]` as `2026-10-01 20`, one entry per comma.
fn top_ups_text(top_ups: &[Value]) -> String {
    top_ups
        .iter()
        .filter_map(|top_up| {
            Some(format!(
                "{} {}",
                top_up["date"].as_str()?,
                top_up["amountUsd"].as_f64()?
            ))
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// The reverse of [`top_ups_text`]; `None` when any entry isn't a date and a positive amount.
fn parse_top_ups(text: &str) -> Option<Value> {
    let mut top_ups = Vec::new();
    for entry in text
        .split([',', ';'])
        .map(str::trim)
        .filter(|e| !e.is_empty())
    {
        let (date, amount) = entry.split_once(char::is_whitespace)?;
        let date: NaiveDate = date.parse().ok()?;
        let amount: f64 = amount.trim().trim_start_matches('$').parse().ok()?;
        if !amount.is_finite() || amount <= 0.0 {
            return None;
        }
        top_ups.push(json!({ "amountUsd": amount, "date": date }));
    }
    Some(Value::Array(top_ups))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_form_shows_stored_values_but_never_a_secret() {
        // Shape from `GET /api/connectors/obs/config`, which blanks secrets.
        let stored = json!({"enabled": true, "host": "localhost", "port": 4455,
                            "password": "", "passwordSet": true});
        let fields = form("obs", &stored);
        assert_eq!(fields.len(), 4);
        assert_eq!(fields[1].value, "localhost");
        assert_eq!(fields[2].value, "4455");
        let password = &fields[3];
        assert_eq!((password.kind, password.value.as_str()), (Secret, ""));
        assert!(password.is_set);
    }

    #[test]
    fn saving_keeps_clears_or_replaces_a_secret() {
        let edited = |value: &str, clear: bool| {
            vec![
                ConfigValue {
                    key: "enabled".into(),
                    value: "true".into(),
                    clear: false,
                },
                ConfigValue {
                    key: "host".into(),
                    value: " studio.local ".into(),
                    clear: false,
                },
                ConfigValue {
                    key: "port".into(),
                    value: "4455".into(),
                    clear: false,
                },
                ConfigValue {
                    key: "password".into(),
                    value: value.into(),
                    clear,
                },
            ]
        };

        // Untouched: no password key at all, so the server keeps the stored one.
        let kept = body("obs", &edited("", false));
        assert_eq!(kept["host"], "studio.local");
        assert_eq!(kept["port"], 4455);
        assert_eq!(kept["enabled"], true);
        assert!(kept.get("password").is_none() && kept.get("passwordSet").is_none());

        assert_eq!(
            body("obs", &edited("hunter2", false))["password"],
            "hunter2"
        );
        assert_eq!(body("obs", &edited("", true))["passwordSet"], false);
    }

    #[test]
    fn the_rodecaster_form_reads_and_writes_the_nested_audio_recording() {
        let stored = json!({"enabled": true, "notifyOnMute": false,
            "audioRecording": {"schemaVersion": 1, "enabled": true, "directory": "/Volumes/Audio"}});
        let fields = form("rodecaster", &stored);
        let value = |key: &str| fields.iter().find(|f| f.key == key).unwrap().value.clone();
        assert_eq!(value("audioRecording.enabled"), "true");
        assert_eq!(value("audioRecording.directory"), "/Volumes/Audio");

        let edited = vec![
            ConfigValue {
                key: "audioRecording.enabled".into(),
                value: "true".into(),
                clear: false,
            },
            ConfigValue {
                key: "audioRecording.directory".into(),
                value: " /Volumes/New ".into(),
                clear: false,
            },
        ];
        assert_eq!(
            body("rodecaster", &edited),
            json!({"audioRecording": {"enabled": true, "directory": "/Volumes/New"}})
        );
    }

    #[test]
    fn openai_top_ups_round_trip_as_text_and_bad_text_keeps_the_stored_ones() {
        let stored = json!({"enabled": true, "topUps": [
            {"amountUsd": 20.0, "date": "2026-10-01"},
            {"amountUsd": 10.5, "date": "2026-10-05"}
        ]});
        let text = form("openai", &stored)
            .into_iter()
            .find(|f| f.key == "topUps")
            .unwrap()
            .value;
        assert_eq!(text, "2026-10-01 20, 2026-10-05 10.5");

        let save = |value: &str| {
            body(
                "openai",
                &[ConfigValue {
                    key: "topUps".into(),
                    value: value.into(),
                    clear: false,
                }],
            )
        };
        assert_eq!(
            save(&text)["topUps"],
            json!([
                {"amountUsd": 20.0, "date": "2026-10-01"},
                {"amountUsd": 10.5, "date": "2026-10-05"}
            ])
        );
        assert_eq!(save("")["topUps"], json!([]));
        assert_eq!(save("2026-10-01 $15")["topUps"][0]["amountUsd"], 15.0);
        for bad in ["twenty", "2026-13-01 20", "2026-10-01 -5", "2026-10-01"] {
            assert!(save(bad).get("topUps").is_none(), "{bad}");
        }
    }

    #[test]
    fn unknown_connectors_have_no_form() {
        assert!(schema("nope").is_empty());
        assert!(form("nope", &json!({})).is_empty());
        assert_eq!(body("nope", &[]), json!({}));
    }
}
