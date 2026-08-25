use super::*;

    #[test]
    fn a_passing_case_with_no_remaining_impacted_flow_is_phantom_not_removed() {
        let exact = "service/permission_test.go#TestViewerCannotDelete";
        let base = snapshot_with_executed_tests(
            &RevisionId::new("base-protector-inventory").unwrap(),
            vec![FlowProtection {
                flow: "symbol:service/permission.go#CanDelete".into(),
                revision: "base-protector-inventory".into(),
                tests: vec![exact.into()],
                sessions: Vec::new(),
                covered_nodes: vec!["symbol:service/permission.go#CanDelete".into()],
                covered_branches: Vec::new(),
                proven_obligations: vec!["viewer-deny".into()],
                proofs: Vec::new(),
            }],
            vec![exact.into()],
        )
        .unwrap();
        let head = snapshot_with_executed_tests(
            &RevisionId::new("head-protector-inventory").unwrap(),
            vec![FlowProtection {
                flow: "symbol:service/permission.go#CanDelete".into(),
                revision: "head-protector-inventory".into(),
                tests: Vec::new(),
                sessions: Vec::new(),
                covered_nodes: Vec::new(),
                covered_branches: Vec::new(),
                proven_obligations: Vec::new(),
                proofs: Vec::new(),
            }],
            vec![exact.into()],
        )
        .unwrap();

        let lineage = protection_lineage(&base, &head);

        assert_eq!(lineage.len(), 1);
        assert_eq!(lineage[0].test, exact);
        assert_eq!(lineage[0].state, "unchanged");
        assert!(lineage[0].phantom);
        assert_eq!(
            lineage[0].lost_flows,
            ["symbol:service/permission.go#CanDelete"]
        );
    }

    #[test]
    fn batch_coverage_never_claims_which_normalized_case_protected_a_flow() {
        let mut record = record("go-test");
        record.cwd = "service".into();
        record.artifacts.push(ProducedArtifact {
            kind: "normalized-test-run".into(),
            path: "go-test#normalized".into(),
            bytes: serde_json::to_vec(&NormalizedTestRun {
                cases: ["TestViewerCannotDelete", "TestAdminCanDelete"]
                    .into_iter()
                    .map(|name| wvq_runtime::TestCaseResult {
                        name: name.into(),
                        suite: "fixture.local/product/service".into(),
                        status: TestStatus::Pass,
                        duration_ms: Some(1),
                        message: None,
                    })
                    .collect(),
                coverage: None,
                raw_artifacts: Vec::new(),
            })
            .unwrap(),
        });

        let inventory =
            executed_test_inventory(Path::new("."), std::slice::from_ref(&record), &[]).unwrap();
        assert_eq!(
            inventory,
            [
                "go-test:fixture.local/product/service#TestAdminCanDelete",
                "go-test:fixture.local/product/service#TestViewerCannotDelete"
            ],
            "exact case execution is retained even though batch coverage stays executor-level"
        );

        let protectors = coverage_protectors(Path::new("."), &record, &[]).unwrap();
        assert_eq!(
            protectors,
            [CoverageProtector {
                identity: "executor:go-test@service".into(),
                obligations: Vec::new(),
            }]
        );
    }

    #[test]
    fn dynamic_selection_learns_only_repeated_single_test_coverage() {
        let root = TempDir::new("dynamic-selection-history");
        let store = Store::open(&root.0).unwrap();
        let revision = RevisionId::new("revision-dynamic-selection").unwrap();
        let graph = json!({
            "nodes": [{
                "id": "symbol:add",
                "span": {"file": "src/lib.rs", "start_line": 1, "end_line": 2}
            }]
        });
        let coverage = CoverageArtifact {
            files: vec![wvq_runtime::FileCoverage {
                path: "src/lib.rs".into(),
                covered: vec![wvq_runtime::LineRange { start: 1, end: 2 }],
                uncovered: Vec::new(),
            }],
        };

        for index in 1..=2 {
            let run_id = RunId::new(format!("run-dynamic-selection-{index}")).unwrap();
            store
                .put_run(&StoredRun {
                    id: run_id.clone(),
                    change_id: "dynamic-selection".into(),
                    revision: revision.clone(),
                    status: "complete".into(),
                    passed: true,
                    outcome: "passed".into(),
                })
                .unwrap();
            let mut exact = record("vitest");
            exact.selection = vec!["tests/add.test.ts".into()];
            exact.artifacts.push(ProducedArtifact {
                kind: "coverage".into(),
                path: "coverage#normalized".into(),
                bytes: serde_json::to_vec(&coverage).unwrap(),
            });
            persist_dynamic_coverage_history(&store, &run_id, &revision, &graph, &[exact]).unwrap();
        }

        let learned = store
            .historical_tests_for_nodes(&["symbol:add".into()], 2, 100)
            .unwrap();
        assert_eq!(learned.len(), 1);
        assert_eq!(learned[0].test_path, "tests/add.test.ts");

        let batch_run = RunId::new("run-dynamic-selection-batch").unwrap();
        store
            .put_run(&StoredRun {
                id: batch_run.clone(),
                change_id: "dynamic-selection".into(),
                revision: revision.clone(),
                status: "complete".into(),
                passed: true,
                outcome: "passed".into(),
            })
            .unwrap();
        let mut batch = record("vitest");
        batch.selection = vec!["tests/a.test.ts".into(), "tests/b.test.ts".into()];
        batch.artifacts.push(ProducedArtifact {
            kind: "coverage".into(),
            path: "coverage#normalized".into(),
            bytes: serde_json::to_vec(&coverage).unwrap(),
        });
        persist_dynamic_coverage_history(&store, &batch_run, &revision, &graph, &[batch]).unwrap();
        assert_eq!(
            store
                .historical_tests_for_nodes(&["symbol:add".into()], 1, 100)
                .unwrap()
                .len(),
            1,
            "aggregate coverage from a multi-test batch is not attributed"
        );
    }

    #[test]
    fn repeated_dynamic_history_is_unioned_with_weavatrix_selection() {
        let root = TempDir::new("dynamic-selection-union");
        std::fs::create_dir_all(root.0.join("tests")).unwrap();
        std::fs::write(root.0.join("tests/history.test.ts"), "test").unwrap();
        let static_report = json!({"tests": []});
        let diff = json!({
            "counts": {
                "nodes_added": 0,
                "nodes_removed": 0,
                "nodes_changed": 0,
                "edges_added": 0,
                "edges_removed": 0
            },
            "nodes": {"added": [], "removed": [], "changed": []},
            "edges": {"added": [], "removed": []}
        });
        let selection = build_live_selection(
            &root.0,
            &static_report,
            &diff,
            &wvq_intelligence::ImpactedSurface::default(),
            &[],
            &[],
            &[HistoricalTestCandidate {
                test_path: "tests/history.test.ts".into(),
                matched_nodes: vec!["symbol:history".into()],
                minimum_observations: 2,
                defensive_misses: 0,
                last_revision: RevisionId::new("revision-history").unwrap(),
            }],
        )
        .unwrap();
        assert_eq!(selection.selected, ["tests/history.test.ts"]);
        assert!(selection.explanations[0][0].contains("repeated measured coverage"));
    }

    #[test]
    fn defensive_full_run_miss_is_persisted_and_teaches_future_selection() {
        let root = TempDir::new("defensive-selection-audit");
        std::fs::create_dir_all(root.0.join("tests")).unwrap();
        std::fs::write(root.0.join("tests/missed.test.ts"), "test").unwrap();
        let store = Store::open(&root.0).unwrap();
        let revision = RevisionId::new("revision-selection-audit").unwrap();
        let impacted = RunId::new("run-selection-audit-impacted").unwrap();
        let full = RunId::new("run-selection-audit-full").unwrap();
        for (run, passed, outcome) in [(&impacted, true, "passed"), (&full, false, "failed")] {
            store
                .put_run(&StoredRun {
                    id: run.clone(),
                    change_id: "selection-audit".into(),
                    revision: revision.clone(),
                    status: "complete".into(),
                    passed,
                    outcome: outcome.into(),
                })
                .unwrap();
        }
        let mut handles = Vec::new();
        put_json_run_artifact(
            &store,
            &impacted,
            "artifact-selection-audit-impacted-summary",
            "execution-summary",
            &json!({"requested_scope": "impacted", "effective_scope": "impacted"}),
            &mut handles,
        )
        .unwrap();
        put_json_run_artifact(
            &store,
            &impacted,
            "artifact-selection-audit-impact",
            "impacted-surface",
            &json!({
                "base_only": [],
                "head_only": ["symbol:cart"],
                "shared": [],
                "removed_nodes": [],
                "removed_edges": [],
                "removed_surfaces": []
            }),
            &mut handles,
        )
        .unwrap();
        put_json_run_artifact(
            &store,
            &full,
            "artifact-selection-audit-full-summary",
            "execution-summary",
            &json!({"requested_scope": "all", "effective_scope": "all"}),
            &mut handles,
        )
        .unwrap();
        store
            .put_test_case_result(&StoredTestCaseResult {
                id: "selection-audit-missed-case".into(),
                run_id: full.clone(),
                revision: revision.clone(),
                executor: "vitest".into(),
                suite: "tests/missed.test.ts".into(),
                name: "detects the regression".into(),
                status: "fail".into(),
                duration_ms: Some(10),
                fingerprint: None,
            })
            .unwrap();

        let audit =
            audit_live_selection(&root.0, &store, impacted.as_str(), full.as_str()).unwrap();
        assert_eq!(audit.status, "contradicted");
        assert_eq!(audit.missed_failure_count, 1);
        assert_eq!(audit.learned_test_count, 1);
        assert!(audit.evidence_handle.is_some());
        assert_eq!(
            audit_live_selection(&root.0, &store, impacted.as_str(), full.as_str(),).unwrap(),
            audit,
            "replaying the same audit is idempotent"
        );
        let learned = store
            .historical_tests_for_nodes(&["symbol:cart".into()], 2, 100)
            .unwrap();
        assert_eq!(learned.len(), 1);
        assert_eq!(learned[0].test_path, "tests/missed.test.ts");
        assert_eq!(learned[0].defensive_misses, 1);
    }


