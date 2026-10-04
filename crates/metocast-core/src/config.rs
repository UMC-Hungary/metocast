//! Connector configuration as a form.
//!
//! The server keeps one JSON blob per connector and never returns a stored secret: it blanks the
//! field and adds `"<field>Set": true` beside it. A client only needs to know which fields a
//! connector has, so that knowledge lives here rather than in every app.
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
        "rodecaster" => &[
            ("enabled", "Enabled", Toggle),
            ("notifyOnMute", "Notify on Mute", Toggle),
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
                _ => match config.get(*key) {
                    Some(Value::String(text)) => text.clone(),
                    Some(Value::Bool(flag)) => flag.to_string(),
                    Some(Value::Number(number)) => number.to_string(),
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
                body.insert((*key).to_string(), json!(text == "true"));
            }
            Number => {
                body.insert((*key).to_string(), json!(text.parse::<u16>().unwrap_or(0)));
            }
            Text => {
                body.insert((*key).to_string(), json!(text));
            }
            Secret if !text.is_empty() => {
                body.insert((*key).to_string(), json!(text));
            }
            Secret if entry.clear => {
                body.insert(format!("{key}Set"), json!(false));
            }
            Secret => {}
        }
    }
    Value::Object(body)
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
    fn unknown_connectors_have_no_form() {
        assert!(schema("nope").is_empty());
        assert!(form("nope", &json!({})).is_empty());
        assert_eq!(body("nope", &[]), json!({}));
    }
}
