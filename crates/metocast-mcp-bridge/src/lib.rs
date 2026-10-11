//! The stdio side of Metocast's MCP server, run as `metocast-server mcp-stdio`.
//!
//! Some MCP clients (Claude Desktop's JSON config among them) can only start a program and talk
//! to it over stdin and stdout. This is that program: it reads one JSON-RPC message per line,
//! `POST`s it to Metocast's `/mcp` endpoint and writes what comes back. Beyond that it keeps the
//! `Mcp-Session-Id`, forwards the server's event stream, starts a new session when the server
//! forgot the old one, and turns every failure into a JSON-RPC error a person can read.
//!
//! Stdout carries protocol only; everything else goes to stderr. Nothing here starts a server,
//! binds a port or touches a data directory.

use std::sync::Arc;
use std::time::Duration;

use futures_util::StreamExt;
use reqwest::{Method, RequestBuilder, Response, StatusCode};
use serde_json::{Value, json};
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncWrite, AsyncWriteExt};
use tokio::sync::Mutex;
use tokio::task::{JoinHandle, JoinSet};

pub const DEFAULT_URL: &str = "http://127.0.0.1:3737/mcp";
const SESSION: &str = "mcp-session-id";
const PROTOCOL: &str = "mcp-protocol-version";
const USAGE: &str = "usage: metocast-server mcp-stdio [--check] [--url ADDRESS] [--token-file PATH]\n\
    The token comes from METOCAST_MCP_TOKEN or --token-file, the address from METOCAST_MCP_URL or --url.";
/// How long a request that was already sent may keep running after the client closed stdin.
const DRAIN: Duration = Duration::from_secs(5);

pub struct Config {
    pub url: String,
    pub token: String,
}

/// Why a request got no answer the client can use.
enum Failure {
    Unreachable(reqwest::Error),
    /// The server answered with an error; carries its own message when it sent one.
    Refused(StatusCode, Option<String>),
    /// The server does not know this session: it restarted, or the session idled out.
    Gone(String),
}

impl Failure {
    fn describe(&self, url: &str) -> String {
        match self {
            Self::Unreachable(error) if error.is_connect() || error.is_timeout() => format!(
                "Metocast is not running, or the address is wrong ({url}). Open the Metocast app and \
                 check that its server is started."
            ),
            Self::Unreachable(error) => format!("Could not talk to Metocast at {url}: {error}"),
            Self::Refused(StatusCode::UNAUTHORIZED, _) => "The access token is wrong or missing. \
                Copy it again from Metocast under Connectors, AI Agents (MCP)."
                .into(),
            // "MCP is turned off. Turn it on in the Metocast app ..." is already written for people.
            Self::Refused(_, Some(message)) => message.clone(),
            Self::Refused(status, None) => format!("Metocast answered {status} at {url}."),
            Self::Gone(_) => "Metocast no longer knows this session and a new one could not be \
                started. Restart the AI app."
                .into(),
        }
    }
}

#[derive(Default)]
struct State {
    /// The client's `initialize` request, replayed when the server forgets the session.
    hello: Option<String>,
    session: Option<String>,
    protocol: Option<String>,
    /// The `GET /mcp` event stream, forwarded for as long as the session lives.
    stream: Option<JoinHandle<()>>,
}

struct Bridge {
    client: reqwest::Client,
    config: Config,
    out: Mutex<Box<dyn AsyncWrite + Send + Unpin>>,
    state: Mutex<State>,
}

