//! Privacy-safe request identity never stores a raw body.

use serde_json::json;
use wvq_runtime::{canonical_json_digest, identify_request};

#[test]
fn json_key_order_does_not_change_the_digest() {
    let left = identify_request(
        "post",
        "https://app.example/api/save",
        "application/json; charset=utf-8",
        Some(br#"{"b":1,"a":{"z":2,"y":3}}"#),
    );
    let right = identify_request(
        "POST",
        "https://other.example/api/save",
        "Application/JSON",
        Some(br#"{"a":{"y":3,"z":2},"b":1}"#),
    );
    assert_eq!(left.body_digest, right.body_digest);
    assert_eq!(left.key(), right.key());
    assert!(left.key().contains("body:"));
    assert!(!left.key().contains("\"a\""));
}

#[test]
fn a_theme_payload_does_not_match_a_checkout_payload() {
    let checkout = identify_request(
        "POST",
        "https://app.example/api/save",
        "application/json",
        Some(br#"{"order":"42"}"#),
    );
    let theme = identify_request(
        "POST",
        "https://app.example/api/save",
        "application/json",
        Some(br#"{"theme":"dark"}"#),
    );
    assert_ne!(checkout.key(), theme.key());
    assert_eq!(checkout.path, "/api/save");
    assert_eq!(theme.path, "/api/save");
}

#[test]
fn graphql_operations_on_the_same_path_are_distinct() {
    let checkout = identify_request(
        "POST",
        "https://app.example/graphql",
        "application/json",
        Some(
            br#"{"operationName":"Checkout","query":"query Checkout { order { id } }","variables":{"id":"1"}}"#,
        ),
    );
    let theme = identify_request(
        "POST",
        "https://app.example/graphql",
        "application/json",
        Some(
            br#"{"query":"query Theme { palette }","variables":{}}"#,
        ),
    );
    let checkout_gql = checkout.graphql.as_ref().expect("checkout is graphql");
    let theme_gql = theme.graphql.as_ref().expect("theme is graphql");
    assert_eq!(checkout_gql.operation_name.as_deref(), Some("Checkout"));
    assert_eq!(theme_gql.operation_name.as_deref(), Some("Theme"));
    assert_ne!(checkout_gql.query_digest, theme_gql.query_digest);
    assert_ne!(checkout.key(), theme.key());
    assert!(checkout.body_digest.is_none());
    assert!(!checkout.key().contains("order { id }"));
}

#[test]
fn graphql_variable_key_order_is_canonical() {
    let left = identify_request(
        "POST",
        "/graphql",
        "application/json",
        Some(br#"{"query":"query Q($a:ID,$b:ID){n}","variables":{"b":2,"a":1}}"#),
    );
    let right = identify_request(
        "POST",
        "/graphql",
        "application/json",
        Some(br#"{"query":"query Q($a:ID,$b:ID){n}","variables":{"a":1,"b":2}}"#),
    );
    assert_eq!(
        left.graphql.as_ref().unwrap().variables_digest,
        right.graphql.as_ref().unwrap().variables_digest
    );
}

#[test]
fn graphql_query_whitespace_does_not_change_the_digest() {
    let compact = identify_request(
        "POST",
        "/graphql",
        "application/json",
        Some(br#"{"query":"query Checkout { order { id } }"}"#),
    );
    let spaced = identify_request(
        "POST",
        "/graphql",
        "application/json",
        Some(br#"{"query":"query   Checkout\n{\n  order { id }\n}"}"#),
    );
    assert_eq!(
        compact.graphql.as_ref().unwrap().query_digest,
        spaced.graphql.as_ref().unwrap().query_digest
    );
}

#[test]
fn empty_bodies_are_identified_without_a_digest() {
    let identity = identify_request("GET", "https://app.example/api/orders", "", None);
    assert!(identity.body_digest.is_none());
    assert!(identity.graphql.is_none());
    assert_eq!(identity.key(), "GET /api/orders");
}

#[test]
fn screenshot_png_digest_names_its_surface() {
    let left = wvq_runtime::bytes_digest(b"\x89PNG-fake-a");
    let right = wvq_runtime::bytes_digest(b"\x89PNG-fake-b");
    assert_eq!(left.len(), 64);
    assert_ne!(left, right);
    assert!(!left.contains("PNG"));
}

#[test]
fn canonical_digest_never_embeds_the_source_json() {
    let value = json!({"token":"secret-value","email":"qa@example.com"});
    let digest = canonical_json_digest(&value);
    assert_eq!(digest.len(), 64);
    assert!(!digest.contains("secret"));
    assert!(!digest.contains("qa@"));
}
