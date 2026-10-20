use axum::extract::ws::Message;
use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
};
use chrono::{DateTime, SubsecRound, Utc};
use metocast_core::best_effort::LogGone;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::{BTreeSet, HashMap};
use std::sync::atomic::{AtomicU32, Ordering};
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::server::{AppState, presenter, websocket};

// ── Folder management ────────────────────────────────────────────────────────

#[derive(Debug, Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct PptFolder {
    pub id: Uuid,
    pub path: String,
    pub name: String,
    pub sort_order: i32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddFolderBody {
    pub path: String,
    pub name: String,
}

pub async fn list_folders(State(state): State<AppState>) -> impl IntoResponse {
    match sqlx::query_as::<_, PptFolder>(
        "SELECT id, path, name, sort_order FROM ppt_folders ORDER BY sort_order, name",
    )
    .fetch_all(&state.pool)
    .await
    {
        Ok(folders) => (
            StatusCode::OK,
            Json(json!({ "success": true, "data": folders })),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "success": false, "error": e.to_string() })),
        ),
    }
}

pub async fn add_folder(
    State(state): State<AppState>,
    Json(body): Json<AddFolderBody>,
) -> impl IntoResponse {
    let path = body.path.trim();
    if !std::fs::metadata(path).is_ok_and(|m| m.is_dir()) {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "success": false,
                "error": format!("No such folder on the server: {path}")
            })),
        );
    }

    let result = sqlx::query_as::<_, PptFolder>(
        "INSERT INTO ppt_folders (path, name) VALUES ($1, $2) \
         ON CONFLICT (path) DO UPDATE SET name = EXCLUDED.name \
         RETURNING id, path, name, sort_order",
    )
    .bind(path)
    .bind(&body.name)
    .fetch_one(&state.pool)
    .await;

    match result {
        Ok(folder) => {
            websocket::broadcast_ppt_folders_changed(&state.ws_clients).await;
            spawn_sync(state);
            (
                StatusCode::CREATED,
                Json(json!({ "success": true, "data": folder })),
            )
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "success": false, "error": e.to_string() })),
        ),
    }
}

pub async fn remove_folder(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    match sqlx::query("DELETE FROM ppt_folders WHERE id = $1")
        .bind(id)
        .execute(&state.pool)
        .await
    {
        Ok(_) => {
            websocket::broadcast_ppt_folders_changed(&state.ws_clients).await;
            (StatusCode::OK, Json(json!({ "success": true })))
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "success": false, "error": e.to_string() })),
        ),
    }
}

// ── Search index ─────────────────────────────────────────────────────────────
//
// `ppt_decks` holds every deck in the watched folders with its title, hymn number and slide
// text. Each search first syncs it with the folders by mtime and size, so only new or changed
// decks are parsed; adding a folder syncs in the background. A folder's rows go with it
// (ON DELETE CASCADE). ponytail: the walk runs on every search; if folders ever live on a slow
// network share, move `sync_index` to a cron feature beside `pull_youtube`.

/// Decks being parsed by the running sync, 0 when idle. Sent as `ppt.index.state`.
static INDEXING: AtomicU32 = AtomicU32::new(0);
/// Serialises syncs. Holds the folders that could not be read last time, so that a missing
/// drive is logged when it goes and when it comes back, not on every search.
static SYNC: Mutex<BTreeSet<Uuid>> = Mutex::const_new(BTreeSet::new());

const HIT_START: char = '⟦';
const HIT_END: char = '⟧';
const DEFAULT_LIMIT: u32 = 5;
const MAX_LIMIT: u32 = 200;

