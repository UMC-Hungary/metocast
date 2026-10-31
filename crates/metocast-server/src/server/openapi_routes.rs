//! OpenAPI operations for routes the long hand-written spec in `openapi.rs` does not cover:
//! recordings and activities, Broadlink, scheduled jobs, queues, slide folders, Keynote and the
//! sign-in routes. `mcp::document_missing` stubs whatever is still absent after these.
//!
//! Shapes follow the handlers and models; casing is as each handler reads it (Broadlink bodies
//! are camelCase, the rest of the request bodies here are snake_case).

use serde_json::{Map, Value, json};

fn text() -> Value {
    json!({ "type": "string" })
}

fn uuid() -> Value {
    json!({ "type": "string", "format": "uuid" })
}

fn date_time() -> Value {
    json!({ "type": "string", "format": "date-time" })
}

fn flag() -> Value {
    json!({ "type": "boolean" })
}

fn int() -> Value {
    json!({ "type": "integer" })
}

fn list(items: Value) -> Value {
    json!({ "type": "array", "items": items })
}

fn object(properties: Value, required: &[&str]) -> Value {
    json!({ "type": "object", "properties": properties, "required": required })
}

fn recording() -> Value {
    json!({ "$ref": "#/components/schemas/Recording" })
}

fn path(name: &str) -> Value {
    json!({ "name": name, "in": "path", "required": true, "schema": uuid() })
}

fn query(name: &str, schema: Value, description: &str) -> Value {
    json!({ "name": name, "in": "query", "required": false, "schema": schema, "description": description })
}

/// One operation. `ok` is `(status, description, response schema)`; `errors` are more statuses.
fn op(
    summary: &str,
    tag: &str,
    parameters: Vec<Value>,
    body: Option<Value>,
    ok: (&str, &str, Option<Value>),
    errors: &[(&str, &str)],
) -> Value {
    let mut responses = Map::new();
    let (status, description, schema) = ok;
    let mut success = json!({ "description": description });
    if let Some(schema) = schema {
        success["content"] = json!({ "application/json": { "schema": schema } });
    }
    responses.insert("401".into(), json!({ "description": "Unauthorized" }));
    responses.insert(status.to_string(), success);
    for (status, description) in errors {
        responses.insert((*status).to_string(), json!({ "description": description }));
    }
    let mut operation = json!({
        "summary": summary,
        "tags": [tag],
        "parameters": parameters,
        "responses": responses,
    });
    if let Some(body) = body {
        operation["requestBody"] = json!({
            "required": true,
            "content": { "application/json": { "schema": body } },
        });
    }
    operation
}

fn done(description: &str) -> (&str, &str, Option<Value>) {
    ("204", description, None)
}