impl Bridge {
    fn new(config: Config, out: impl AsyncWrite + Send + Unpin + 'static) -> Arc<Self> {
        // Redirects are not followed: the token goes only to the address it was given.
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(5))
            .build()
            .expect("reqwest builds with these options like it does with none");
        Arc::new(Self {
            client,
            config,
            out: Mutex::new(Box::new(out)),
            state: Mutex::default(),
        })
    }

    /// One line on stdout. A closed stdout means the client is gone, and reading stdin ends too.
    async fn emit(&self, line: &str) {
        let mut out = self.out.lock().await;
        let _ = out.write_all(format!("{line}\n").as_bytes()).await;
        let _ = out.flush().await;
    }

    fn request(
        &self,
        method: Method,
        session: Option<&str>,
        protocol: Option<&str>,
    ) -> RequestBuilder {
        let mut request = self.client.request(method, &self.config.url);
        if !self.config.token.is_empty() {
            request = request.bearer_auth(&self.config.token);
        }
        if let Some(id) = session {
            request = request.header(SESSION, id);
        }
        if let Some(version) = protocol {
            request = request.header(PROTOCOL, version);
        }
        request
    }

    async fn send(
        &self,
        body: &str,
        session: Option<&str>,
        protocol: Option<&str>,
    ) -> Result<Response, Failure> {
        let response = self
            .request(Method::POST, session, protocol)
            .header("content-type", "application/json")
            .header("accept", "application/json, text/event-stream")
            .body(body.to_string())
            .send()
            .await
            .map_err(Failure::Unreachable)?;
        let status = response.status();
        if status.is_success() {
            return Ok(response);
        }
        if status == StatusCode::NOT_FOUND
            && let Some(id) = session
        {
            return Err(Failure::Gone(id.to_string()));
        }
        let message = response
            .json::<Value>()
            .await
            .ok()
            .and_then(|body| body["error"]["message"].as_str().map(str::to_string));
        Err(Failure::Refused(status, message))
    }

    /// `send` with the session the bridge currently holds.
    async fn post(&self, body: &str) -> Result<Response, Failure> {
        let (session, protocol) = {
            let state = self.state.lock().await;
            (state.session.clone(), state.protocol.clone())
        };
        self.send(body, session.as_deref(), protocol.as_deref())
            .await
    }

    /// Writes what the server answered, one line per message, and returns the last line.
    /// A notification gets `202` and an empty body, so nothing is written.
    async fn deliver(&self, response: Response) -> Option<String> {
        let streamed = response
            .headers()
            .get("content-type")
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| value.contains("text/event-stream"));
        if streamed {
            return self.relay(response).await;
        }
        let line = one_line(&response.text().await.ok()?)?;
        self.emit(&line).await;
        Some(line)
    }

    /// Writes each server-sent event of `response` as it arrives; returns the last one.
    async fn relay(&self, response: Response) -> Option<String> {
        let mut chunks = response.bytes_stream();
        let mut pending = Vec::new();
        let mut data = String::new();
        let mut last = None;
        while let Some(Ok(chunk)) = chunks.next().await {
            pending.extend_from_slice(&chunk);
            while let Some(end) = pending.iter().position(|&byte| byte == b'\n') {
                let raw: Vec<u8> = pending.drain(..=end).collect();
                let line = String::from_utf8_lossy(&raw);
                let line = line.trim_end_matches(['\r', '\n']);
                if let Some(rest) = line.strip_prefix("data:") {
                    if !data.is_empty() {
                        data.push('\n');
                    }
                    data.push_str(rest.strip_prefix(' ').unwrap_or(rest));
                } else if line.is_empty()
                    && let Some(event) = one_line(&std::mem::take(&mut data))
                {
                    self.emit(&event).await;
                    last = Some(event);
                }
            }
        }
        last
    }

    async fn exchange(self: &Arc<Self>, body: &str) -> Result<(), Failure> {
        let response = match self.post(body).await {
            Err(Failure::Gone(stale)) => {
                self.rejoin(&stale).await?;
                self.post(body).await?
            }
            other => other?,
        };
        self.deliver(response).await;
        Ok(())
    }

    async fn initialize(self: &Arc<Self>, body: &str) -> Result<(), Failure> {
        // Held until the session is stored, so the client's next message already carries it.
        let mut state = self.state.lock().await;
        let response = self.send(body, None, None).await?;
        let session = header(&response, SESSION);
        let reply = self.deliver(response).await;
        state.protocol = reply
            .and_then(|line| serde_json::from_str::<Value>(&line).ok())
            .and_then(|reply| {
                reply["result"]["protocolVersion"]
                    .as_str()
                    .map(str::to_string)
            });
        state.session = session;
        state.hello = Some(body.to_string());
        self.follow(&mut state);
        Ok(())
    }

    /// The server forgot `stale`: initialize again with the client's own `initialize`, which the
    /// client never notices. Several requests can fail at once; the first one to get here does it.
    async fn rejoin(self: &Arc<Self>, stale: &str) -> Result<(), Failure> {
        let mut state = self.state.lock().await;
        if state.session.as_deref() != Some(stale) {
            return Ok(());
        }
        let Some(hello) = state.hello.clone() else {
            return Err(Failure::Gone(stale.to_string()));
        };
        let response = self.send(&hello, None, None).await?;
        state.session = header(&response, SESSION);
        let ready = json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }).to_string();
        self.send(&ready, state.session.as_deref(), state.protocol.as_deref())
            .await?;
        self.follow(&mut state);
        Ok(())
    }

    /// Starts forwarding the server's event stream for the current session.
    fn follow(self: &Arc<Self>, state: &mut State) {
        if let Some(old) = state.stream.take() {
            old.abort();
        }
        let Some(session) = state.session.clone() else {
            return;
        };
        let bridge = Arc::clone(self);
        let protocol = state.protocol.clone();
        state.stream = Some(tokio::spawn(async move {
            let mut wait = Duration::from_secs(1);
            loop {
                let request = bridge
                    .request(Method::GET, Some(&session), protocol.as_deref())
                    .header("accept", "text/event-stream");
                match request.send().await {
                    Ok(response) if response.status().is_success() => {
                        wait = Duration::from_secs(1);
                        bridge.relay(response).await;
                    }
                    // A 4xx (404 included) ends it: the next request notices a lost session,
                    // starts a new one and a new stream.
                    Ok(response) if !response.status().is_server_error() => return,
                    _ => {}
                }
                tokio::time::sleep(wait).await;
                wait = (wait * 2).min(Duration::from_secs(30));
            }
        }));
    }

    async fn handle(self: Arc<Self>, line: String, message: Value) {
        let outcome = if message["method"] == "initialize" {
            self.initialize(&line).await
        } else {
            self.exchange(&line).await
        };
        if let Err(failure) = outcome {
            let text = failure.describe(&self.config.url);
            eprintln!("metocast mcp-stdio: {text}");
            // A notification has no one to answer.
            if let Some(id) = message.get("id") {
                let error = json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "error": { "code": -32000, "message": text },
                });
                self.emit(&error.to_string()).await;
            }
        }
    }

    /// Ends the session so the agent leaves Metocast's connected list at once.
    async fn close(&self) {
        let mut state = self.state.lock().await;
        if let Some(stream) = state.stream.take() {
            stream.abort();
        }
        if let Some(session) = state.session.take() {
            let goodbye = self.request(Method::DELETE, Some(&session), None).send();
            let _ = tokio::time::timeout(Duration::from_secs(2), goodbye).await;
        }
    }

    /// The `--check` round trip: initialize, list the tools, hang up.
    async fn probe(&self) -> Result<String, Failure> {
        let version = "2025-06-18";
        let hello = json!({
            "jsonrpc": "2.0", "id": 1, "method": "initialize",
            "params": {
                "protocolVersion": version,
                "capabilities": {},
                "clientInfo": { "name": "metocast-mcp-check", "version": env!("CARGO_PKG_VERSION") },
            },
        });
        let response = self.send(&hello.to_string(), None, None).await?;
        let session = header(&response, SESSION);
        let hello = response.json::<Value>().await.unwrap_or(Value::Null);
        if hello["result"].is_null() {
            let message = hello["error"]["message"].as_str();
            return Err(Failure::Refused(
                StatusCode::OK,
                Some(
                    message
                        .unwrap_or("Metocast sent an answer this check does not understand.")
                        .into(),
                ),
            ));
        }
        let session = session.as_deref();
        let ready = json!({ "jsonrpc": "2.0", "method": "notifications/initialized" });
        self.send(&ready.to_string(), session, Some(version))
            .await?;
        let list = json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" });
        let tools = self
            .send(&list.to_string(), session, Some(version))
            .await?
            .json::<Value>()
            .await
            .unwrap_or(Value::Null);
        let _ = self.request(Method::DELETE, session, None).send().await;
        Ok(format!(
            "Metocast {} answers at {} with {} tools.",
            hello["result"]["serverInfo"]["version"]
                .as_str()
                .unwrap_or("(unknown version)"),
            self.config.url,
            tools["result"]["tools"].as_array().map_or(0, Vec::len),
        ))
    }
}

