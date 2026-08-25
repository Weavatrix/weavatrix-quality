//! Extracted command-bus helper.

use super::access::*;
use super::protection_graph_extra::graph_singleton_path;
use super::protection_lineage::{
    approved_replaced_flows, graph_relocations, protection_lineage, protection_test_changes,
    snapshot_relocations,
};

pub(in crate::service) fn expectation_change(
    base: &[TestObligation],
    head: &[TestObligation],
    seal_changed: bool,
) -> (Vec<String>, Vec<(String, String)>) {
    let mut changed = BTreeSet::new();
    for before in base {
        if let Some(after) = head.iter().find(|item| item.id == before.id)
            && before != after
        {
            changed.insert(before.id.to_string());
        }
    }
    let removed = base
        .iter()
        .filter(|before| !head.iter().any(|after| after.id == before.id))
        .collect::<Vec<_>>();
    let added = head
        .iter()
        .filter(|after| !base.iter().any(|before| before.id == after.id))
        .collect::<Vec<_>>();
    let same_slot = |before: &TestObligation, after: &TestObligation| {
        before.requirement == after.requirement
            && before.scenario == after.scenario
            && before.kind == after.kind
    };
    let mut replacements = Vec::new();
    for before in &removed {
        changed.insert(before.id.to_string());
        let candidates = added
            .iter()
            .filter(|after| same_slot(before, after))
            .collect::<Vec<_>>();
        if candidates.len() == 1
            && removed
                .iter()
                .filter(|candidate| same_slot(candidate, candidates[0]))
                .count()
                == 1
        {
            replacements.push((before.id.to_string(), candidates[0].id.to_string()));
        }
    }
    changed.extend(added.iter().map(|item| item.id.to_string()));
    if seal_changed && changed.is_empty() {
        changed.extend(base.iter().map(|item| item.id.to_string()));
        changed.extend(head.iter().map(|item| item.id.to_string()));
    }
    replacements.sort();
    replacements.dedup();
    (changed.into_iter().collect(), replacements)
}

pub(in crate::service) fn build_protection_view(
    obligations: &[TestObligation],
    diff: &Value,
    snapshots: (&ProtectionSnapshot, &ProtectionSnapshot),
    graphs: (&Value, &Value),
    files: &ChangedFiles,
    oracle_replacement: Option<OracleReplacementReview>,
) -> ProtectionView {
    let (base, head) = snapshots;
    let (base_graph, head_graph) = graphs;
    let mut relocations = graph_relocations(diff);
    relocations.extend(snapshot_relocations(base, head));
    relocations.sort();
    relocations.dedup();
    let context = DeltaContext {
        critical_branches: Vec::new(),
        intentionally_removed: Vec::new(),
        approved_replaced_flows: approved_replaced_flows(base, head, oracle_replacement.as_ref()),
        relocations,
        changed_obligations: oracle_replacement
            .as_ref()
            .map(|review| review.changed_obligations.clone())
            .unwrap_or_default(),
        obligation_replacements: oracle_replacement
            .as_ref()
            .map(|review| review.obligation_replacements.clone())
            .unwrap_or_default(),
        oracle_replacement_approved: oracle_replacement
            .as_ref()
            .is_some_and(|review| review.approved),
    };
    let deltas = protection_delta(base, head, &context);
    let lineage = protection_lineage(base, head);
    let changed_tests = files.changed_tests().into_iter().collect::<BTreeSet<_>>();
    let tests = protection_test_changes(base, head, &deltas, &changed_tests, &context);
    let any_high_risk = obligations
        .iter()
        .any(|item| matches!(item.risk, RiskLevel::High | RiskLevel::Critical));
    let high_risk_flows = if any_high_risk {
        deltas.iter().map(|item| item.flow.clone()).collect()
    } else {
        Vec::new()
    };
    let findings = gate_protection(&ProtectionCheckInput {
        deltas: deltas.clone(),
        tests,
        trends: Vec::new(),
        policy: ProtectionPolicy {
            high_risk_flows,
            substitution_ratio: 10,
        },
    });
    let flows = deltas
        .iter()
        .map(|delta| {
            let before = base.flow(&delta.flow);
            let head_name = context
                .relocations
                .iter()
                .find(|(source, _)| source == &delta.flow)
                .map_or(delta.flow.as_str(), |(_, target)| target.as_str());
            let after = head.flow(head_name);
            FlowView {
                flow: delta.flow.clone(),
                base_path: graph_singleton_path(base_graph, &delta.flow),
                head_path: graph_singleton_path(head_graph, head_name),
                requirements: before
                    .into_iter()
                    .flat_map(|item| item.proven_obligations.iter().cloned())
                    .chain(
                        after
                            .into_iter()
                            .flat_map(|item| item.proven_obligations.iter().cloned()),
                    )
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect(),
                tests_before: before.map(|item| item.tests.clone()).unwrap_or_default(),
                tests_after: after.map(|item| item.tests.clone()).unwrap_or_default(),
                coverage_before: before
                    .map(|item| item.covered_branches.clone())
                    .unwrap_or_default(),
                coverage_after: after
                    .map(|item| item.covered_branches.clone())
                    .unwrap_or_default(),
                proof_before: before.map(|item| item.proofs.clone()).unwrap_or_default(),
                proof_after: after.map(|item| item.proofs.clone()).unwrap_or_default(),
            }
        })
        .collect();
    ProtectionView {
        deltas,
        findings,
        lineage,
        flows,
        oracle_replacement,
    }
}

