//! Obligation → implementation code surface. Test nodes are not production evidence.

use std::collections::{BTreeMap, BTreeSet};

use wvq_domain::ObligationId;

use crate::protection::FlowProtection;

/// How a node entered an obligation's code surface. Stronger kinds win.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum CodeSurfaceEvidenceKind {
    /// Exact per-case dynamic coverage.
    ExactDynamicCoverage,
    /// Measured API/component/browser trace.
    MeasuredTrace,
    /// Static Weavatrix flow.
    StaticWeavatrixFlow,
    /// Reviewed explicit mapping.
    ReviewedExplicitMapping,
    /// Heuristic mapping. Weakest.
    HeuristicMapping,
}

/// One evidence slice on an obligation surface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeSurfaceEvidence {
    /// Strength of this slice.
    pub kind: CodeSurfaceEvidenceKind,
    /// Flow id, binding path, or review id.
    pub origin: String,
    /// Node ids attributed by this slice.
    pub nodes: Vec<String>,
}

/// Production code that one obligation may claim. Test nodes stay separate.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ObligationCodeSurface {
    /// Obligation id.
    pub obligation: String,
    /// Implementation Weavatrix nodes. Used by `CodeDelta` and mutation.
    pub implementation_nodes: Vec<String>,
    /// Test/spec nodes recorded for lineage, never as production code evidence.
    pub test_nodes: Vec<String>,
    /// Provenance slices, strongest first.
    pub evidence: Vec<CodeSurfaceEvidence>,
}

impl ObligationCodeSurface {
    /// Whether this surface names an implementation node or source path.
    #[must_use]
    pub fn contains_implementation_path(&self, path: &str) -> bool {
        let path = normalize_path(path);
        self.implementation_nodes.iter().any(|id| {
            node_source_path(id).is_some_and(|file| file == path.as_str()) || *id == path
        })
    }

    /// Whether any implementation node was attributed.
    #[must_use]
    pub fn has_implementation_mapping(&self) -> bool {
        !self.implementation_nodes.is_empty()
    }
}

/// True for test, spec, and Storybook paths. Stories are not production code.
#[must_use]
pub fn is_test_source_path(path: &str) -> bool {
    let path = normalize_path(path).to_ascii_lowercase();
    let file = path.rsplit('/').next().unwrap_or(&path);
    path.contains("/tests/")
        || path.starts_with("tests/")
        || path.contains("/__tests__/")
        || file.ends_with("_test.go")
        || file.starts_with("test_")
        || file.contains(".test.")
        || file.contains(".spec.")
        || file.contains(".stories.")
}

/// Source file from a Weavatrix node id (`symbol:path#name` or `file:path`).
#[must_use]
pub fn node_source_path(node_id: &str) -> Option<&str> {
    let raw = node_id
        .strip_prefix("file:")
        .or_else(|| node_id.strip_prefix("symbol:"))
        .unwrap_or(node_id);
    let path = raw.split('#').next().unwrap_or(raw);
    (!path.is_empty()).then_some(path)
}

/// Split nodes into implementation vs test. Test ids never become `CodeDelta`.
#[must_use]
pub fn partition_code_nodes(
    nodes: impl IntoIterator<Item = impl AsRef<str>>,
) -> (Vec<String>, Vec<String>) {
    let mut implementation = Vec::new();
    let mut tests = Vec::new();
    for node in nodes {
        let id = node.as_ref().to_owned();
        if node_source_path(&id).is_some_and(is_test_source_path) {
            tests.push(id);
        } else {
            implementation.push(id);
        }
    }
    implementation.sort();
    implementation.dedup();
    tests.sort();
    tests.dedup();
    (implementation, tests)
}

/// Build one obligation surface from protection flows.
#[must_use]
pub fn surface_from_flows(obligation: &str, flows: &[FlowProtection]) -> ObligationCodeSurface {
    let mut evidence = Vec::new();
    let mut implementation = BTreeSet::new();
    let mut tests = BTreeSet::new();
    for flow in flows {
        if !flow
            .proven_obligations
            .iter()
            .any(|item| item == obligation)
        {
            continue;
        }
        let mut nodes = Vec::new();
        nodes.push(flow.flow.clone());
        nodes.extend(flow.covered_nodes.iter().cloned());
        let (impl_nodes, test_nodes) = partition_code_nodes(nodes);
        if !impl_nodes.is_empty() {
            evidence.push(CodeSurfaceEvidence {
                kind: if flow.tests.is_empty() {
                    CodeSurfaceEvidenceKind::StaticWeavatrixFlow
                } else {
                    CodeSurfaceEvidenceKind::ExactDynamicCoverage
                },
                origin: flow.flow.clone(),
                nodes: impl_nodes.clone(),
            });
            implementation.extend(impl_nodes);
        }
        tests.extend(test_nodes);
    }
    evidence.sort_by_key(|item| item.kind);
    ObligationCodeSurface {
        obligation: obligation.to_owned(),
        implementation_nodes: implementation.into_iter().collect(),
        test_nodes: tests.into_iter().collect(),
        evidence,
    }
}

/// Surfaces for every requested obligation.
#[must_use]
pub fn surfaces_from_flows(
    obligations: &[ObligationId],
    flows: &[FlowProtection],
) -> Vec<ObligationCodeSurface> {
    obligations
        .iter()
        .map(|obligation| surface_from_flows(obligation.as_str(), flows))
        .collect()
}

/// Declared binding paths → surfaces. Test paths never become implementation nodes.
#[must_use]
pub fn surfaces_from_declared_paths(
    bindings: &[(String, BTreeSet<String>)],
) -> Vec<ObligationCodeSurface> {
    let mut by_obligation = BTreeMap::<String, ObligationCodeSurface>::new();
    for (path, obligations) in bindings {
        let origin = normalize_path(path);
        let is_test = is_test_source_path(&origin);
        for obligation in obligations {
            let surface = by_obligation
                .entry(obligation.clone())
                .or_insert_with(|| ObligationCodeSurface {
                    obligation: obligation.clone(),
                    ..ObligationCodeSurface::default()
                });
            if is_test {
                if !surface.test_nodes.contains(&origin) {
                    surface.test_nodes.push(origin.clone());
                }
                continue;
            }
            if !surface.implementation_nodes.contains(&origin) {
                surface.implementation_nodes.push(origin.clone());
            }
            surface.evidence.push(CodeSurfaceEvidence {
                kind: CodeSurfaceEvidenceKind::ReviewedExplicitMapping,
                origin: origin.clone(),
                nodes: vec![origin.clone()],
            });
        }
    }
    let mut out = by_obligation.into_values().collect::<Vec<_>>();
    for surface in &mut out {
        surface.implementation_nodes.sort();
        surface.implementation_nodes.dedup();
        surface.test_nodes.sort();
        surface.test_nodes.dedup();
    }
    out.sort_by(|left, right| left.obligation.cmp(&right.obligation));
    out
}

/// Obligations allowed to judge a mutant on `path`.
///
/// Empty owners means no obligation owns this path. The mutant stays
/// unmeasured; unrelated candidates are not allowed to judge it.
#[must_use]
pub fn obligations_owning_path(
    surfaces: &[ObligationCodeSurface],
    path: &str,
    candidates: &[String],
) -> Vec<String> {
    surfaces
        .iter()
        .filter(|surface| {
            candidates.iter().any(|item| item == &surface.obligation)
                && surface.contains_implementation_path(path)
        })
        .map(|surface| surface.obligation.clone())
        .collect()
}

fn normalize_path(path: &str) -> String {
    path.replace('\\', "/")
}
