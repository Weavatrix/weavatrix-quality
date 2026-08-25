//! HAR → privacy-safe network replay cassette. Cookies and raw secrets never stay.

use std::collections::BTreeSet;

use serde::Deserialize;
use serde_json::Value;
use thiserror::Error;

use crate::browser_bridge::{NetworkReplayEntry, NetworkReplayProfile, NetworkRunPolicy};
use crate::request_identity::{identify_request, request_path};

/// Raw HAR ceiling. Larger documents never become a cassette.
pub const MAX_NETWORK_CASSETTE_BYTES: usize = 8 * 1024 * 1024;

const DEFAULT_REDACTED_JSON_KEYS: &[&str] = &[
    "address",
    "authorization",
    "cookie",
    "email",
    "name",
    "password",
    "phone",
    "secret",
    "session",
    "token",
];

/// Fail-closed HAR ingest error.
#[derive(Debug, Error)]
pub enum CassetteError {
    /// Document is too large, not HAR 1.1/1.2, or origin is unusable.
    #[error("{0}")]
    Invalid(String),
    /// JSON could not be decoded.
    #[error("malformed HAR: {0}")]
    Malformed(String),
}

/// Privacy-safe profile plus honest drop reasons.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CassetteAdmission {
    /// `schema_v: 2` replay profile. Never contains cookies or request bodies.
    pub profile: NetworkReplayProfile,
    /// Same-origin JSON responses kept.
    pub captured_entries: u64,
    /// Entries dropped as cross-origin, non-JSON, oversized, or over the ceiling.
    pub omitted: u64,
    /// Human-readable drop/redaction notes. Never raw secrets.
    pub limitations: Vec<String>,
}

#[derive(Deserialize)]
struct HarFile {
    log: HarLog,
}

#[derive(Deserialize)]
struct HarLog {
    version: String,
    #[serde(default)]
    entries: Vec<HarEntry>,
}

#[derive(Deserialize)]
struct HarEntry {
    request: HarRequest,
    response: HarResponse,
}

#[derive(Deserialize)]
struct HarRequest {
    method: String,
    url: String,
    #[serde(default)]
    headers: Vec<HarHeader>,
    #[serde(default)]
    cookies: Vec<HarCookie>,
    #[serde(default, rename = "postData")]
    post_data: Option<HarPostData>,
}

#[derive(Deserialize)]
struct HarResponse {
    status: u16,
    #[serde(default)]
    headers: Vec<HarHeader>,
    content: HarContent,
}

#[derive(Deserialize)]
struct HarHeader {
    name: String,
    #[serde(default)]
    value: String,
}

#[derive(Deserialize)]
struct HarCookie {
    name: String,
    #[serde(default)]
    value: String,
}

#[derive(Deserialize)]
struct HarPostData {
    #[serde(default, rename = "mimeType")]
    mime_type: String,
    #[serde(default)]
    text: String,
}

#[derive(Deserialize)]
struct HarContent {
    #[serde(default, rename = "mimeType")]
    mime_type: String,
    #[serde(default)]
    text: String,
    #[serde(default)]
    encoding: String,
}

