//! Task 4: every Weavatrix result carries revision identity. No second graph.

use std::path::{Path, PathBuf};

use serde_json::json;
use wvq_intelligence::{CodeEvidenceProvider, IntelligenceError, WeavatrixProvider};

fn tiny_js() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("fixtures")
        .join("repos")
        .join("tiny-js")
}

#[test]
fn analyze_quotes_revision_without_copying_the_graph() {
    let evidence = WeavatrixProvider.analyze(&tiny_js()).unwrap();
    assert!(!evidence.revision.as_str().is_empty());
    assert!(!evidence.repository.is_empty());
    assert!(
        evidence.node_count > 0,
        "Weavatrix must see the tiny-js fixture"
    );
    assert!(
        evidence.generator.contains("weavatrix"),
        "{}",
        evidence.generator
    );
}

#[test]
fn operation_results_share_the_analyze_revision() {
    let provider = WeavatrixProvider;
    let evidence = provider.analyze(&tiny_js()).unwrap();
    let stats = provider
        .operation(&tiny_js(), "graph_stats", &json!({}))
        .unwrap();
    assert_eq!(
        stats["revision"].as_str(),
        Some(evidence.revision.as_str())
    );
    assert!(stats.get("repository").and_then(serde_json::Value::as_str).is_some());
    assert_eq!(
        stats["nodes"].as_u64().unwrap_or(0),
        evidence.node_count
    );
}

#[test]
fn indexed_files_are_quoted_from_the_authoritative_snapshot() {
    let files = WeavatrixProvider.indexed_files(&tiny_js()).unwrap();
    assert!(files.contains("src/add.js"), "{files:?}");
    assert!(files.iter().all(|path| !path.starts_with(".git/")));
}

#[test]
fn unknown_operation_fails_closed() {
    let err = WeavatrixProvider
        .operation(&tiny_js(), "invent_a_graph", &json!({}))
        .unwrap_err();
    match err {
        IntelligenceError::Engine(message) => {
            assert!(message.contains("unknown tool"), "{message}");
        }
        other => panic!("unexpected {other}"),
    }
}

#[test]
fn missing_repository_fails_closed() {
    let missing = tiny_js().join("does-not-exist");
    let err = WeavatrixProvider.analyze(&missing).unwrap_err();
    assert!(matches!(err, IntelligenceError::Engine(_)));
}