/// `/api{path} → { method → operation }`.
pub fn paths() -> Value {
    let event = || path("id");
    let not_found = ("404", "Not found");
    let fault = ("500", "The database failed");

    let untracked = object(
        json!({
            "id": uuid(), "filePath": text(), "fileName": text(), "fileSize": int(),
            "durationSeconds": json!({ "type": "number" }),
            "detectedAt": date_time(), "createdAt": date_time(),
        }),
        &["id", "filePath", "fileName"],
    );
    let activity = object(
        json!({
            "id": uuid(), "eventId": uuid(), "activityType": text(),
            "message": json!({ "type": ["string", "null"] }), "createdAt": date_time(),
        }),
        &["id", "eventId", "activityType"],
    );
    let cron_job = object(
        json!({
            "id": uuid(), "name": text(), "cronExpression": text(), "enabled": flag(),
            "pullYoutube": flag(), "autoUpload": flag(),
            "createdAt": date_time(), "updatedAt": date_time(),
        }),
        &["id", "name", "cronExpression", "enabled"],
    );
    let cron_body = object(
        json!({
            "name": text(), "cronExpression": text(), "enabled": flag(),
            "pullYoutube": flag(), "autoUpload": flag(),
        }),
        &[
            "name",
            "cronExpression",
            "enabled",
            "pullYoutube",
            "autoUpload",
        ],
    );
    let device = object(
        json!({
            "id": uuid(), "name": text(), "deviceType": text(),
            "model": json!({ "type": ["string", "null"] }), "host": text(), "mac": text(),
            "isDefault": flag(),
        }),
        &["id", "name", "host", "mac"],
    );
    let command = object(
        json!({
            "id": uuid(), "deviceId": json!({ "type": ["string", "null"], "format": "uuid" }),
            "name": text(), "slug": text(), "code": text(), "codeType": text(),
            "category": json!({ "type": ["string", "null"] }),
        }),
        &["id", "name", "slug", "code", "codeType"],
    );
    let queue = object(
        json!({
            "queue": text(), "pending": int(), "processing": int(), "succeeded": int(),
            "dead": int(), "oldestAvailableAt": json!({ "type": ["string", "null"], "format": "date-time" }),
        }),
        &["queue"],
    );
    let job = object(
        json!({
            "id": uuid(), "queue": text(), "jobType": text(), "status": text(),
            "attempts": int(), "maxAttempts": int(),
            "lastError": json!({ "type": ["string", "null"] }),
            "payload": json!({ "type": "object" }),
            "createdAt": date_time(), "updatedAt": date_time(),
        }),
        &["id", "queue", "status"],
    );
    let folder = object(
        json!({ "id": uuid(), "path": text(), "name": text(), "sortOrder": int() }),
        &["id", "path", "name"],
    );
    let wrapped = |data: Value| object(json!({ "success": flag(), "data": data }), &["success"]);
    let success = object(json!({ "success": flag(), "error": text() }), &["success"]);
    let keynote_status = object(
        json!({
            "appRunning": flag(), "slideshowActive": flag(),
            "currentSlide": json!({ "type": ["integer", "null"] }),
            "totalSlides": json!({ "type": ["integer", "null"] }),
            "documentName": json!({ "type": ["string", "null"] }),
        }),
        &["appRunning", "slideshowActive"],
    );
    let url = object(json!({ "url": text() }), &["url"]);
    let delete_file = || {
        query(
            "delete_file",
            flag(),
            "Also delete the file from disk. Default false.",
        )
    };
    let keynote = |summary: &str, body: Option<Value>| {
        op(
            summary,
            "Keynote",
            vec![],
            body,
            ("200", "Done", Some(success.clone())),
            &[("500", "Keynote refused or is not available")],
        )
    };

    let mut paths = Map::new();
    let mut put = |route: &str, method: &str, operation: Value| {
        paths
            .entry(format!("/api{route}"))
            .or_insert_with(|| json!({}))[method] = operation;
    };

    put(
        "/events/{id}",
        "delete",
        op(
            "Delete an event with its recordings, activities and order of service",
            "Events",
            vec![event()],
            None,
            done("Deleted"),
            &[not_found, fault],
        ),
    );

    // ── Recordings and activities ────────────────────────────────────────────
    put(
        "/recordings",
        "get",
        op(
            "List recordings across events, newest first (at most 100)",
            "Recordings",
            vec![query(
                "filter",
                json!({ "type": "string", "enum": ["not_flagged", "flagged", "in_progress", "uploaded"] }),
                "not_flagged: never flagged and no uploads. flagged: marked for upload, none started. \
             in_progress: uploading, paused or failed. uploaded: at least one platform finished.",
            )],
            None,
            (
                "200",
                "Recordings, each with its event's title",
                Some(list(json!({
                    "allOf": [recording(), object(json!({ "eventTitle": text() }), &["eventTitle"])],
                }))),
            ),
            &[],
        ),
    );
    put(
        "/events/{id}/recordings/{recording_id}",
        "delete",
        op(
            "Remove a recording from an event",
            "Recordings",
            vec![event(), path("recording_id"), delete_file()],
            None,
            done("Removed"),
            &[not_found, fault],
        ),
    );
    put(
        "/events/{id}/recordings/flag-upload",
        "post",
        op(
            "Mark recordings uploadable and choose their platforms and visibility",
            "Recordings",
            vec![event()],
            Some(object(
                json!({ "recordings": list(object(
                json!({
                    "recording_id": uuid(),
                    "platforms": list(json!({ "type": "string", "enum": ["youtube", "facebook"] })),
                    "custom_title": text(), "custom_description": text(),
                    "youtube_visibility": json!({ "type": "string", "enum": ["private", "unlisted", "public"] }),
                    "facebook_visibility": text(),
                }),
                &["recording_id", "platforms"],
            )) }),
                &["recordings"],
            )),
            done("Flagged; an upload cycle is triggered"),
            &[fault],
        ),
    );
    put(
        "/recordings/untracked",
        "get",
        op(
            "List recording files found on disk that no event owns",
            "Recordings",
            vec![],
            None,
            (
                "200",
                "Untracked recordings, newest first",
                Some(list(untracked)),
            ),
            &[fault],
        ),
    );
    put(
        "/recordings/untracked/{id}/assign",
        "post",
        op(
            "Link an untracked recording to an event",
            "Recordings",
            vec![path("id")],
            Some(object(json!({ "event_id": uuid() }), &["event_id"])),
            ("200", "The new recording", Some(recording())),
            &[("404", "Untracked recording or event not found"), fault],
        ),
    );
    put(
        "/recordings/untracked/{id}",
        "delete",
        op(
            "Forget an untracked recording",
            "Recordings",
            vec![path("id"), delete_file()],
            None,
            done("Forgotten"),
            &[not_found, fault],
        ),
    );
    put(
        "/events/{id}/activities",
        "get",
        op(
            "List an event's activity log, oldest first",
            "Events",
            vec![event()],
            None,
            ("200", "Activities", Some(list(activity.clone()))),
            &[fault],
        ),
    );
    put(
        "/events/{id}/activities",
        "post",
        op(
            "Add an entry to an event's activity log",
            "Events",
            vec![event()],
            Some(object(
                json!({ "activity_type": text(), "message": json!({ "type": ["string", "null"] }) }),
                &["activity_type"],
            )),
            ("201", "The entry", Some(activity)),
            &[not_found, fault],
        ),
    );
    put(
        "/events/{id}/activities/{activity_id}",
        "delete",
        op(
            "Delete an activity log entry",
            "Events",
            vec![event(), path("activity_id")],
            None,
            done("Deleted"),
            &[not_found, fault],
        ),
    );

    // ── Connectors, sign-in, scheduling ──────────────────────────────────────
    put(
        "/connectors/state",
        "get",
        op(
            "Live state of the connectors that report one",
            "Connectors",
            vec![],
            None,
            (
                "200",
                "State by connector; a connector with none yet is null",
                Some(object(
                    json!({
                        "obs": json!({ "type": ["object", "null"], "properties": { "isStreaming": flag(), "isRecording": flag() } }),
                        "blackmagic-camera": json!({ "type": ["object", "null"], "properties": { "isStreaming": flag(), "streamStatus": text(), "isRecording": flag(), "hasMedia": flag(), "destination": json!({ "type": ["object", "null"], "description": "Where the livestream points; never the stream key", "properties": { "service": text(), "url": text(), "broadcast": json!({ "type": ["string", "null"] }), "matchesYoutube": json!({ "type": ["boolean", "null"] }) } }) } }),
                        "middlecontrol": json!({ "type": ["object", "null"] }),
                        "atem": json!({ "type": ["object", "null"] }),
                    }),
                    &[],
                )),
            ),
            &[],
        ),
    );
    put(
        "/connectors/youtube/content",
        "get",
        op(
            "YouTube channel content for the event forms (categories, playlists)",
            "Connectors",
            vec![],
            None,
            ("200", "Channel content", Some(json!({ "type": "object" }))),
            &[
                ("401", "YouTube sign-in expired (`auth_required`)"),
                ("500", "YouTube refused"),
            ],
        ),
    );
    let platform = json!({
        "name": "platform", "in": "path", "required": true,
        "schema": { "type": "string", "enum": ["youtube", "facebook"] }
    });
    let stream_errors = [
        not_found,
        (
            "401",
            "Not signed in to that platform (`not_authenticated`)",
        ),
        (
            "409",
            "`not_scheduled`, `already_live`, `not_live`, `broadcast_ended`, or `encoder_not_streaming` \
             (YouTube: no video reaches the stream yet; start OBS or the encoder first)",
        ),
        (
            "502",
            "The platform refused (`youtube_api_…` / `facebook_api_…` with its message)",
        ),
    ];
    let stream_ok = Some(object(json!({ "ok": flag() }), &["ok"]));
    put(
        "/events/{id}/stream/{platform}/start",
        "post",
        op(
            "Take the event's YouTube broadcast or Facebook live video live now. Destructive: viewers \
             see the stream. YouTube binds the channel's stream if needed and goes through `testing`, \
             which takes up to ~30 s; Facebook waits as `starting` until video arrives. Clients get \
             `event.changed` with the connection's new `scheduleStatus`.",
            "Events",
            vec![path("id"), platform.clone()],
            None,
            ("200", "Live (or starting, on Facebook)", stream_ok.clone()),
            &stream_errors,
        ),
    );
    put(
        "/events/{id}/stream/{platform}/stop",
        "post",
        op(
            "End the event's live YouTube broadcast or Facebook live video. Destructive: it can't be \
             restarted. Clients get `event.changed` with `scheduleStatus: ended`.",
            "Events",
            vec![path("id"), platform],
            None,
            ("200", "Ended", stream_ok),
            &stream_errors,
        ),
    );
    put(
        "/connectors/youtube/schedule/{event_id}",
        "post",
        op(
            "Create the YouTube broadcast for an event",
            "Connectors",
            vec![path("event_id")],
            None,
            (
                "200",
                "The scheduled broadcast",
                Some(json!({ "type": "object" })),
            ),
            &[not_found, ("401", "Not signed in to YouTube")],
        ),
    );
    put(
        "/connectors/facebook/schedule/{event_id}",
        "post",
        op(
            "Create the Facebook live video for an event",
            "Connectors",
            vec![path("event_id")],
            None,
            (
                "200",
                "The scheduled live video",
                Some(json!({ "type": "object" })),
            ),
            &[
                not_found,
                ("400", "No Facebook page id configured"),
                ("401", "Not signed in to Facebook"),
            ],
        ),
    );
    put(
        "/auth/callback",
        "post",
        op(
            "Finish a sign-in that a client device captured on its loopback address",
            "Connectors",
            vec![],
            Some(object(
                json!({ "code": text(), "state": text() }),
                &["code", "state"],
            )),
            (
                "200",
                "Signed in; names the connector",
                Some(object(
                    json!({ "status": text(), "connector": text() }),
                    &["status", "connector"],
                )),
            ),
            &[("400", "The code or state was refused")],
        ),
    );
    for platform in ["youtube", "facebook"] {
        let name = if platform == "youtube" {
            "YouTube"
        } else {
            "Facebook"
        };
        put(
            &format!("/auth/{platform}/url"),
            "get",
            op(
                &format!("The URL that signs the operator in to {name}"),
                "Connectors",
                vec![],
                None,
                ("200", "Open this URL in a browser", Some(url.clone())),
                &[("400", "The connector has no client id or app id yet")],
            ),
        );
        put(
            &format!("/auth/{platform}/logout"),
            "post",
            op(
                &format!("Sign out of {name} and forget its tokens"),
                "Connectors",
                vec![],
                None,
                done("Signed out"),
                &[fault],
            ),
        );
    }
    put(
        "/cron-jobs",
        "get",
        op(
            "List scheduled jobs",
            "Scheduling",
            vec![],
            None,
            ("200", "Jobs", Some(list(cron_job.clone()))),
            &[fault],
        ),
    );
    put(
        "/cron-jobs",
        "post",
        op(
            "Create a scheduled job",
            "Scheduling",
            vec![],
            Some(cron_body.clone()),
            ("201", "The job", Some(cron_job.clone())),
            &[("400", "Invalid cron expression"), fault],
        ),
    );
    put(
        "/cron-jobs/{id}",
        "put",
        op(
            "Edit a scheduled job",
            "Scheduling",
            vec![path("id")],
            Some(cron_body),
            ("200", "The job", Some(cron_job)),
            &[("400", "Invalid cron expression"), not_found],
        ),
    );
    put(
        "/cron-jobs/{id}",
        "delete",
        op(
            "Delete a scheduled job",
            "Scheduling",
            vec![path("id")],
            None,
            done("Deleted"),
            &[not_found, fault],
        ),
    );
    put(
        "/uploads/trigger",
        "post",
        op(
            "Run an upload cycle now",
            "Uploads",
            vec![],
            None,
            done("Triggered"),
            &[],
        ),
    );

    // ── Broadlink ────────────────────────────────────────────────────────────
    put(
        "/connectors/broadlink/status",
        "get",
        op(
            "Broadlink connector status",
            "Broadlink",
            vec![],
            None,
            (
                "200",
                "Status",
                Some(object(
                    json!({ "status": json!({ "type": "object" }) }),
                    &["status"],
                )),
            ),
            &[],
        ),
    );
    put(
        "/connectors/broadlink/devices",
        "get",
        op(
            "List Broadlink devices",
            "Broadlink",
            vec![],
            None,
            ("200", "Devices", Some(list(device.clone()))),
            &[fault],
        ),
    );
    put(
        "/connectors/broadlink/devices",
        "post",
        op(
            "Add a Broadlink device",
            "Broadlink",
            vec![],
            Some(object(
                json!({ "name": text(), "host": text(), "mac": text(), "deviceType": text(),
                    "model": json!({ "type": ["string", "null"] }) }),
                &["name", "host", "mac", "deviceType"],
            )),
            ("201", "The device", Some(device)),
            &[fault],
        ),
    );
    put(
        "/connectors/broadlink/devices/{id}",
        "delete",
        op(
            "Remove a Broadlink device",
            "Broadlink",
            vec![path("id")],
            None,
            done("Removed"),
            &[not_found, fault],
        ),
    );
    put(
        "/connectors/broadlink/discover",
        "post",
        op(
            "Scan the network for Broadlink devices; results also arrive as `broadlink.device.discovered`",
            "Broadlink",
            vec![],
            None,
            ("200", "Devices found", Some(json!({ "type": "object" }))),
            &[],
        ),
    );
    put(
        "/connectors/broadlink/commands",
        "get",
        op(
            "List learned commands",
            "Broadlink",
            vec![
                query("device_id", uuid(), "Only this device's commands."),
                query("category", text(), "Only this category."),
            ],
            None,
            ("200", "Commands", Some(list(command.clone()))),
            &[fault],
        ),
    );
    put(
        "/connectors/broadlink/commands",
        "post",
        op(
            "Save a command",
            "Broadlink",
            vec![],
            Some(object(
                json!({ "deviceId": json!({ "type": ["string", "null"], "format": "uuid" }),
                    "name": text(), "slug": text(), "code": text(), "codeType": text(),
                    "category": json!({ "type": ["string", "null"] }) }),
                &["name", "slug", "code", "codeType"],
            )),
            ("201", "The command", Some(command)),
            &[fault],
        ),
    );
    put(
        "/connectors/broadlink/commands/{id}",
        "put",
        op(
            "Edit a command; omitted fields keep their value",
            "Broadlink",
            vec![path("id")],
            Some(object(
                json!({ "name": text(), "slug": text(), "code": text(), "codeType": text(), "category": text() }),
                &[],
            )),
            done("Saved"),
            &[not_found, fault],
        ),
    );
    put(
        "/connectors/broadlink/commands/{id}",
        "delete",
        op(
            "Delete a command",
            "Broadlink",
            vec![path("id")],
            None,
            done("Deleted"),
            &[not_found, fault],
        ),
    );
    put(
        "/connectors/broadlink/devices/{id}/learn",
        "post",
        op(
            "Put a device into learn mode; the code arrives as `broadlink.learn.result`",
            "Broadlink",
            vec![path("id")],
            Some(object(
                json!({ "signalType": json!({ "type": "string", "enum": ["ir", "rf"] }) }),
                &[],
            )),
            ("200", "Learning started", Some(json!({ "type": "object" }))),
            &[
                ("404", "Device not found"),
                ("409", "Learning already in progress"),
            ],
        ),
    );
    put(
        "/connectors/broadlink/learn/cancel",
        "post",
        op(
            "Cancel learn mode",
            "Broadlink",
            vec![],
            None,
            done("Cancelled"),
            &[],
        ),
    );
    put(
        "/connectors/broadlink/commands/{id}/send",
        "post",
        op(
            "Transmit a learned command",
            "Broadlink",
            vec![path("id")],
            None,
            done("Sent"),
            &[("404", "Command not found"), ("502", "The device refused")],
        ),
    );

    // ── Queues ───────────────────────────────────────────────────────────────
    put(
        "/queues",
        "get",
        op(
            "The job queues with their counters",
            "Queues",
            vec![],
            None,
            ("200", "One row per queue", Some(list(queue))),
            &[fault],
        ),
    );
    put(
        "/queues/{queue}/jobs",
        "get",
        op(
            "One queue's jobs, newest first",
            "Queues",
            vec![
                json!({ "name": "queue", "in": "path", "required": true, "schema": text() }),
                query("status", text(), "pending, processing, succeeded or dead."),
                query("limit", int(), "Page size."),
                query("offset", int(), "Rows to skip."),
            ],
            None,
            ("200", "Jobs", Some(list(job))),
            &[fault],
        ),
    );
    put(
        "/jobs/{id}/retry",
        "post",
        op(
            "Put a failed job back in its queue",
            "Queues",
            vec![path("id")],
            None,
            done("Queued again"),
            &[not_found, fault],
        ),
    );
    put(
        "/jobs/{id}",
        "delete",
        op(
            "Delete a job from its queue",
            "Queues",
            vec![path("id")],
            None,
            done("Deleted"),
            &[not_found, fault],
        ),
    );

    // ── Slide folders and Keynote ────────────────────────────────────────────
    let highlight_part = object(json!({ "text": text(), "hit": flag() }), &["text", "hit"]);
    put(
        "/ppt/folders",
        "get",
        op(
            "List slide folders",
            "Presentations",
            vec![],
            None,
            ("200", "Folders", Some(wrapped(list(folder.clone())))),
            &[fault],
        ),
    );
    put(
        "/ppt/folders",
        "post",
        op(
            "Add a slide folder (an existing path is renamed)",
            "Presentations",
            vec![],
            Some(object(
                json!({ "path": text(), "name": text() }),
                &["path", "name"],
            )),
            ("200", "The folder", Some(wrapped(folder))),
            &[("400", "Missing path or name")],
        ),
    );
    put(
        "/ppt/folders/{id}",
        "delete",
        op(
            "Remove a slide folder",
            "Presentations",
            vec![path("id")],
            None,
            ("200", "Removed", Some(success.clone())),
            &[fault],
        ),
    );
    put(
        "/ppt/files",
        "get",
        op(
            "Search the slide folders for decks",
            "Presentations",
            vec![
                query(
                    "filter",
                    text(),
                    "Words from the title or slide text, or a hymn number; accents and case are ignored and each word matches as a prefix. Empty lists every deck by title.",
                ),
                query("folderId", uuid(), "Only this folder's decks."),
                query(
                    "limit",
                    json!({ "type": "integer", "minimum": 1, "maximum": 200, "default": 5 }),
                    "How many decks to return.",
                ),
            ],
            None,
            (
                "200",
                "Matching decks, best match first: hymn number, then title, then slide text",
                Some(wrapped(list(object(
                    json!({
                        "id": text(), "name": text(), "path": text(), "folderId": text(),
                        "title": text(),
                        "number": json!({ "type": ["integer", "null"] }),
                        "matchedIn": json!({ "type": ["string", "null"], "enum": ["number", "title", "text", null] }),
                        "titleParts": list(highlight_part.clone()),
                        "snippetParts": list(highlight_part),
                    }),
                    &[
                        "id",
                        "name",
                        "path",
                        "folderId",
                        "title",
                        "titleParts",
                        "snippetParts",
                    ],
                )))),
            ),
            &[fault],
        ),
    );
    put(
        "/keynote/status",
        "get",
        op(
            "Keynote status",
            "Keynote",
            vec![],
            None,
            ("200", "Status", Some(wrapped(keynote_status))),
            &[],
        ),
    );
    put(
        "/keynote/open",
        "post",
        keynote(
            "Open a deck in Keynote",
            Some(object(json!({ "file_path": text() }), &["file_path"])),
        ),
    );
    put(
        "/keynote/goto",
        "post",
        keynote(
            "Go to a slide",
            Some(object(
                json!({ "slide": json!({ "type": "integer", "minimum": 1 }) }),
                &["slide"],
            )),
        ),
    );
    for (route, summary) in [
        ("next", "Next slide"),
        ("prev", "Previous slide"),
        ("first", "First slide"),
        ("last", "Last slide"),
        ("start", "Start the slideshow"),
        ("stop", "Stop the slideshow"),
        ("close_all", "Close every Keynote document"),
    ] {
        put(&format!("/keynote/{route}"), "post", keynote(summary, None));
    }

    Value::Object(paths)
}

#[cfg(test)]
mod tests {
    #[test]
    fn every_operation_is_a_complete_one() {
        let paths = super::paths();
        let mut count = 0;
        for (route, methods) in paths.as_object().unwrap() {
            assert!(route.starts_with("/api/"), "{route}");
            for (method, operation) in methods.as_object().unwrap() {
                count += 1;
                assert!(["get", "post", "put", "delete"].contains(&method.as_str()));
                assert!(operation["summary"].is_string(), "{method} {route}");
                assert!(
                    operation["responses"]["401"].is_object(),
                    "{method} {route}"
                );
                let ok = operation["responses"]
                    .as_object()
                    .unwrap()
                    .keys()
                    .any(|s| s.starts_with('2'));
                assert!(ok, "{method} {route} has no success response");
            }
        }
        assert!(count >= 45, "only {count} operations");
    }
}
