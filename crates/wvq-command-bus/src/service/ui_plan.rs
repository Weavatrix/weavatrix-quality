//! Frozen UI execution plan helpers (selected programs + cancel).

use std::sync::atomic::{AtomicBool, Ordering};

use super::access::*;

/// Resolve the frozen UI program set for a measurement.
///
/// Head-selected programs are the plan authority. When `selected` is `Some` and
/// non-empty, only those programs run. When `None` or empty, the full catalog is
/// used (explicit all-scope views such as `ui_integrity_view`).
pub(in crate::service) fn frozen_ui_programs<'a>(
    selected: Option<&'a [ConfiguredBrowserProgram]>,
    catalog: &'a [ConfiguredBrowserProgram],
) -> Vec<&'a ConfiguredBrowserProgram> {
    match selected {
        Some(programs) if !programs.is_empty() => programs.iter().collect(),
        _ => catalog.iter().collect(),
    }
}

/// Fail closed when the shared run cancel token is set.
pub(in crate::service) fn ensure_not_cancelled(cancel: &AtomicBool) -> Result<(), BusError> {
    if cancel.load(Ordering::SeqCst) {
        Err(BusError::Runtime("run cancelled".into()))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wvq_runtime::TestProgram;

    fn program(id: &str, path: &str) -> ConfiguredBrowserProgram {
        let raw = format!(
            r#"{{
            "schema_v": 1,
            "id": "{id}",
            "source": "authored",
            "obligations": ["visible"],
            "steps": [{{"action":"assert","obligation":"visible"}}]
        }}"#
        );
        ConfiguredBrowserProgram {
            path: path.into(),
            program: TestProgram::from_json(&raw).expect("program"),
            oracles: Vec::new(),
        }
    }

    #[test]
    fn selected_subset_freezes_plan_and_ignores_catalog_extras() {
        let catalog = vec![
            program("a", "programs/a.json"),
            program("b", "programs/b.json"),
            program("c", "programs/c.json"),
        ];
        // Twenty configured / two selected: only the selected identities run.
        let selected = vec![
            program("a", "programs/a.json"),
            program("c", "programs/c.json"),
        ];
        let frozen = frozen_ui_programs(Some(&selected), &catalog);
        assert_eq!(frozen.len(), 2);
        assert_eq!(frozen[0].program.id.as_str(), "a");
        assert_eq!(frozen[1].program.id.as_str(), "c");
        assert!(!frozen.iter().any(|p| p.program.id.as_str() == "b"));
    }

    #[test]
    fn empty_or_missing_selection_uses_full_catalog() {
        let catalog = vec![program("a", "programs/a.json"), program("b", "programs/b.json")];
        assert_eq!(frozen_ui_programs(None, &catalog).len(), 2);
        let empty: Vec<ConfiguredBrowserProgram> = Vec::new();
        assert_eq!(frozen_ui_programs(Some(&empty), &catalog).len(), 2);
    }

    #[test]
    fn cancel_token_blocks_further_ui_work() {
        let cancel = AtomicBool::new(false);
        ensure_not_cancelled(&cancel).expect("open");
        cancel.store(true, Ordering::SeqCst);
        let err = ensure_not_cancelled(&cancel).expect_err("cancelled");
        assert!(err.to_string().contains("cancelled"));
    }
}
