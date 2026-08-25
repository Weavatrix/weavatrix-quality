//! Privacy-safe request identity. Raw bodies never become evidence.

use std::fmt::Write as _;

use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

/// GraphQL-shaped identity. The query and variables themselves are not stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphqlIdentity {
    /// `operationName`, or the first named operation in the query.
    pub operation_name: Option<String>,
    /// SHA-256 of the whitespace-normalised query text.
    pub query_digest: String,
    /// SHA-256 of canonical JSON variables (`{}` when omitted).
    pub variables_digest: String,
}

/// Method + path + content type + body digest, with GraphQL as its own triple.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestIdentity {
    /// Uppercase HTTP method.
    pub method: String,
    /// Authority-free path and query.
    pub path: String,
    /// Lowercase media type without parameters. Empty when the request had none.
    pub content_type: String,
    /// Canonical JSON digest, or raw SHA-256 when the body is not JSON.
    pub body_digest: Option<String>,
    /// Present only for GraphQL requests. Replaces `body_digest` as the body key.
    pub graphql: Option<GraphqlIdentity>,
}

impl RequestIdentity {
    /// Stable comparison token. Never contains a request body.
    #[must_use]
    pub fn key(&self) -> String {
        let mut key = format!("{} {}", self.method, self.path);
        if !self.content_type.is_empty() {
            key.push(' ');
            key.push_str(&self.content_type);
        }
        if let Some(graphql) = &self.graphql {
            key.push_str(" gql:");
            key.push_str(graphql.operation_name.as_deref().unwrap_or("-"));
            key.push_str(" q:");
            key.push_str(&graphql.query_digest);
            key.push_str(" v:");
            key.push_str(&graphql.variables_digest);
        } else if let Some(digest) = &self.body_digest {
            key.push_str(" body:");
            key.push_str(digest);
        }
        key
    }
}

/// Identify one request from method, URL, content type, and optional body bytes.
///
/// The body is hashed and discarded. JSON objects are canonicalised by sorting
/// keys so semantically equal payloads compare equal.
#[must_use]
pub fn identify_request(
    method: &str,
    url: &str,
    content_type: &str,
    body: Option<&[u8]>,
) -> RequestIdentity {
    let method = method.trim().to_ascii_uppercase();
    let path = request_path(url);
    let content_type = media_type(content_type);
    let parsed_json = body.and_then(|bytes| serde_json::from_slice::<Value>(bytes).ok());
    if looks_like_graphql(&path, &content_type, parsed_json.as_ref(), body)
        && let Some(graphql) = graphql_identity(&content_type, body, parsed_json.as_ref())
    {
        return RequestIdentity {
            method,
            path,
            content_type,
            body_digest: None,
            graphql: Some(graphql),
        };
    }
    let body_digest = body
        .filter(|bytes| !bytes.is_empty())
        .map(|bytes| match &parsed_json {
            Some(value) => canonical_json_digest(value),
            None => sha256_hex(bytes),
        });
    RequestIdentity {
        method,
        path,
        content_type,
        body_digest,
        graphql: None,
    }
}

/// Authority-free path and query. Origin is infrastructure, not identity.
#[must_use]
pub fn request_path(url: &str) -> String {
    let Some((_, after_scheme)) = url.split_once("://") else {
        return url.to_owned();
    };
    after_scheme
        .find('/')
        .map_or_else(|| "/".into(), |index| after_scheme[index..].to_owned())
}

/// SHA-256 of canonical JSON. Object keys are sorted recursively.
#[must_use]
pub fn canonical_json_digest(value: &Value) -> String {
    sha256_hex(canonical_json(value).to_string().as_bytes())
}

fn media_type(content_type: &str) -> String {
    content_type
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase()
}

fn looks_like_graphql(
    path: &str,
    content_type: &str,
    json: Option<&Value>,
    body: Option<&[u8]>,
) -> bool {
    if content_type == "application/graphql" || content_type.starts_with("application/graphql+") {
        return true;
    }
    let path = path.to_ascii_lowercase();
    let path_looks_graphql = path.contains("/graphql") || path.ends_with("graphql");
    json.is_some_and(|value| value.get("query").and_then(Value::as_str).is_some())
        || (path_looks_graphql && body.is_some_and(|bytes| !bytes.is_empty()))
}

fn graphql_identity(
    content_type: &str,
    body: Option<&[u8]>,
    json: Option<&Value>,
) -> Option<GraphqlIdentity> {
    let (query, operation_name, variables) = if content_type == "application/graphql"
        || content_type.starts_with("application/graphql+")
    {
        let query = std::str::from_utf8(body?).ok()?.trim();
        if query.is_empty() {
            return None;
        }
        (query.to_owned(), None, Value::Object(Map::new()))
    } else {
        let json = json?;
        let query = json.get("query").and_then(Value::as_str)?.to_owned();
        if query.trim().is_empty() {
            return None;
        }
        let operation_name = json
            .get("operationName")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(ToOwned::to_owned);
        let variables = json
            .get("variables")
            .cloned()
            .unwrap_or_else(|| Value::Object(Map::new()));
        (query, operation_name, variables)
    };
    let operation_name = operation_name.or_else(|| named_graphql_operation(&query));
    Some(GraphqlIdentity {
        operation_name,
        query_digest: sha256_hex(normalise_graphql_query(&query).as_bytes()),
        variables_digest: canonical_json_digest(&variables),
    })
}

fn named_graphql_operation(query: &str) -> Option<String> {
    let tokens = query.split_whitespace().collect::<Vec<_>>();
    tokens.windows(2).find_map(|pair| {
        matches!(pair[0], "query" | "mutation" | "subscription")
            .then(|| pair[1])
            .filter(|name| {
                name.starts_with(|ch: char| ch.is_ascii_alphabetic() || ch == '_')
                    && name
                        .chars()
                        .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
            })
            .map(ToOwned::to_owned)
    })
}

fn normalise_graphql_query(query: &str) -> String {
    query.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn canonical_json(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let sorted = map
                .iter()
                .map(|(key, child)| (key.clone(), canonical_json(child)))
                .collect::<std::collections::BTreeMap<_, _>>();
            Value::Object(sorted.into_iter().collect())
        }
        Value::Array(items) => Value::Array(items.iter().map(canonical_json).collect()),
        other => other.clone(),
    }
}

/// SHA-256 hex of raw bytes. Used for request bodies and visual surfaces.
#[must_use]
pub fn bytes_digest(bytes: &[u8]) -> String {
    sha256_hex(bytes)
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .fold(String::with_capacity(64), |mut out, byte| {
            let _ = write!(out, "{byte:02x}");
            out
        })
}
