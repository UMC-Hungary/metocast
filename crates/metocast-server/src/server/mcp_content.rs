//! What the MCP server offers besides tools: state and manual resources, and prompts.
//! `mcp.rs` serves them; the lists live here so they read as data.

use serde_json::{Value, json};

pub const STATE_PREFIX: &str = "metocast://state/";
pub const EVENT_PREFIX: &str = "metocast://state/events/";
pub const FEATURE_PREFIX: &str = "metocast://docs/features/";

/// `(uri, name, tool that reads it)`. An empty tool means `mcp.rs` reads it from the service
/// directly. All of them are read-only, so they stay available in read-only mode.
pub const STATE: &[(&str, &str, &str)] = &[
    (
        "metocast://state/connectors",
        "Connector state",
        "connectors_state",
    ),
    (
        "metocast://state/connector-status",
        "Connector status",
        "connectors_status",
    ),
    (
        "metocast://state/presenter",
        "Web presenter",
        "presenter_status",
    ),
    (
        "metocast://state/presentation",
        "Presentation",
        "presentation_status",
    ),
    ("metocast://state/transcription", "Live transcript", ""),
    ("metocast://state/queues", "Job queues", "queues_list"),
    (
        "metocast://state/clients",
        "Connected clients",
        "clients_list",
    ),
    ("metocast://state/events", "Service events", "events_list"),
];

/// The feature pages served as `metocast://docs/features/{name}.md`.
pub const FEATURES: &[(&str, &str)] = &[
    ("mcp", include_str!("../../../../docs/mcp/README.md")),
    ("media", include_str!("../../../../docs/features/media.md")),
    (
        "connector-automations",
        include_str!("../../../../docs/features/connector-automations.md"),
    ),
    (
        "broadcast-controls",
        include_str!("../../../../docs/features/broadcast-controls.md"),
    ),
];

/// Which state resources a pushed message makes stale, so a subscriber hears about it.
#[must_use]
pub fn affected(event: &Value) -> Vec<String> {
    let kind = event["type"].as_str().unwrap_or_default();
    let uri = |name: &str| format!("{STATE_PREFIX}{name}");
    match kind {
        "connector.status" | "connector.state" => {
            vec![uri("connectors"), uri("connector-status")]
        }
        "presenter.state" | "presenter.slide_changed" => {
            vec![uri("presenter"), uri("presentation")]
        }
        "presentation.status" | "presentation.settings" | "keynote.status" => {
            vec![uri("presentation")]
        }
        // `transcription.partial` is left out: it fires several times a second.
        "transcription.state"
        | "transcription.segment"
        | "transcription.snapshot"
        | "transcription.credit" => vec![uri("transcription")],
        "queue.stats" => vec![uri("queues")],
        "clients.updated" => vec![uri("clients")],
        "event.changed" => {
            let mut stale = vec![uri("events")];
            if let Some(id) = event["data"]["record"]["id"].as_str() {
                stale.push(format!("{EVENT_PREFIX}{id}"));
            }
            stale
        }
        _ => Vec::new(),
    }
}

pub fn resources() -> Value {
    let mut list = vec![
        json!({ "uri": "metocast://docs/openapi.json", "name": "REST API (OpenAPI)", "mimeType": "application/json" }),
        json!({ "uri": "metocast://docs/websocket.md", "name": "WebSocket commands and events", "mimeType": "text/markdown" }),
        json!({ "uri": "metocast://docs/mcp-tools.md", "name": "MCP tool reference", "mimeType": "text/markdown" }),
    ];
    list.extend(FEATURES.iter().map(|(name, _)| {
        json!({
            "uri": format!("{FEATURE_PREFIX}{name}.md"),
            "name": format!("Feature guide: {name}"),
            "mimeType": "text/markdown",
        })
    }));
    list.extend(
        STATE.iter().map(
            |(uri, name, _)| json!({ "uri": uri, "name": name, "mimeType": "application/json" }),
        ),
    );
    json!({ "resources": list })
}

pub fn templates() -> Value {
    json!({ "resourceTemplates": [
        {
            "uriTemplate": format!("{EVENT_PREFIX}{{id}}"),
            "name": "One service event",
            "mimeType": "application/json",
        },
        {
            "uriTemplate": format!("{FEATURE_PREFIX}{{name}}.md"),
            "name": "Feature guide",
            "mimeType": "text/markdown",
        },
    ] })
}

// ── Prompts ─────────────────────────────────────────────────────────────────

/// `(name, what it is for, [(argument, description, required)], steps)`.
pub type Prompt = (
    &'static str,
    &'static str,
    &'static [(&'static str, &'static str, bool)],
    &'static str,
);

