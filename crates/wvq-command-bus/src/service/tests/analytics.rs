use super::*;

    #[test]
    fn live_test_analytics_persist_failures_durations_and_flaky_history() {
        let root = TempDir::new("live-test-analytics");
        let store = Store::open(&root.0).unwrap();
        let revision = RevisionId::new("revision-test-analytics").unwrap();

        let failed_run = RunId::new("run-test-analytics-failed").unwrap();
        store
            .put_run(&StoredRun {
                id: failed_run.clone(),
                change_id: "test-analytics".into(),
                revision: revision.clone(),
                status: "complete".into(),
                passed: false,
                outcome: "failed".into(),
            })
            .unwrap();
        let mut failed_record = record("vitest");
        failed_record.passed = false;
        failed_record.artifacts.push(ProducedArtifact {
            kind: "normalized-test-run".into(),
            path: "junit.xml#normalized".into(),
            bytes: serde_json::to_vec(&NormalizedTestRun {
                cases: vec![wvq_runtime::TestCaseResult {
                    name: "loads the cart".into(),
                    suite: "src/cart.test.ts".into(),
                    status: TestStatus::Fail,
                    duration_ms: Some(8_000),
                    message: Some("timed out waiting for response".into()),
                }],
                coverage: None,
                raw_artifacts: Vec::new(),
            })
            .unwrap(),
        });
        let failed =
            persist_test_analytics(&store, &failed_run, &revision, &[failed_record], &[]).unwrap();
        assert_eq!(failed.recorded_test_count, 1);
        assert_eq!(failed.failed_test_count, 1);
        assert_eq!(failed.flaky_test_count, 0);
        assert_eq!(failed.unknown_failure_count, 0);

        let passed_run = RunId::new("run-test-analytics-passed").unwrap();
        store
            .put_run(&StoredRun {
                id: passed_run.clone(),
                change_id: "test-analytics".into(),
                revision: revision.clone(),
                status: "complete".into(),
                passed: true,
                outcome: "passed".into(),
            })
            .unwrap();
        let mut passed_record = record("vitest");
        passed_record.artifacts.push(ProducedArtifact {
            kind: "normalized-test-run".into(),
            path: "junit.xml#normalized".into(),
            bytes: serde_json::to_vec(&NormalizedTestRun {
                cases: vec![wvq_runtime::TestCaseResult {
                    name: "loads the cart".into(),
                    suite: "src/cart.test.ts".into(),
                    status: TestStatus::Pass,
                    duration_ms: Some(2_000),
                    message: None,
                }],
                coverage: None,
                raw_artifacts: Vec::new(),
            })
            .unwrap(),
        });
        let passed =
            persist_test_analytics(&store, &passed_run, &revision, &[passed_record], &[]).unwrap();
        assert_eq!(passed.recorded_test_count, 1);
        assert_eq!(passed.failed_test_count, 0);
        assert_eq!(passed.flaky_test_count, 1);
        assert_eq!(passed.unknown_failure_count, 0);

        let value: Value = serde_json::from_slice(&passed.bytes).unwrap();
        assert_eq!(value["slowest_tests"][0]["historical_average_ms"], 5_000);
    }

    #[test]
    fn fresh_junit_and_lcov_are_preserved_and_normalized() {
        let root = TempDir::new("runner-artifacts");
        std::fs::create_dir_all(root.0.join("coverage")).unwrap();
        let started = SystemTime::now();
        std::fs::write(
            root.0.join("junit.xml"),
            "<testsuite name=\"suite\"><testcase name=\"works\"/></testsuite>",
        )
        .unwrap();
        std::fs::write(
            root.0.join("coverage/lcov.info"),
            "SF:src/lib.rs\nDA:1,1\nDA:2,0\nend_of_record\n",
        )
        .unwrap();
        let mut record = record("npm-test");

        attach_normalized_artifacts(&root.0, &root.0, started, &mut record);

        assert!(record.passed);
        assert!(record.error.is_none());
        assert_eq!(
            record
                .artifacts
                .iter()
                .map(|artifact| artifact.kind.as_str())
                .collect::<Vec<_>>(),
            ["junit", "normalized-test-run", "lcov", "coverage"]
        );
    }

