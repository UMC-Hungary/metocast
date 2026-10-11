//! `METOCAST_REDACT=1`: what identifies the machine or the operator is replaced by placeholders
//! in everything the server sends (REST bodies, WebSocket messages, MCP results), so screenshots
//! and demos of any UI are safe to publish. `scripts/docs-screenshot.sh` runs with it on.
//!
//! Display only: nothing stored changes. The server still returns placeholders for paths and
//! names, so don't operate on what it returns while this is on.

use std::borrow::Cow;
use std::sync::{OnceLock, RwLock};

use axum::{body::Body, extract::Request, http::header, middleware::Next, response::Response};
use regex::Regex;

/// Where a computer name goes.
pub const HOST: &str = "studio-mac";
/// A home folder.
pub const HOME: &str = "/Users/operator";
/// A private LAN address (the RFC 5737 documentation range).
const ADDRESS: &str = "192.0.2.10";
const MASK: &str = "•••••";

#[must_use]
pub fn enabled() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| {
        std::env::var("METOCAST_REDACT")
            .is_ok_and(|value| value == "1" || value.eq_ignore_ascii_case("true"))
    })
}

static SECRETS: RwLock<Vec<String>> = RwLock::new(Vec::new());

/// Registers a token to mask wherever it appears. A no-op while redaction is off.
pub fn add_secret(value: &str) {
    if enabled()
        && value.len() >= 8
        && let Ok(mut secrets) = SECRETS.write()
        && !secrets.iter().any(|known| known == value)
    {
        secrets.push(value.to_string());
    }
}

/// This machine's names and the account's: host name, `ComputerName` and `LocalHostName` on a
/// Mac, and the user name.
fn own_names() -> &'static [String] {
    static NAMES: OnceLock<Vec<String>> = OnceLock::new();
    NAMES.get_or_init(|| {
        let run = |program: &str, args: &[&str]| {
            std::process::Command::new(program)
                .args(args)
                .output()
                .ok()
                .filter(|output| output.status.success())
                .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
        };
        let mut names: Vec<String> = Vec::new();
        names.extend(run("hostname", &[]));
        #[cfg(target_os = "macos")]
        for key in ["ComputerName", "LocalHostName", "HostName"] {
            names.extend(run("/usr/sbin/scutil", &["--get", key]));
        }
        names.extend(std::env::var("USER").ok());
        names.extend(std::env::var("USERNAME").ok());
        let mut stems: Vec<String> = names
            .iter()
            .filter_map(|name| name.strip_suffix(".local").map(str::to_string))
            .collect();
        names.append(&mut stems);
        names.retain(|name| name.chars().count() >= 4);
        names.sort_by_key(|name| std::cmp::Reverse(name.len()));
        names.dedup();
        names
    })
}

/// The rewrite, with its inputs passed in so a test can run it without the environment.
fn rewrite(text: &str, names: &[String], secrets: &[String]) -> String {
    static HOMES: OnceLock<Regex> = OnceLock::new();
    static ADDRESSES: OnceLock<Regex> = OnceLock::new();
    let homes = HOMES.get_or_init(|| {
        Regex::new(r#"(?:/Users|/home)/[^/"\\\s]+|[A-Za-z]:\\\\Users\\\\[^\\"\s]+"#).unwrap()
    });
    let addresses = ADDRESSES.get_or_init(|| {
        Regex::new(
            r"\b(?:10\.\d{1,3}\.\d{1,3}\.\d{1,3}|192\.168\.\d{1,3}\.\d{1,3}|172\.(?:1[6-9]|2\d|3[01])\.\d{1,3}\.\d{1,3})\b",
        )
        .unwrap()
    });
    let mut out = text.to_string();
    for secret in secrets {
        out = out.replace(secret.as_str(), MASK);
    }
    out = homes.replace_all(&out, HOME).into_owned();
    out = addresses.replace_all(&out, ADDRESS).into_owned();
    if !names.is_empty() {
        let alternatives: Vec<String> = names.iter().map(|name| regex::escape(name)).collect();
        if let Ok(own) = Regex::new(&format!(r"(?i)\b(?:{})\b", alternatives.join("|"))) {
            out = own.replace_all(&out, HOST).into_owned();
        }
    }
    out
}

/// `text` with the machine's identity replaced, or `text` itself when redaction is off.
#[must_use]
pub fn apply(text: &str) -> Cow<'_, str> {
    if !enabled() {
        return Cow::Borrowed(text);
    }
    let secrets = SECRETS.read().map(|s| s.clone()).unwrap_or_default();
    Cow::Owned(rewrite(text, own_names(), &secrets))
}

/// Redacts JSON and text response bodies of the routes it wraps.
pub async fn middleware(request: Request, next: Next) -> Response {
    let response = next.run(request).await;
    if !enabled() {
        return response;
    }
    let textual = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|kind| kind.starts_with("application/json") || kind.starts_with("text/"));
    if !textual {
        return response;
    }
    let (mut parts, body) = response.into_parts();
    let Ok(bytes) = axum::body::to_bytes(body, 32 * 1024 * 1024).await else {
        return Response::from_parts(parts, Body::empty());
    };
    let redacted = apply(&String::from_utf8_lossy(&bytes)).into_owned();
    parts.headers.remove(header::CONTENT_LENGTH);
    Response::from_parts(parts, Body::from(redacted))
}

#[cfg(test)]
mod tests {
    use super::rewrite;

    #[test]
    fn identity_is_replaced_and_the_rest_is_left_alone() {
        let names = vec!["Esztergomi-Mac".to_string(), "esztergomi".to_string()];
        let secrets = vec!["0a1b2c3d-aaaa-bbbb-cccc-0123456789ab".to_string()];
        let text = r#"{"label":"Metocast on Esztergomi-Mac","hostname":"esztergomi-mac.local",
            "filePath":"/Users/esztergomi/Movies/service.mov","url":"http://192.168.0.27:3737",
            "other":"http://172.20.1.5/x","public":"203.0.113.9","token":"0a1b2c3d-aaaa-bbbb-cccc-0123456789ab",
            "note":"Esztergomi-Macroon stays"}"#;
        let out = rewrite(text, &names, &secrets);
        assert!(out.contains("Metocast on studio-mac"));
        assert!(out.contains("/Users/operator/Movies/service.mov"));
        assert!(out.contains("http://192.0.2.10:3737"));
        assert!(out.contains("http://192.0.2.10/x"));
        assert!(
            out.contains("203.0.113.9"),
            "a public address is not private"
        );
        assert!(out.contains("•••••") && !out.contains("0a1b2c3d"));
        assert!(!out.contains("esztergomi") || out.contains("Esztergomi-Macroon"));
        assert!(!out.contains("192.168."));
    }
}