fn header(response: &Response, name: &str) -> Option<String> {
    response
        .headers()
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string)
}

/// The message on one line, as stdio framing needs, or `None` when there is none.
fn one_line(text: &str) -> Option<String> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    Some(match serde_json::from_str::<Value>(text) {
        // Pretty-printed JSON would break the framing.
        Ok(value) if text.contains('\n') => value.to_string(),
        _ => text.to_string(),
    })
}

/// Serves one client until `input` closes: every line is a message for Metocast, every line of
/// `output` an answer or a notification from it.
pub async fn pipe(
    config: Config,
    input: impl AsyncBufRead + Unpin,
    output: impl AsyncWrite + Send + Unpin + 'static,
) {
    let bridge = Bridge::new(config, output);
    let mut lines = input.lines();
    let mut running = JoinSet::new();
    while let Ok(Some(line)) = lines.next_line().await {
        if line.trim().is_empty() {
            continue;
        }
        let message: Value = serde_json::from_str(&line).unwrap_or(Value::Null);
        let handling = Arc::clone(&bridge).handle(line, message.clone());
        if message.get("method").is_some() && message.get("id").is_none() {
            // A notification is answered `202` at once, and the request behind it (`initialized`,
            // then `tools/list`) must not overtake it.
            handling.await;
        } else {
            // A task per request: a long `events_poll` must not hold up the calls behind it.
            running.spawn(handling);
        }
        while running.try_join_next().is_some() {}
    }
    // The client is done. What it already asked for may finish, briefly.
    let _ = tokio::time::timeout(DRAIN, async {
        while running.join_next().await.is_some() {}
    })
    .await;
    bridge.close().await;
}