/// `(name, what it is for, [(argument, description, required)], steps)`. `{event_id}` in the steps
/// is replaced by the argument. Every `backticked_name` must be a real tool or input;
/// `prompts_name_real_tools` fails otherwise.
#[rustfmt::skip]
pub const PROMPTS: &[Prompt] = &[
    (
        "prepare_service",
        "Check that everything is ready for a service, without starting anything that goes on air.",
        &[("event_id", "The event to prepare; the next one when omitted.", false)],
        "Get Metocast ready for the service (event {event_id}). Do not start anything that goes on \
         air.\n\
         1. Call `events_list`, pick the event (the one given, otherwise the next upcoming), and \
         read it with `events_get`. Report its title, time, speaker and Bible readings.\n\
         2. Call `connectors_status` and `connectors_state`. List every connector that is enabled \
         but not connected.\n\
         3. Call `rodecaster_profile` and `rodecaster_audio_record_state`. Say whether the desk is \
         reachable and the audio recorder is idle.\n\
         4. Call `presentation_status`. If the event has a Bible reading, offer to load it with \
         `presenter_load_bible_reference` (`reference_type` is `textus` or `leckio`), and wait for \
         a yes.\n\
         5. Call `recordings_untracked_list` and mention recordings not yet linked to an event.\n\
         Finish with a short checklist: what is ready, and what needs the operator.",
    ),
    (
        "go_live_checklist",
        "Run the pre-live checks, then wait for the operator's yes before anything goes on air.",
        &[],
        "Run the pre-live checklist. Read only until the operator confirms.\n\
         1. `connectors_status`: report each connector that is not connected.\n\
         2. `obs_devices_available`. If it answers `no_scan_data`, call `obs_devices_scan` first \
         and ask again.\n\
         3. `rodecaster_audio_record_state` and `rodecaster_profile`.\n\
         4. `queues_list`: report failed or stuck jobs.\n\
         5. `recordings_untracked_list`.\n\
         Summarise what is ready and what is not. Then ask the operator which of these to run and \
         wait for an explicit yes before each: `obs_stream_start`, `obs_record_start`, \
         `rodecaster_audio_record_start` (needs `event_id`), `atem_stream_start`, and once the \
         encoder streams, `events_stream_start` for the event's YouTube or Facebook broadcast. \
         Never call a \
         stream or record tool without a yes in this conversation.",
    ),
    (
        "post_service_upload",
        "Link the recordings of a finished service, choose where they go, upload and follow it.",
        &[("event_id", "The event that was just held.", true)],
        "Get the recordings of event {event_id} uploaded.\n\
         1. `recordings_list` for the event: show each file's name, length and whether it \
         uploaded.\n\
         2. `recordings_untracked_list`: offer to link any that belong to this event with \
         `recordings_untracked_assign`.\n\
         3. Ask which recordings go to which platform, then call `recordings_flag_upload`.\n\
         4. Call `uploads_trigger` to run the upload now.\n\
         5. Call `events_poll` with `types` set to the `upload.progress`, `upload.completed` and \
         `upload.failed` events and `wait_ms` 20000, repeating until every upload ends. Check \
         `queues_jobs_list` for failures and offer `jobs_retry`.\n\
         6. If asked, call `recordings_captions_create` for each finished upload.\n\
         Report what uploaded and what failed.",
    ),
];

pub fn prompts() -> Value {
    let list: Vec<Value> = PROMPTS
        .iter()
        .map(|(name, about, arguments, _)| {
            json!({
                "name": name,
                "description": about,
                "arguments": arguments
                    .iter()
                    .map(|(arg, description, required)| json!({
                        "name": arg, "description": description, "required": required,
                    }))
                    .collect::<Vec<_>>(),
            })
        })
        .collect();
    json!({ "prompts": list })
}

/// `Err` is the message for an invalid-params error.
pub fn prompt(name: &str, arguments: &Value) -> Result<Value, String> {
    let Some((_, about, declared, steps)) = PROMPTS.iter().find(|prompt| prompt.0 == name) else {
        return Err(format!("Unknown prompt: {name}"));
    };
    let mut text = (*steps).to_string();
    for (arg, _, required) in *declared {
        match arguments[*arg].as_str() {
            Some(value) => text = text.replace(&format!("{{{arg}}}"), value),
            None if *required => return Err(format!("Missing argument: {arg}")),
            None => text = text.replace(&format!("{{{arg}}}"), "not given"),
        }
    }
    Ok(json!({
        "description": about,
        "messages": [{ "role": "user", "content": { "type": "text", "text": text } }],
    }))
}
