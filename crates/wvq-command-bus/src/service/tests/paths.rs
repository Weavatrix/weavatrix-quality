use super::*;

    #[test]
    fn live_service_canonicalizes_existing_repository_paths() {
        let root = TempDir::new("canonical-repo");
        let dotted = root.0.join(".");
        let service = LiveService::new(&dotted);
        assert_eq!(service.repo, canonical_repo_path(&dotted));
        assert!(service.repo.is_absolute());
        #[cfg(windows)]
        assert!(!service.repo.to_string_lossy().starts_with(r"\\?\"));
    }

    #[test]
    fn graph_symbol_ids_resolve_to_repository_test_paths() {
        assert_eq!(
            test_path_from_node_id("symbol:src/widget/Widget.test.tsx#renders"),
            Some("src/widget/Widget.test.tsx".into())
        );
        assert_eq!(
            test_path_from_node_id("file:src/widget/Widget.test.tsx"),
            Some("src/widget/Widget.test.tsx".into())
        );
        assert_eq!(
            test_path_from_node_id("file:src/widget/Widget.stories.tsx"),
            Some("src/widget/Widget.stories.tsx".into())
        );
    }

    #[test]
    fn an_impacted_story_routes_only_to_the_storybook_vitest_project() {
        let root = TempDir::new("storybook-impact-routing");
        std::fs::create_dir_all(root.0.join("src/widget")).unwrap();
        let story = "src/widget/Widget.stories.tsx";
        std::fs::write(root.0.join(story), "export const Default = {};").unwrap();
        let impact = wvq_intelligence::ImpactedSurface {
            head_only: vec![format!("file:{story}")],
            ..wvq_intelligence::ImpactedSurface::default()
        };
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
            &json!({"tests": []}),
            &diff,
            &impact,
            &[],
            &[],
            &[],
        )
        .unwrap();
        assert_eq!(selection.selected, [story]);
        assert!(selection.explanations[0][0].contains("base/head Weavatrix impact union"));

        let targets = vec![
            ExecutorTarget {
                executor: wvq_runtime::ExecutorId::new("storybook-vitest-v8").unwrap(),
                cwd: root.0.clone(),
            },
            ExecutorTarget {
                executor: wvq_runtime::ExecutorId::new("vitest").unwrap(),
                cwd: root.0.clone(),
            },
        ];
        let (requests, scope, _, executed) =
            build_execution_requests(&root.0, &targets, &selection, &BTreeSet::new(), "impacted");
        assert_eq!(scope, "impacted");
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].target.executor.as_str(), "storybook-vitest-v8");
        assert_eq!(requests[0].filters, [story]);
        assert_eq!(executed, Some(BTreeSet::from([story.into()])));
    }

    #[test]
    fn normalized_suite_identity_resolves_from_a_nested_runner_root() {
        let root = TempDir::new("nested-suite-identity");
        std::fs::create_dir_all(root.0.join("frontend/tests")).unwrap();
        std::fs::write(root.0.join("frontend/tests/cart.test.ts"), "test").unwrap();
        let record = ExecutorRecord {
            executor: "vitest".into(),
            cwd: "frontend".into(),
            selection: Vec::new(),
            status_code: Some(0),
            passed: true,
            error: None,
            stdout: Vec::new(),
            stderr: Vec::new(),
            artifacts: Vec::new(),
        };
        let binding = TestBinding {
            path: "frontend/tests/cart.test.ts".into(),
            runner: Some("vitest".into()),
            suite: None,
            case: Some("viewer cannot delete".into()),
            obligations: BTreeSet::from(["viewer-deny".into()]),
            cost: 10,
            flake_penalty: 0,
        };

        assert!(normalized_suite_matches(
            &root.0,
            &record,
            &binding,
            "tests/cart.test.ts"
        ));
        assert!(!normalized_suite_matches(
            &root.0,
            &record,
            &binding,
            "tests/other.test.ts"
        ));
    }

    #[test]
    fn large_file_selection_batches_into_one_bounded_runner_process() {
        let root = TempDir::new("filter-amplification");
        let selected = (0..17)
            .map(|index| {
                let path = format!("tests/case-{index}.test.ts");
                std::fs::create_dir_all(root.0.join("tests")).unwrap();
                std::fs::write(root.0.join(&path), "test").unwrap();
                path
            })
            .collect::<Vec<_>>();
        let selection = LiveSelection {
            selected,
            explanations: Vec::new(),
            uncovered_mandatory: Vec::new(),
            uncovered_all: Vec::new(),
            bindings: Vec::new(),
        };
        let targets = vec![ExecutorTarget {
            executor: wvq_runtime::ExecutorId::new("vitest").unwrap(),
            cwd: root.0.clone(),
        }];
        let (requests, scope, reason, executed) =
            build_execution_requests(&root.0, &targets, &selection, &BTreeSet::new(), "impacted");
        assert_eq!(scope, "impacted");
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].filters.len(), 17);
        assert_eq!(requests[0].selected_tests.len(), 17);
        assert_eq!(executed.as_ref().map(BTreeSet::len), Some(17));
        assert!(reason.contains("17 test paths"), "{reason}");
        assert!(reason.contains("1 bounded runner process"), "{reason}");
    }

    #[test]
    fn generic_npm_script_widens_instead_of_assuming_path_filter_support() {
        let root = TempDir::new("npm-filter-safety");
        std::fs::create_dir_all(root.0.join("tests")).unwrap();
        std::fs::write(root.0.join("tests/case.test.ts"), "test").unwrap();
        let selection = LiveSelection {
            selected: vec!["tests/case.test.ts".into()],
            explanations: Vec::new(),
            uncovered_mandatory: Vec::new(),
            uncovered_all: Vec::new(),
            bindings: Vec::new(),
        };
        let targets = vec![ExecutorTarget {
            executor: wvq_runtime::ExecutorId::new("npm-test").unwrap(),
            cwd: root.0.clone(),
        }];

        let (requests, scope, reason, executed) =
            build_execution_requests(&root.0, &targets, &selection, &BTreeSet::new(), "impacted");
        assert_eq!(scope, "all");
        assert_eq!(requests.len(), 1);
        assert!(requests[0].filters.is_empty());
        assert!(executed.is_none());
        assert!(
            reason.contains("no filterable registered executor"),
            "{reason}"
        );
    }