/// Turn a HAR 1.1/1.2 document into a same-origin JSON replay cassette.
///
/// # Errors
///
/// Unknown versions, unusable origins, oversize documents, or malformed JSON.
pub fn ingest_har(raw: &str, origin: &str) -> Result<CassetteAdmission, CassetteError> {
    if raw.len() > MAX_NETWORK_CASSETTE_BYTES {
        return Err(CassetteError::Invalid(
            "network cassette exceeds 8MiB".into(),
        ));
    }
    let expected = normalize_origin(origin).ok_or_else(|| {
        CassetteError::Invalid("network cassette origin must be an absolute http(s) URL".into())
    })?;
    let har: HarFile =
        serde_json::from_str(raw).map_err(|err| CassetteError::Malformed(err.to_string()))?;
    if har.log.version != "1.2" && har.log.version != "1.1" {
        return Err(CassetteError::Invalid(format!(
            "unknown HAR log.version {}",
            har.log.version
        )));
    }
    let keys = DEFAULT_REDACTED_JSON_KEYS
        .iter()
        .map(|key| (*key).to_owned())
        .collect::<BTreeSet<_>>();
    let policy = NetworkRunPolicy::default();
    let mut entries = Vec::new();
    let mut omitted = 0_u64;
    let mut limitations = Vec::new();
    let total = har.log.entries.len();
    for (index, entry) in har.log.entries.iter().enumerate() {
        match convert_entry(entry, &expected, &keys, &policy) {
            Ok(converted) => {
                note_secrets(entry, &mut limitations);
                entries.push(converted);
            }
            Err(reason) => {
                omitted = omitted.saturating_add(1);
                push_limit(&mut limitations, reason);
            }
        }
        if entries.len() >= policy.max_entries as usize {
            omitted = omitted.saturating_add(u64::try_from(total.saturating_sub(index.saturating_add(1))).unwrap_or(0));
            push_limit(
                &mut limitations,
                format!(
                    "network cassette hit the {}-entry ceiling",
                    policy.max_entries
                ),
            );
            break;
        }
    }
    let profile = NetworkReplayProfile {
        schema_v: 2,
        entries,
    };
    profile
        .validate(&policy)
        .map_err(|err| CassetteError::Invalid(err.to_string()))?;
    let captured_entries = u64::try_from(profile.entries.len()).unwrap_or(u64::MAX);
    Ok(CassetteAdmission {
        profile,
        captured_entries,
        omitted,
        limitations,
    })
}

fn convert_entry(
    entry: &HarEntry,
    origin: &str,
    keys: &BTreeSet<String>,
    policy: &NetworkRunPolicy,
) -> Result<NetworkReplayEntry, String> {
    let url_origin = request_origin(&entry.request.url).ok_or_else(|| {
        "network cassette omitted a request without an absolute http(s) URL".to_owned()
    })?;
    if url_origin != origin {
        return Err(format!(
            "network cassette omitted cross-origin {} {}",
            entry.request.method.to_ascii_uppercase(),
            request_path(&entry.request.url)
        ));
    }
    if !entry.response.content.encoding.is_empty() && entry.response.content.encoding != "identity"
    {
        return Err(format!(
            "network cassette omitted encoded response {} {}",
            entry.request.method.to_ascii_uppercase(),
            request_path(&entry.request.url)
        ));
    }
    let content_type = media_type(&entry.response.content.mime_type);
    if content_type != "application/json" && !content_type.ends_with("+json") {
        return Err(format!(
            "network cassette omitted non-JSON {} {}",
            entry.request.method.to_ascii_uppercase(),
            request_path(&entry.request.url)
        ));
    }
    let parsed: Value = serde_json::from_str(&entry.response.content.text).map_err(|_| {
        format!(
            "network cassette omitted non-JSON {} {}",
            entry.request.method.to_ascii_uppercase(),
            request_path(&entry.request.url)
        )
    })?;
    let body = serde_json::to_string(&redact_json(&parsed, keys, "")).map_err(|_| {
        "network cassette could not serialise a redacted JSON response".to_owned()
    })?;
    if body.len() > policy.max_body_bytes as usize {
        return Err("network cassette omitted a response over the body ceiling".into());
    }
    if !(100..=599).contains(&entry.response.status) {
        return Err("network cassette omitted a response with an invalid status".into());
    }
    let path = redact_query(&request_path(&entry.request.url), keys);
    let request_type = media_type(
        header_value(&entry.request.headers, "content-type")
            .unwrap_or(entry.request.post_data.as_ref().map_or("", |data| &data.mime_type)),
    );
    let request_body = entry
        .request
        .post_data
        .as_ref()
        .map(|data| data.text.as_bytes());
    let identity = identify_request(&entry.request.method, &path, &request_type, request_body);
    Ok(NetworkReplayEntry {
        method: identity.method,
        path: identity.path,
        status: entry.response.status,
        content_type,
        body,
        request_content_type: (!identity.content_type.is_empty()).then_some(identity.content_type),
        request_body_digest: identity.body_digest,
        graphql_operation_name: identity
            .graphql
            .as_ref()
            .and_then(|graphql| graphql.operation_name.clone()),
        graphql_query_digest: identity
            .graphql
            .as_ref()
            .map(|graphql| graphql.query_digest.clone()),
        graphql_variables_digest: identity
            .graphql
            .as_ref()
            .map(|graphql| graphql.variables_digest.clone()),
    })
}

