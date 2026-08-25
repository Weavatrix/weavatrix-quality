use super::*;
use super::super::policy::yaml::{utc_date, valid_iso_date};

    #[test]
    fn run_policy_caps_program_owned_browser_capture() {
        let raw = r#"{
            "schema_v": 1,
            "id": "capture-policy",
            "source": "authored",
            "obligations": ["visible"],
            "steps": [{"action":"assert","obligation":"visible"}],
            "evidence_policy": {
                "screenshot":"always",
                "trace":"always",
                "network":"always",
                "console":"always",
                "storage":"always"
            }
        }"#;
        let mut minimal = TestProgram::from_json(raw).unwrap();
        cap_browser_evidence(&mut minimal, "minimal");
        assert_eq!(minimal.evidence_policy.screenshot, CaptureWhen::OnFailure);
        assert_eq!(minimal.evidence_policy.trace, CaptureWhen::OnFailure);
        assert!(!browser_capture_active(
            CaptureWhen::Always,
            false,
            "minimal"
        ));
        assert!(browser_capture_active(CaptureWhen::Always, true, "minimal"));

        let mut none = TestProgram::from_json(raw).unwrap();
        cap_browser_evidence(&mut none, "none");
        assert_eq!(none.evidence_policy.screenshot, CaptureWhen::Never);
        assert_eq!(none.evidence_policy.network, CaptureWhen::Never);
        assert!(!browser_capture_active(CaptureWhen::Always, true, "none"));
    }
    #[test]
    fn debt_policy_loads_active_exceptions_and_rejects_expired_ones() {
        let root = TempDir::new("debt-policy");
        std::fs::create_dir_all(root.0.join(".weavatrix-quality")).unwrap();
        std::fs::write(
            root.0.join(".weavatrix-quality/config.yaml"),
            "quality_policy_v: 1\nratchet:\n  mode: no_new_debt\n  exceptions:\n    - fingerprint: active-id\n      reason: tracked cleanup\n      expires: 2999-12-31\n    - fingerprint: expired-id\n      reason: old waiver\n      expires: 2000-01-01\n",
        )
        .unwrap();

        let exceptions = load_debt_exceptions(&root.0).unwrap();

        assert_eq!(exceptions.active, BTreeSet::from(["active-id".into()]));
        assert_eq!(exceptions.notes.len(), 1);
        assert!(exceptions.notes[0].contains("expired-id"));
    }

    #[test]
    fn current_utc_date_uses_iso_ordering() {
        let today = utc_date();
        assert!(valid_iso_date(&today));
    }

    #[test]
    fn responsive_probe_retries_only_incomplete_evidence() {
        let complete = ResponsiveProbe {
            width: 767,
            delta: UiIntegrityDelta::default(),
        };
        assert!(!responsive_probe_incomplete(&complete));

        let mut truncated = complete.clone();
        truncated.delta.truncated = true;
        assert!(responsive_probe_incomplete(&truncated));

        let mut unmeasured = complete;
        unmeasured
            .delta
            .unmeasured_states
            .push("checkout#0@/@767x720".into());
        assert!(responsive_probe_incomplete(&unmeasured));
    }

