//! HAR cassette ingest never enables replay and never seals.

use super::*;
use crate::IngestCassetteCommand;

fn json_har() -> String {
    r#"{
        "log": {
            "version": "1.2",
            "entries": [{
                "request": {
                    "method": "GET",
                    "url": "https://app.example/api/pay",
                    "headers": [{ "name": "Content-Type", "value": "application/json" }]
                },
                "response": {
                    "status": 200,
                    "content": { "mimeType": "application/json", "text": "{ \"ok\": true }" }
                }
            }]
        }
    }"#
    .into()
}

#[test]
fn ingest_cassette_never_enables_replay_or_a_seal() {
    let reply = FakeService::default()
        .ingest_cassette(&IngestCassetteCommand {
            origin: "https://app.example".into(),
            har: json_har(),
        })
        .unwrap();
    assert!(reply.useful);
    assert!(!reply.replay_enabled);
    assert!(!reply.seal_eligible);
    assert_eq!(reply.runtime_llm_tokens, 0);
    assert!(reply.profile_handle.is_some());
}

#[test]
fn ingest_cassette_refuses_unknown_har_versions() {
    let err = FakeService::default()
        .ingest_cassette(&IngestCassetteCommand {
            origin: "https://app.example".into(),
            har: r#"{"log":{"version":"9.9","entries":[]}}"#.into(),
        })
        .unwrap_err();
    assert!(err.to_string().contains("9.9"), "{err}");
}