fn note_secrets(entry: &HarEntry, limitations: &mut Vec<String>) {
    let has_cookie = !entry.request.cookies.is_empty()
        || header_value(&entry.request.headers, "cookie").is_some()
        || header_value(&entry.response.headers, "set-cookie").is_some()
        || entry.request.cookies.iter().any(|cookie| !cookie.name.is_empty() || !cookie.value.is_empty());
    if has_cookie {
        push_limit(
            limitations,
            "network cassette dropped cookies; they are never retained".into(),
        );
    }
    if header_value(&entry.request.headers, "authorization").is_some() {
        push_limit(
            limitations,
            "network cassette dropped authorization headers; they are never retained".into(),
        );
    }
}

fn redact_json(value: &Value, keys: &BTreeSet<String>, parent_key: &str) -> Value {
    if keys.contains(&parent_key.to_ascii_lowercase()) {
        return Value::String("[REDACTED]".into());
    }
    match value {
        Value::Array(items) => Value::Array(
            items
                .iter()
                .map(|item| redact_json(item, keys, ""))
                .collect(),
        ),
        Value::Object(map) => {
            let mut out = serde_json::Map::new();
            for (key, item) in map {
                out.insert(key.clone(), redact_json(item, keys, key));
            }
            Value::Object(out)
        }
        Value::String(text) if looks_sensitive(text) => Value::String("[REDACTED]".into()),
        other => other.clone(),
    }
}

fn redact_query(path: &str, keys: &BTreeSet<String>) -> String {
    let Some((base, query)) = path.split_once('?') else {
        return path.to_owned();
    };
    if query.is_empty() {
        return path.to_owned();
    }
    let rewritten = query
        .split('&')
        .map(|pair| {
            let (name, value) = pair.split_once('=').unwrap_or((pair, ""));
            if keys.contains(&name.to_ascii_lowercase()) || looks_sensitive(value) {
                format!("{name}=[REDACTED]")
            } else {
                pair.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join("&");
    format!("{base}?{rewritten}")
}

fn looks_sensitive(value: &str) -> bool {
    let trimmed = value.trim();
    is_email(trimmed) || trimmed.to_ascii_lowercase().starts_with("bearer ") || is_jwt(trimmed)
}

fn is_email(value: &str) -> bool {
    let Some((user, host)) = value.split_once('@') else {
        return false;
    };
    !user.is_empty()
        && !user.contains(char::is_whitespace)
        && host.contains('.')
        && !host.contains(char::is_whitespace)
        && !host.contains('@')
}

fn is_jwt(value: &str) -> bool {
    let parts: Vec<&str> = value.split('.').collect();
    parts.len() == 3
        && parts[0].len() >= 20
        && parts[1].len() >= 10
        && parts[2].len() >= 10
        && parts.iter().all(|part| {
            part.chars()
                .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-')
        })
}

fn normalize_origin(raw: &str) -> Option<String> {
    request_origin(raw.trim().trim_end_matches('/'))
}

fn request_origin(url: &str) -> Option<String> {
    let (scheme, rest) = url.split_once("://")?;
    if !scheme.eq_ignore_ascii_case("http") && !scheme.eq_ignore_ascii_case("https") {
        return None;
    }
    let host = rest.split(['/', '?', '#']).next()?;
    if host.is_empty() {
        return None;
    }
    Some(format!("{}://{}", scheme.to_ascii_lowercase(), host.to_ascii_lowercase()))
}

fn media_type(content_type: &str) -> String {
    content_type
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase()
}

fn header_value<'a>(headers: &'a [HarHeader], name: &str) -> Option<&'a str> {
    headers.iter().find_map(|header| {
        header
            .name
            .eq_ignore_ascii_case(name)
            .then_some(header.value.as_str())
            .filter(|value| !value.is_empty())
    })
}

fn push_limit(limitations: &mut Vec<String>, note: String) {
    if limitations.len() >= 32 || limitations.contains(&note) {
        return;
    }
    limitations.push(note);
}