/// A run of text in a search result; `hit` marks the words the search matched.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct HighlightPart {
    pub text: String,
    pub hit: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PptFile {
    pub id: String,
    pub name: String,
    pub path: String,
    pub folder_id: String,
    /// The file name without its extension.
    pub title: String,
    /// The hymn number, from the last "(47)" in the title.
    pub number: Option<i32>,
    /// `number`, `title` or `text`; null when nothing was searched for.
    pub matched_in: Option<String>,
    pub title_parts: Vec<HighlightPart>,
    /// A few words of slide text around the match; empty unless `matched_in` is `text`.
    pub snippet_parts: Vec<HighlightPart>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchQuery {
    pub filter: Option<String>,
    pub folder_id: Option<Uuid>,
    pub limit: Option<u32>,
}

/// The `ppt.index.state` message for the current state.
pub fn index_state_message() -> String {
    let pending = INDEXING.load(Ordering::Relaxed);
    json!({ "type": "ppt.index.state", "indexing": pending > 0, "pending": pending }).to_string()
}

async fn set_indexing(state: &AppState, pending: u32) {
    INDEXING.store(pending, Ordering::Relaxed);
    let msg = index_state_message();
    let clients = state.ws_clients.read().await;
    for tx in clients.values() {
        tx.send(Message::Text(msg.clone().into())).log_gone();
    }
}

struct FileStat {
    path: String,
    name: String,
    modified_at: DateTime<Utc>,
    size: i64,
}

/// One folder's decks, or `None` when the folder cannot be read.
type FolderScan = (Uuid, String, Option<Vec<FileStat>>);

fn scan_folder(path: &str) -> Option<Vec<FileStat>> {
    let dir = std::fs::read_dir(path).ok()?;
    let mut files = Vec::new();
    for entry in dir.flatten() {
        let path = entry.path();
        let is_deck = path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("pptx") || e.eq_ignore_ascii_case("ppt"));
        let Ok(meta) = entry.metadata() else { continue };
        if !is_deck || !meta.is_file() {
            continue;
        }
        let Ok(modified) = meta.modified() else {
            continue;
        };
        files.push(FileStat {
            name: path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_string(),
            path: path.to_string_lossy().into_owned(),
            // Postgres keeps microseconds; compare at that precision or every file looks changed.
            modified_at: DateTime::<Utc>::from(modified).trunc_subsecs(6),
            size: i64::try_from(meta.len()).unwrap_or(i64::MAX),
        });
    }
    Some(files)
}

/// The title (the file name without its extension) and the hymn number in its last "(47)".
fn title_and_number(name: &str) -> (String, Option<i32>) {
    let title = std::path::Path::new(name)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(name)
        .trim()
        .to_string();
    let number = title.rmatch_indices('(').find_map(|(start, _)| {
        let digits = title[start + 1..].split_once(')')?.0.trim();
        if digits.is_empty() || digits.len() > 6 || !digits.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        digits.parse().ok()
    });
    (title, number)
}