/// `mcp-stdio --check`: the readable result, or the readable reason it failed.
pub async fn check(config: Config) -> Result<String, String> {
    let url = config.url.clone();
    let bridge = Bridge::new(config, tokio::io::sink());
    match tokio::time::timeout(Duration::from_secs(15), bridge.probe()).await {
        Ok(result) => result.map_err(|failure| failure.describe(&url)),
        Err(_) => Err(format!(
            "Metocast did not answer within 15 seconds at {url}."
        )),
    }
}

/// `metocast-server mcp-stdio [--check] [--url ADDRESS] [--token-file PATH]`; returns the exit
/// code. `args` are the arguments after `mcp-stdio`.
pub async fn cli(mut args: impl Iterator<Item = String>) -> i32 {
    let mut config = Config {
        url: std::env::var("METOCAST_MCP_URL").unwrap_or_else(|_| DEFAULT_URL.into()),
        token: std::env::var("METOCAST_MCP_TOKEN").unwrap_or_default(),
    };
    let mut check_only = false;
    while let Some(flag) = args.next() {
        match flag.as_str() {
            "--check" => check_only = true,
            "--url" => match args.next() {
                Some(url) => config.url = url,
                None => return usage(),
            },
            "--token-file" => {
                let Some(path) = args.next() else {
                    return usage();
                };
                match std::fs::read_to_string(&path) {
                    Ok(text) => config.token = text.trim().to_string(),
                    Err(error) => {
                        eprintln!("metocast mcp-stdio: cannot read {path}: {error}");
                        return 2;
                    }
                }
            }
            _ => return usage(),
        }
    }
    let Ok(url) = reqwest::Url::parse(&config.url) else {
        eprintln!("metocast mcp-stdio: {} is not a valid address", config.url);
        return 2;
    };
    let host = url.host_str().unwrap_or_default();
    if url.scheme() == "http" && !matches!(host, "localhost" | "127.0.0.1" | "[::1]") {
        eprintln!(
            "metocast mcp-stdio: warning: {host} is not this Mac and the address is not encrypted, \
             so the access token crosses the network in clear text."
        );
    }
    if check_only {
        return match check(config).await {
            Ok(summary) => {
                println!("{summary}");
                0
            }
            Err(reason) => {
                eprintln!("{reason}");
                1
            }
        };
    }
    pipe(
        config,
        tokio::io::BufReader::new(tokio::io::stdin()),
        tokio::io::stdout(),
    )
    .await;
    0
}

