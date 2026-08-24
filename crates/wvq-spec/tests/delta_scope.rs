//! `OpenSpec` authorization is scoped to the requirement/scenario that changed.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use wvq_spec::{diff_spec_scope, read_change};

static NEXT: AtomicU64 = AtomicU64::new(1);

struct TempRepo(PathBuf);

impl TempRepo {
    fn new(label: &str, spec: &str) -> Self {
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "wvq-spec-delta-{label}-{}-{id}",
            std::process::id()
        ));
        let dir = root.join("openspec/changes/scoped/specs/product");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("spec.md"), spec).unwrap();
        Self(root)
    }

    fn root(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

const BASE: &str = r"# Product delta

## MODIFIED Requirements

### Requirement: Export report
The system SHALL export the current report.

#### Scenario: CSV export
- GIVEN a report
- WHEN Export is selected
- THEN a CSV is downloaded

#### Scenario: PDF export
- GIVEN a report
- WHEN PDF is selected
- THEN a PDF is downloaded

### Requirement: Viewer permissions
The system SHALL deny destructive actions to viewers.

#### Scenario: Delete denied
- GIVEN a viewer
- WHEN Delete is selected
- THEN deletion is denied
";

const HEAD_EXPORT_ONLY: &str = r"# Product delta

## MODIFIED Requirements

### Requirement: Export report
The system SHALL export the current report.

#### Scenario: CSV export
- GIVEN a report
- WHEN Export is selected
- THEN a UTF-8 CSV is downloaded

#### Scenario: PDF export
- GIVEN a report
- WHEN PDF is selected
- THEN a PDF is downloaded

### Requirement: Viewer permissions
The system SHALL deny destructive actions to viewers.

#### Scenario: Delete denied
- GIVEN a viewer
- WHEN Delete is selected
- THEN deletion is denied
";

#[test]
fn changing_requirement_a_does_not_authorize_requirement_b() {
    let base = TempRepo::new("base", BASE);
    let head = TempRepo::new("head", HEAD_EXPORT_ONLY);
    let base = read_change(base.root(), "scoped").unwrap();
    let head = read_change(head.root(), "scoped").unwrap();

    let scope = diff_spec_scope(Some(&base), &head).unwrap();

    assert!(scope.authorizes("product.export-report", "csv-export"));
    assert!(!scope.authorizes("product.export-report", "pdf-export"));
    assert!(!scope.authorizes("product.viewer-permissions", "delete-denied"));
    assert_eq!(
        scope.changed_scenarios(),
        &[("product.export-report".into(), "csv-export".into())]
    );
}

#[test]
fn a_normative_requirement_text_change_authorizes_all_its_scenarios_only() {
    let base = TempRepo::new("base-body", BASE);
    let head = TempRepo::new(
        "head-body",
        &HEAD_EXPORT_ONLY.replace(
            "The system SHALL export the current report.",
            "The system SHALL export the current report in the selected format.",
        ),
    );
    let base = read_change(base.root(), "scoped").unwrap();
    let head = read_change(head.root(), "scoped").unwrap();

    let scope = diff_spec_scope(Some(&base), &head).unwrap();

    assert!(scope.authorizes("product.export-report", "csv-export"));
    assert!(scope.authorizes("product.export-report", "pdf-export"));
    assert!(!scope.authorizes("product.viewer-permissions", "delete-denied"));
    assert_eq!(
        scope.changed_requirements(),
        &["product.export-report".to_owned()]
    );
}

#[test]
fn a_new_change_authorizes_every_declared_requirement() {
    let head = TempRepo::new("new", BASE);
    let head = read_change(head.root(), "scoped").unwrap();

    let scope = diff_spec_scope(None, &head).unwrap();

    assert!(scope.authorizes("product.export-report", "csv-export"));
    assert!(scope.authorizes("product.export-report", "pdf-export"));
    assert!(scope.authorizes("product.viewer-permissions", "delete-denied"));
}