/// Every line of slide text, one per line, or why the deck could not be read.
async fn deck_text(path: &str) -> Result<String, String> {
    let owned = path.to_string();
    let parsed = tokio::task::spawn_blocking(move || presenter::parse_pptx(&owned))
        .await
        .map_err(|e| format!("Reading the deck stopped: {e}"))??;
    Ok(parsed
        .slides
        .iter()
        .flat_map(|slide| &slide.paragraphs)
        .flat_map(|paragraph| &paragraph.lines)
        .map(|line| line.trim())
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n"))
}

/// Brings `ppt_decks` in line with the watched folders. `wait` makes it queue behind a running
/// sync; without it a running sync means this one is skipped and the search uses the index as is.
pub async fn sync_index(state: &AppState, wait: bool) {
    let mut unreadable = if wait {
        SYNC.lock().await
    } else {
        match SYNC.try_lock() {
            Ok(guard) => guard,
            Err(_) => return,
        }
    };
    if let Err(error) = sync_locked(state, &mut unreadable).await {
        tracing::warn!(%error, "slide index: sync failed, searching the index as it is");
    }
}

async fn sync_locked(state: &AppState, unreadable: &mut BTreeSet<Uuid>) -> Result<(), String> {
    let started = std::time::Instant::now();
    let folders = sqlx::query_as::<_, PptFolder>(
        "SELECT id, path, name, sort_order FROM ppt_folders ORDER BY sort_order, name",
    )
    .fetch_all(&state.pool)
    .await
    .map_err(|e| e.to_string())?;

    let scans: Vec<FolderScan> = tokio::task::spawn_blocking(move || {
        folders
            .into_iter()
            .map(|f| {
                let files = scan_folder(&f.path);
                (f.id, f.path, files)
            })
            .collect()
    })
    .await
    .map_err(|e| e.to_string())?;

    let stored: HashMap<String, (Uuid, DateTime<Utc>, i64)> =
        sqlx::query_as::<_, (String, Uuid, DateTime<Utc>, i64)>(
            "SELECT path, folder_id, modified_at, size FROM ppt_decks",
        )
        .fetch_all(&state.pool)
        .await
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|(path, folder_id, modified_at, size)| (path, (folder_id, modified_at, size)))
        .collect();

    let mut changed: Vec<(Uuid, &FileStat)> = Vec::new();
    let mut gone: Vec<String> = Vec::new();
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    for (folder_id, folder_path, files) in &scans {
        let Some(files) = files else {
            // Keep a missing drive's rows: it is usually asleep or unplugged, not emptied.
            if unreadable.insert(*folder_id) {
                tracing::warn!(folder = %folder_path, "slide index: folder cannot be read, keeping its decks");
            }
            continue;
        };
        if unreadable.remove(folder_id) {
            tracing::info!(folder = %folder_path, "slide index: folder can be read again");
        }
        for file in files {
            seen.insert(&file.path);
            if stored.get(&file.path) != Some(&(*folder_id, file.modified_at, file.size)) {
                changed.push((*folder_id, file));
            }
        }
    }
    for (path, (folder_id, _, _)) in &stored {
        if !seen.contains(path.as_str()) && !unreadable.contains(folder_id) {
            gone.push(path.clone());
        }
    }
    unreadable.retain(|id| scans.iter().any(|(folder_id, _, _)| folder_id == id));

    let mut removed = 0;
    if !gone.is_empty() {
        removed = sqlx::query("DELETE FROM ppt_decks WHERE path = ANY($1)")
            .bind(&gone)
            .execute(&state.pool)
            .await
            .map_err(|e| e.to_string())?
            .rows_affected();
    }

    let (mut added, mut updated, mut unparseable) = (0, 0, 0);
    if !changed.is_empty() {
        set_indexing(state, u32::try_from(changed.len()).unwrap_or(u32::MAX)).await;
        for (folder_id, file) in &changed {
            let (title, number) = title_and_number(&file.name);
            let (body, parse_error) = match deck_text(&file.path).await {
                Ok(body) => (body, None),
                Err(error) => {
                    tracing::warn!(path = %file.path, %error, "slide index: only the title is searchable");
                    unparseable += 1;
                    (String::new(), Some(error))
                }
            };
            let result = sqlx::query(
                "INSERT INTO ppt_decks \
                     (path, folder_id, name, title, number, body, modified_at, size, parse_error) \
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9) \
                 ON CONFLICT (path) DO UPDATE SET folder_id = EXCLUDED.folder_id, \
                     name = EXCLUDED.name, title = EXCLUDED.title, number = EXCLUDED.number, \
                     body = EXCLUDED.body, modified_at = EXCLUDED.modified_at, \
                     size = EXCLUDED.size, parse_error = EXCLUDED.parse_error",
            )
            .bind(&file.path)
            .bind(folder_id)
            .bind(&file.name)
            .bind(&title)
            .bind(number)
            .bind(&body)
            .bind(file.modified_at)
            .bind(file.size)
            .bind(parse_error)
            .execute(&state.pool)
            .await;
            match result {
                Ok(_) if stored.contains_key(&file.path) => updated += 1,
                Ok(_) => added += 1,
                // Its stored mtime still differs, so the next sync tries it again.
                Err(error) => {
                    tracing::warn!(path = %file.path, %error, "slide index: cannot store deck")
                }
            }
        }
        set_indexing(state, 0).await;
    }

    if changed.is_empty() && removed == 0 {
        tracing::debug!("slide index: up to date");
    } else {
        tracing::info!(
            added,
            updated,
            removed,
            unparseable,
            elapsed_ms = started.elapsed().as_millis(),
            "slide index: synced"
        );
    }
    Ok(())
}

/// Indexes a newly added folder in the background, so the first search does not parse it.
pub fn spawn_sync(state: AppState) {
    tokio::spawn(async move { sync_index(&state, true).await });
}

/// `hala ur` → `hala:* & ur:*`. Only letters and digits reach `to_tsquery`, so no input can
/// make it a syntax error.
fn prefix_query(filter: &str) -> Option<String> {
    let words: Vec<String> = filter
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(|w| format!("{w}:*"))
        .collect();
    (!words.is_empty()).then(|| words.join(" & "))
}

/// Splits `ts_headline` output marked with `⟦…⟧` into parts.
fn highlight_parts(marked: &str) -> Vec<HighlightPart> {
    let mut parts = Vec::new();
    let mut hit = false;
    for piece in marked.split([HIT_START, HIT_END]) {
        if !piece.is_empty() {
            match parts.last_mut() {
                Some(HighlightPart { text, hit: last }) if *last == hit => text.push_str(piece),
                _ => parts.push(HighlightPart {
                    text: piece.to_string(),
                    hit,
                }),
            }
        }
        hit = !hit;
    }
    parts
}

#[derive(sqlx::FromRow)]
struct DeckRow {
    path: String,
    name: String,
    folder_id: Uuid,
    title: String,
    number: Option<i32>,
    matched_in: Option<String>,
    title_marked: Option<String>,
    snippet_marked: Option<String>,
}

const LIST_SQL: &str = "SELECT path, name, folder_id, title, number, \
         NULL::text AS matched_in, NULL::text AS title_marked, NULL::text AS snippet_marked \
     FROM ppt_decks WHERE $1::uuid IS NULL OR folder_id = $1 \
     ORDER BY lower(unaccent(title)), title LIMIT $2";

