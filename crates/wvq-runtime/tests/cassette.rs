//! Extended network cassette: HAR in, privacy-safe `NetworkReplayProfile` out.

use wvq_runtime::{CassetteError, ingest_har};

fn pay_har(response_body: &str, extra_entries: &str) -> String {
    format!(
        r#"{{
            "log": {{
                "version": "1.2",
                "entries": [
                    {{
                        "request": {{
                            "method": "POST",
                            "url": "https://app.example/api/pay?token=secret-token-value",
                            "headers": [
                                {{ "name": "Content-Type", "value": "application/json" }},
                                {{ "name": "Authorization", "value": "Bearer abc.def.ghi" }},
                                {{ "name": "Cookie", "value": "sid=raw-session" }}
                            ],
                            "cookies": [{{ "name": "sid", "value": "raw-session" }}],
                            "postData": {{
                                "mimeType": "application/json",
                                "text": "{{ \"order\": \"42\", \"email\": \"ada@example.com\" }}"
                            }}
                        }},
                        "response": {{
                            "status": 200,
                            "headers": [
                                {{ "name": "Set-Cookie", "value": "sid=other" }},
                                {{ "name": "Content-Type", "value": "application/json" }}
                            ],
                            "content": {{
                                "mimeType": "application/json",
                                "text": {response_body}
                            }}
                        }}
                    }}{extra_entries}
                ]
            }}
        }}"#
    )
}

#[test]
fn a_json_har_becomes_a_schema_v2_profile_without_secrets() {
    let admitted = ingest_har(
        &pay_har(r#""{ \"ok\": true, \"email\": \"ada@example.com\", \"token\": \"abc\" }""#, ""),
        "https://app.example",
    )
    .unwrap();
    assert_eq!(admitted.profile.schema_v, 2);
    assert_eq!(admitted.profile.entries.len(), 1);
    let entry = &admitted.profile.entries[0];
    assert_eq!(entry.method, "POST");
    assert_eq!(entry.path, "/api/pay?token=[REDACTED]");
    assert_eq!(entry.status, 200);
    assert_eq!(entry.content_type, "application/json");
    assert!(entry.body.contains("\"ok\":true") || entry.body.contains("\"ok\": true"));
    assert!(entry.body.contains("[REDACTED]"));
    assert!(!entry.body.contains("ada@example.com"));
    assert!(!entry.body.contains("raw-session"));
    let dumped = serde_json::to_string(&admitted.profile).unwrap();
    assert!(!dumped.to_lowercase().contains("bearer"));
    assert!(!dumped.contains("sid="));
    assert!(!dumped.contains("ada@example.com"));
    assert!(entry.request_body_digest.is_some());
    assert!(admitted
        .limitations
        .iter()
        .any(|item| item.contains("cookies") || item.contains("authorization")));
}

#[test]
fn cross_origin_and_non_json_are_omitted_not_replayed() {
    let extra = r#",
        {
            "request": {
                "method": "GET",
                "url": "https://other.example/api/pay",
                "headers": [{ "name": "Content-Type", "value": "application/json" }]
            },
            "response": {
                "status": 200,
                "content": { "mimeType": "application/json", "text": "{ \"ok\": true }" }
            }
        },
        {
            "request": {
                "method": "GET",
                "url": "https://app.example/logo.png",
                "headers": []
            },
            "response": {
                "status": 200,
                "content": { "mimeType": "image/png", "text": "iVBORw0KGgo=" }
            }
        }"#;
    let admitted = ingest_har(&pay_har(r#""{ \"ok\": true }""#, extra), "https://app.example").unwrap();
    assert_eq!(admitted.profile.entries.len(), 1);
    assert!(admitted.omitted >= 2);
    assert!(admitted
        .limitations
        .iter()
        .any(|item| item.contains("cross-origin")));
    assert!(admitted
        .limitations
        .iter()
        .any(|item| item.contains("non-JSON")));
}

#[test]
fn unknown_har_version_fails_closed() {
    let err = ingest_har(
        r#"{"log":{"version":"1.3","entries":[]}}"#,
        "https://app.example",
    )
    .unwrap_err();
    assert!(matches!(err, CassetteError::Invalid(_)), "{err}");
    assert!(err.to_string().contains("1.3"), "{err}");
}

#[test]
fn graphql_query_text_never_enters_the_cassette() {
    let har = r#"{
        "log": {
            "version": "1.2",
            "entries": [{
                "request": {
                    "method": "POST",
                    "url": "https://app.example/graphql",
                    "headers": [{ "name": "Content-Type", "value": "application/json" }],
                    "postData": {
                        "mimeType": "application/json",
                        "text": "{\"operationName\":\"Checkout\",\"query\":\"query Checkout { order { id } }\",\"variables\":{\"id\":\"1\"}}"
                    }
                },
                "response": {
                    "status": 200,
                    "content": { "mimeType": "application/json", "text": "{\"data\":{\"order\":{\"id\":\"1\"}}}" }
                }
            }]
        }
    }"#;
    let admitted = ingest_har(har, "https://app.example").unwrap();
    let entry = &admitted.profile.entries[0];
    assert_eq!(entry.graphql_operation_name.as_deref(), Some("Checkout"));
    assert!(entry.graphql_query_digest.is_some());
    assert!(entry.request_body_digest.is_none());
    let dumped = serde_json::to_string(&admitted.profile).unwrap();
    assert!(!dumped.contains("order { id }"));
}