fn usage() -> i32 {
    eprintln!("{USAGE}");
    2
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex as StdMutex;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    use axum::Router;
    use axum::extract::State;
    use axum::http::{HeaderMap, StatusCode, header};
    use axum::response::{IntoResponse, Response};
    use axum::routing::post;
    use tokio::io::{BufReader, DuplexStream, Lines};

    use super::*;

    const TOKEN: &str = "secret";

    #[derive(Clone)]
    struct Seen {
        verb: &'static str,
        session: Option<String>,
        protocol: Option<String>,
        body: String,
    }

    /// A stand-in for Metocast's `/mcp`: sessions, `202` for notifications, and switches for the
    /// answers a real server gives when something is wrong.
    #[derive(Default)]
    struct Fake {
        seen: StdMutex<Vec<Seen>>,
        sessions: StdMutex<Vec<String>>,
        issued: AtomicUsize,
        refuse: StdMutex<Option<(u16, String)>>,
        streamed: AtomicBool,
        /// Whether the event stream carries a notification (an empty stream just ends).
        notify: AtomicBool,
    }

    impl Fake {
        fn log(&self, verb: &'static str, headers: &HeaderMap, body: &str) -> Option<String> {
            let text = |name: &str| {
                headers
                    .get(name)
                    .and_then(|value| value.to_str().ok())
                    .map(str::to_string)
            };
            let session = text("mcp-session-id");
            self.seen.lock().unwrap().push(Seen {
                verb,
                session: session.clone(),
                protocol: text("mcp-protocol-version"),
                body: body.to_string(),
            });
            session
        }

        fn known(&self, session: &Option<String>) -> bool {
            session
                .as_ref()
                .is_none_or(|id| self.sessions.lock().unwrap().contains(id))
        }

        fn requests(&self, verb: &str) -> Vec<Seen> {
            let seen = self.seen.lock().unwrap();
            seen.iter()
                .filter(|request| request.verb == verb)
                .cloned()
                .collect()
        }
    }

    async fn post_mcp(State(fake): State<Arc<Fake>>, headers: HeaderMap, body: String) -> Response {
        let session = fake.log("POST", &headers, &body);
        if let Some((status, text)) = fake.refuse.lock().unwrap().clone() {
            return (StatusCode::from_u16(status).unwrap(), text).into_response();
        }
        let bearer = headers
            .get("authorization")
            .and_then(|value| value.to_str().ok());
        if bearer.is_none_or(|value| value != format!("Bearer {TOKEN}")) {
            return StatusCode::UNAUTHORIZED.into_response();
        }
        if !fake.known(&session) {
            return StatusCode::NOT_FOUND.into_response();
        }
        let message: Value = serde_json::from_str(&body).unwrap();
        let Some(id) = message.get("id") else {
            return StatusCode::ACCEPTED.into_response();
        };
        let method = message["method"].as_str().unwrap_or_default();
        let result = match method {
            "initialize" => json!({
                "protocolVersion": "2025-06-18",
                "serverInfo": { "name": "metocast", "version": "9.9.9" },
            }),
            "tools/list" => json!({ "tools": [{ "name": "a" }, { "name": "b" }] }),
            _ => json!({ "method": method }),
        };
        let reply = json!({ "jsonrpc": "2.0", "id": id, "result": result });
        let mut response = if fake.streamed.load(Ordering::SeqCst) {
            let progress = json!({ "jsonrpc": "2.0", "method": "notifications/progress" });
            let body = format!("data: {progress}\n\ndata: {reply}\n\n");
            ([(header::CONTENT_TYPE, "text/event-stream")], body).into_response()
        } else {
            axum::Json(reply).into_response()
        };
        if method == "initialize" {
            let session = format!("session-{}", fake.issued.fetch_add(1, Ordering::SeqCst) + 1);
            fake.sessions.lock().unwrap().push(session.clone());
            response
                .headers_mut()
                .insert("mcp-session-id", session.parse().unwrap());
        }
        response
    }

    async fn get_mcp(State(fake): State<Arc<Fake>>, headers: HeaderMap) -> Response {
        let session = fake.log("GET", &headers, "");
        if session.is_none() || !fake.known(&session) {
            return StatusCode::NOT_FOUND.into_response();
        }
        let note = json!({
            "jsonrpc": "2.0",
            "method": "notifications/resources/updated",
            "params": { "uri": "metocast://state/events" },
        });
        let body = if fake.notify.load(Ordering::SeqCst) {
            format!("data: {note}\n\n")
        } else {
            String::new()
        };
        ([(header::CONTENT_TYPE, "text/event-stream")], body).into_response()
    }

    async fn delete_mcp(State(fake): State<Arc<Fake>>, headers: HeaderMap) -> StatusCode {
        fake.log("DELETE", &headers, "");
        StatusCode::OK
    }

    async fn serve() -> (String, Arc<Fake>) {
        let fake = Arc::new(Fake::default());
        let app = Router::new()
            .route("/mcp", post(post_mcp).get(get_mcp).delete(delete_mcp))
            .with_state(Arc::clone(&fake));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/mcp", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, app).await });
        (url, fake)
    }

    /// The two ends of the bridge's pipes, the way an AI app sees them.
    struct Wire {
        to_bridge: Option<DuplexStream>,
        from_bridge: Lines<BufReader<DuplexStream>>,
        bridge: JoinHandle<()>,
    }

    fn connect(url: &str) -> Wire {
        let (to_bridge, input) = tokio::io::duplex(1 << 16);
        let (output, from_bridge) = tokio::io::duplex(1 << 16);
        let config = Config {
            url: url.to_string(),
            token: TOKEN.to_string(),
        };
        Wire {
            to_bridge: Some(to_bridge),
            from_bridge: BufReader::new(from_bridge).lines(),
            bridge: tokio::spawn(pipe(config, BufReader::new(input), output)),
        }
    }

    impl Wire {
        async fn send(&mut self, line: &str) {
            let input = self.to_bridge.as_mut().unwrap();
            input
                .write_all(format!("{line}\n").as_bytes())
                .await
                .unwrap();
        }

        async fn next(&mut self) -> Value {
            let line = tokio::time::timeout(Duration::from_secs(5), self.from_bridge.next_line())
                .await
                .expect("no line within 5 seconds")
                .unwrap()
                .expect("the bridge closed its output");
            serde_json::from_str(&line).unwrap()
        }

        async fn initialize(&mut self) {
            let hello = json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
                "protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": { "name": "t" } } });
            self.send(&hello.to_string()).await;
            assert_eq!(self.next().await["id"], 1);
        }

        async fn close(mut self) {
            self.to_bridge = None;
            tokio::time::timeout(Duration::from_secs(5), self.bridge)
                .await
                .expect("the bridge did not exit when stdin closed")
                .unwrap();
        }
    }

    #[tokio::test]
    async fn requests_get_answers_and_notifications_get_none() {
        let (url, fake) = serve().await;
        let mut wire = connect(&url);
        wire.initialize().await;
        wire.send(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#)
            .await;
        wire.send(r#"{"jsonrpc":"2.0","id":2,"method":"ping"}"#)
            .await;
        // The notification produced no line, so the next one is the ping's answer.
        let reply = wire.next().await;
        assert_eq!(reply["id"], 2);
        assert_eq!(reply["result"]["method"], "ping");

        let posts = fake.requests("POST");
        assert_eq!(posts.len(), 3);
        assert_eq!(posts[0].session, None, "initialize carries no session");
        assert_eq!(posts[2].session.as_deref(), Some("session-1"));
        assert_eq!(posts[2].protocol.as_deref(), Some("2025-06-18"));
    }

    #[tokio::test]
    async fn a_streamed_reply_is_forwarded_event_by_event() {
        let (url, fake) = serve().await;
        let mut wire = connect(&url);
        wire.initialize().await;
        fake.streamed.store(true, Ordering::SeqCst);
        wire.send(r#"{"jsonrpc":"2.0","id":2,"method":"ping"}"#)
            .await;
        assert_eq!(wire.next().await["method"], "notifications/progress");
        assert_eq!(wire.next().await["id"], 2);
    }

    #[tokio::test]
    async fn the_server_stream_is_forwarded() {
        let (url, fake) = serve().await;
        fake.notify.store(true, Ordering::SeqCst);
        let mut wire = connect(&url);
        wire.initialize().await;
        let note = wire.next().await;
        assert_eq!(note["method"], "notifications/resources/updated");
        assert_eq!(note["params"]["uri"], "metocast://state/events");
    }

    #[tokio::test]
    async fn a_forgotten_session_is_replaced_and_the_request_retried() {
        let (url, fake) = serve().await;
        let mut wire = connect(&url);
        wire.initialize().await;
        // Metocast restarted: the old session is unknown now.
        fake.sessions.lock().unwrap().clear();
        wire.send(r#"{"jsonrpc":"2.0","id":7,"method":"tools/list"}"#)
            .await;
        let reply = wire.next().await;
        assert_eq!(reply["id"], 7);
        assert_eq!(reply["result"]["tools"].as_array().unwrap().len(), 2);

        // initialize, the failed tools/list, initialize again, initialized, the retry.
        let posts = fake.requests("POST");
        let sent: Vec<(Option<&str>, bool, bool)> = posts
            .iter()
            .map(|p| {
                (
                    p.session.as_deref(),
                    p.body.contains("initialize\""),
                    p.body.contains("tools/list"),
                )
            })
            .collect();
        assert_eq!(sent.len(), 5);
        assert_eq!(sent[1], (Some("session-1"), false, true));
        assert_eq!(sent[2], (None, true, false));
        assert_eq!(sent[3].0, Some("session-2"));
        assert!(posts[3].body.contains("notifications/initialized"));
        assert_eq!(sent[4], (Some("session-2"), false, true));
    }

    #[tokio::test]
    async fn failures_are_errors_a_person_can_read() {
        let (url, fake) = serve().await;
        let mut wire = connect(&url);

        *fake.refuse.lock().unwrap() = Some((401, String::new()));
        wire.send(r#"{"jsonrpc":"2.0","id":1,"method":"initialize"}"#)
            .await;
        let reply = wire.next().await;
        assert_eq!(reply["id"], 1);
        assert!(
            reply["error"]["message"]
                .as_str()
                .unwrap()
                .contains("access token")
        );

        let off =
            "MCP is turned off. Turn it on in the Metocast app under Connectors, AI Agents (MCP).";
        let body =
            json!({ "jsonrpc": "2.0", "id": null, "error": { "code": -32000, "message": off } });
        *fake.refuse.lock().unwrap() = Some((503, body.to_string()));
        wire.send(r#"{"jsonrpc":"2.0","id":2,"method":"initialize"}"#)
            .await;
        let reply = wire.next().await;
        assert_eq!(reply["id"], 2);
        assert_eq!(reply["error"]["message"], off);

        // Nobody listens on this address.
        let closed = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/mcp", closed.local_addr().unwrap());
        drop(closed);
        let mut wire = connect(&url);
        wire.send(r#"{"jsonrpc":"2.0","id":3,"method":"initialize"}"#)
            .await;
        let reply = wire.next().await;
        assert_eq!(reply["id"], 3);
        assert!(
            reply["error"]["message"]
                .as_str()
                .unwrap()
                .contains("not running")
        );
    }

    #[tokio::test]
    async fn crlf_lines_are_accepted() {
        let (url, _fake) = serve().await;
        let mut wire = connect(&url);
        let input = wire.to_bridge.as_mut().unwrap();
        input
            .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":4,\"method\":\"ping\"}\r\n")
            .await
            .unwrap();
        assert_eq!(wire.next().await["id"], 4);
    }

    #[tokio::test]
    async fn closing_stdin_ends_the_bridge_and_the_session() {
        let (url, fake) = serve().await;
        let mut wire = connect(&url);
        wire.initialize().await;
        wire.close().await;
        let goodbyes = fake.requests("DELETE");
        assert_eq!(goodbyes.len(), 1);
        assert_eq!(goodbyes[0].session.as_deref(), Some("session-1"));
    }

    #[tokio::test]
    async fn check_reports_the_version_and_the_tool_count() {
        let (url, fake) = serve().await;
        let config = Config {
            url: url.clone(),
            token: TOKEN.to_string(),
        };
        let summary = check(config).await.unwrap();
        assert!(
            summary.contains("9.9.9") && summary.contains("2 tools"),
            "{summary}"
        );
        assert_eq!(fake.requests("DELETE").len(), 1);

        let config = Config {
            url,
            token: "wrong".to_string(),
        };
        assert!(check(config).await.unwrap_err().contains("access token"));
    }
}