const SEARCH_SQL: &str = "WITH q AS (SELECT to_tsquery('metocast_search', $1) AS tsq), \
     hits AS ( \
         SELECT d.path, d.name, d.folder_id, d.title, d.number, d.body, q.tsq, \
                COALESCE(d.number = $2, false) AS number_hit, \
                to_tsvector('metocast_search', d.title) @@ q.tsq AS title_hit, \
                ts_rank(d.search, q.tsq) AS rank \
         FROM ppt_decks d, q \
         WHERE (d.search @@ q.tsq OR d.number = $2) AND ($3::uuid IS NULL OR d.folder_id = $3) \
         ORDER BY number_hit DESC, title_hit DESC, rank DESC, lower(unaccent(d.title)) \
         LIMIT $4) \
     SELECT path, name, folder_id, title, number, \
         CASE WHEN number_hit THEN 'number' WHEN title_hit THEN 'title' ELSE 'text' END AS matched_in, \
         ts_headline('metocast_search', title, tsq, 'HighlightAll=true, StartSel=⟦, StopSel=⟧') \
             AS title_marked, \
         CASE WHEN NOT number_hit AND NOT title_hit THEN ts_headline('metocast_search', \
             replace(body, E'\\n', ' / '), tsq, \
             'MaxFragments=1, MinWords=4, MaxWords=14, StartSel=⟦, StopSel=⟧') END AS snippet_marked \
     FROM hits ORDER BY number_hit DESC, title_hit DESC, rank DESC, lower(unaccent(title))";

/// Syncs the index (unless a sync is already running), then finds decks by title, hymn number
/// or slide text, best match first. Used by the HTTP handler and the WS command.
pub async fn search_files_internal(
    state: &AppState,
    filter: &str,
    folder_id: Option<Uuid>,
    limit: Option<u32>,
) -> Result<Vec<PptFile>, sqlx::Error> {
    sync_index(state, false).await;
    let limit = i64::from(limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT));
    let filter = filter.trim();
    let rows = if filter.is_empty() {
        sqlx::query_as::<_, DeckRow>(LIST_SQL)
            .bind(folder_id)
            .bind(limit)
            .fetch_all(&state.pool)
            .await?
    } else if let Some(tsquery) = prefix_query(filter) {
        let number: Option<i32> = filter
            .bytes()
            .all(|b| b.is_ascii_digit())
            .then(|| filter.parse().ok())
            .flatten();
        sqlx::query_as::<_, DeckRow>(SEARCH_SQL)
            .bind(tsquery)
            .bind(number)
            .bind(folder_id)
            .bind(limit)
            .fetch_all(&state.pool)
            .await?
    } else {
        Vec::new()
    };
    Ok(rows
        .into_iter()
        .map(|row| PptFile {
            id: row.path.clone(),
            title_parts: match &row.title_marked {
                Some(marked) => highlight_parts(marked),
                None => vec![HighlightPart {
                    text: row.title.clone(),
                    hit: false,
                }],
            },
            snippet_parts: row
                .snippet_marked
                .as_deref()
                .map(highlight_parts)
                .unwrap_or_default(),
            name: row.name,
            path: row.path,
            folder_id: row.folder_id.to_string(),
            title: row.title,
            number: row.number,
            matched_in: row.matched_in,
        })
        .collect())
}

pub async fn search_files(
    State(state): State<AppState>,
    Query(query): Query<SearchQuery>,
) -> impl IntoResponse {
    let filter = query.filter.as_deref().unwrap_or("");
    match search_files_internal(&state, filter, query.folder_id, query.limit).await {
        Ok(files) => (
            StatusCode::OK,
            Json(json!({ "success": true, "data": files })),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "success": false, "error": e.to_string() })),
        ),
    }
}

// ── Keynote control (macOS only) ─────────────────────────────────────────────

#[cfg(target_os = "macos")]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenBody {
    pub file_path: String,
}

#[cfg(target_os = "macos")]
#[derive(Debug, Deserialize)]
pub struct GotoBody {
    pub slide: u32,
}

#[cfg(target_os = "macos")]
pub async fn keynote_status(State(state): State<AppState>) -> impl IntoResponse {
    let status = state.keynote_connector.get_status().await;
    (
        StatusCode::OK,
        Json(json!({ "success": true, "data": status })),
    )
}

#[cfg(target_os = "macos")]
pub async fn keynote_open(
    State(state): State<AppState>,
    Json(body): Json<OpenBody>,
) -> impl IntoResponse {
    match state.keynote_connector.open_file(&body.file_path).await {
        Ok(()) => (StatusCode::OK, Json(json!({ "success": true }))),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "success": false, "error": e })),
        ),
    }
}

#[cfg(target_os = "macos")]
pub async fn keynote_next(State(state): State<AppState>) -> impl IntoResponse {
    match state.keynote_connector.next().await {
        Ok(()) => (StatusCode::OK, Json(json!({ "success": true }))),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "success": false, "error": e })),
        ),
    }
}

#[cfg(target_os = "macos")]
pub async fn keynote_prev(State(state): State<AppState>) -> impl IntoResponse {
    match state.keynote_connector.prev().await {
        Ok(()) => (StatusCode::OK, Json(json!({ "success": true }))),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "success": false, "error": e })),
        ),
    }
}

#[cfg(target_os = "macos")]
pub async fn keynote_first(State(state): State<AppState>) -> impl IntoResponse {
    match state.keynote_connector.first().await {
        Ok(()) => (StatusCode::OK, Json(json!({ "success": true }))),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "success": false, "error": e })),
        ),
    }
}

#[cfg(target_os = "macos")]
pub async fn keynote_last(State(state): State<AppState>) -> impl IntoResponse {
    match state.keynote_connector.last().await {
        Ok(()) => (StatusCode::OK, Json(json!({ "success": true }))),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "success": false, "error": e })),
        ),
    }
}

#[cfg(target_os = "macos")]
pub async fn keynote_goto(
    State(state): State<AppState>,
    Json(body): Json<GotoBody>,
) -> impl IntoResponse {
    match state.keynote_connector.goto(body.slide).await {
        Ok(()) => (StatusCode::OK, Json(json!({ "success": true }))),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "success": false, "error": e })),
        ),
    }
}

#[cfg(target_os = "macos")]
pub async fn keynote_start(State(state): State<AppState>) -> impl IntoResponse {
    match state.keynote_connector.start_slideshow().await {
        Ok(()) => (StatusCode::OK, Json(json!({ "success": true }))),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "success": false, "error": e })),
        ),
    }
}

#[cfg(target_os = "macos")]
pub async fn keynote_stop(State(state): State<AppState>) -> impl IntoResponse {
    match state.keynote_connector.stop_slideshow().await {
        Ok(()) => (StatusCode::OK, Json(json!({ "success": true }))),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "success": false, "error": e })),
        ),
    }
}

#[cfg(target_os = "macos")]
pub async fn keynote_close_all(State(state): State<AppState>) -> impl IntoResponse {
    match state.keynote_connector.close_all().await {
        Ok(()) => (StatusCode::OK, Json(json!({ "success": true }))),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "success": false, "error": e })),
        ),
    }
}

#[cfg(not(target_os = "macos"))]
pub async fn keynote_not_implemented() -> impl IntoResponse {
    (
        StatusCode::NOT_IMPLEMENTED,
        Json(json!({ "success": false, "error": "Keynote is only available on macOS" })),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hymn_number_comes_from_the_last_parentheses() {
        assert_eq!(
            title_and_number("Ó, hála az Úrnak (47).pptx"),
            ("Ó, hála az Úrnak (47)".to_string(), Some(47))
        );
        assert_eq!(title_and_number("Zsoltár (2) - 150.pptx").1, Some(2));
        assert_eq!(title_and_number("Áldjad (ének) (12) .ppt").1, Some(12));
        assert_eq!(
            title_and_number("Hirdetések.pptx"),
            ("Hirdetések".to_string(), None)
        );
        assert_eq!(title_and_number("Ének (x1).pptx").1, None);
    }

    #[test]
    fn prefix_query_keeps_only_words() {
        assert_eq!(
            prefix_query("hála  az úr!"),
            Some("hála:* & az:* & úr:*".to_string())
        );
        assert_eq!(prefix_query("47"), Some("47:*".to_string()));
        assert_eq!(prefix_query("'&|!():*"), None);
        assert_eq!(prefix_query(""), None);
    }

    #[test]
    fn highlight_parts_split_markers() {
        let part = |text: &str, hit| HighlightPart {
            text: text.to_string(),
            hit,
        };
        assert_eq!(
            highlight_parts("Ó, ⟦hála⟧ az ⟦Úrnak⟧ (47)"),
            vec![
                part("Ó, ", false),
                part("hála", true),
                part(" az ", false),
                part("Úrnak", true),
                part(" (47)", false)
            ]
        );
        assert_eq!(highlight_parts("⟦47⟧"), vec![part("47", true)]);
        assert_eq!(highlight_parts("no hit"), vec![part("no hit", false)]);
    }
}
