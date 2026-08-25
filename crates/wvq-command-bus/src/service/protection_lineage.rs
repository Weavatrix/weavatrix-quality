//! Extracted command-bus helper.

use super::access::*;
use super::protection_graph_extra::graph_singleton_path;

pub(in crate::service) fn protection_test_changes(
    base: &ProtectionSnapshot,
    head: &ProtectionSnapshot,
    deltas: &[ProtectionDelta],
    changed_tests: &BTreeSet<String>,
    context: &DeltaContext,
) -> Vec<TestChange> {
    let mut changes = Vec::new();
    for item in protection_lineage(base, head) {
        for flow in &item.lost_flows {
            let delta = deltas.iter().find(|delta| delta.flow == *flow);
            if delta.is_some_and(|delta| {
                matches!(
                    delta.state,
                    ProtectionDeltaState::Preserved
                        | ProtectionDeltaState::Improved
                        | ProtectionDeltaState::Replaced
                        | ProtectionDeltaState::Relocated
                )
            }) {
                // Source identity changed, but measured protection followed the
                // semantic flow. Moving a test with its implementation is not
                // evidence that its oracle was weakened.
                continue;
            }
            changes.push(TestChange {
                test: item.test.clone(),
                flow: flow.clone(),
                survives: item.state != "removed",
                lost_flows: item.lost_flows.clone(),
                lost_obligations: delta
                    .map(|delta| delta.lost_obligations.clone())
                    .unwrap_or_default(),
                replaced_by: replacement_test_for_flow(base, head, flow, context),
                assertions_weakened: false,
                changed_with_implementation: changed_tests
                    .iter()
                    .any(|path| test_identity_has_path(&item.test, path)),
                new_oracle_seal: context.oracle_replacement_approved,
                declared_spec_delta: !context.changed_obligations.is_empty(),
            });
        }
    }
    changes
}

pub(in crate::service) fn approved_replaced_flows(
    base: &ProtectionSnapshot,
    head: &ProtectionSnapshot,
    review: Option<&OracleReplacementReview>,
) -> Vec<String> {
    let Some(review) = review.filter(|review| review.approved) else {
        return Vec::new();
    };
    let proven_on_head = head
        .flows
        .iter()
        .flat_map(|flow| flow.proven_obligations.iter().map(String::as_str))
        .collect::<BTreeSet<_>>();
    base.flows
        .iter()
        .filter(|flow| {
            head.flow(&flow.flow)
                .is_none_or(|candidate| !candidate.is_protected())
        })
        .filter(|flow| {
            !flow.proven_obligations.is_empty()
                && flow.proven_obligations.iter().all(|before| {
                    review
                        .obligation_replacements
                        .iter()
                        .any(|(from, to)| from == before && proven_on_head.contains(to.as_str()))
                })
        })
        .map(|flow| flow.flow.clone())
        .collect()
}

pub(in crate::service) fn replacement_test_for_flow(
    base: &ProtectionSnapshot,
    head: &ProtectionSnapshot,
    flow: &str,
    context: &DeltaContext,
) -> Option<String> {
    if !context.oracle_replacement_approved {
        return None;
    }
    let before = base.flow(flow)?;
    let targets = before
        .proven_obligations
        .iter()
        .filter_map(|obligation| {
            context
                .obligation_replacements
                .iter()
                .find(|(from, _)| from == obligation)
                .map(|(_, to)| to.as_str())
        })
        .collect::<BTreeSet<_>>();
    if targets.len() != before.proven_obligations.len() {
        return None;
    }
    head.flows
        .iter()
        .filter(|candidate| {
            candidate
                .proven_obligations
                .iter()
                .any(|obligation| targets.contains(obligation.as_str()))
        })
        .flat_map(|candidate| candidate.tests.iter())
        .min()
        .cloned()
}

pub(in crate::service) fn test_identity_has_path(identity: &str, path: &str) -> bool {
    identity == path
        || identity
            .strip_prefix(path)
            .is_some_and(|suffix| suffix.starts_with('#'))
}

pub(in crate::service) fn graph_relocations(diff: &Value) -> Vec<(String, String)> {
    let mut relocations = values_at(diff, "/nodes/changed")
        .iter()
        .filter_map(|changed| {
            let before = changed.get("before").and_then(graph_node_id)?;
            let after = changed.get("after").and_then(graph_node_id)?;
            (before != after).then_some((before, after))
        })
        .collect::<Vec<_>>();
    let removed = values_at(diff, "/nodes/removed")
        .iter()
        .filter_map(graph_node_id)
        .collect::<Vec<_>>();
    let added = values_at(diff, "/nodes/added")
        .iter()
        .filter_map(graph_node_id)
        .collect::<Vec<_>>();
    for before in &removed {
        let Some(signature) = stable_symbol_signature(before) else {
            continue;
        };
        let candidates = added
            .iter()
            .filter(|after| stable_symbol_signature(after) == Some(signature))
            .collect::<Vec<_>>();
        if candidates.len() == 1
            && removed
                .iter()
                .filter(|candidate| stable_symbol_signature(candidate) == Some(signature))
                .count()
                == 1
        {
            relocations.push((before.clone(), candidates[0].clone()));
        }
    }
    relocations.sort();
    relocations.dedup();
    relocations
}

pub(in crate::service) fn snapshot_relocations(
    base: &ProtectionSnapshot,
    head: &ProtectionSnapshot,
) -> Vec<(String, String)> {
    let mut relocations = Vec::new();
    for before in &base.flows {
        if head.flow(&before.flow).is_some() {
            continue;
        }
        let Some(signature) = stable_symbol_signature(&before.flow) else {
            continue;
        };
        let candidates = head
            .flows
            .iter()
            .filter(|after| stable_symbol_signature(&after.flow) == Some(signature))
            .collect::<Vec<_>>();
        if candidates.len() == 1
            && base
                .flows
                .iter()
                .filter(|candidate| stable_symbol_signature(&candidate.flow) == Some(signature))
                .count()
                == 1
        {
            relocations.push((before.flow.clone(), candidates[0].flow.clone()));
        }
    }
    relocations.sort();
    relocations.dedup();
    relocations
}

pub(in crate::service) fn stable_symbol_signature(id: &str) -> Option<&str> {
    let tail = id.strip_prefix("symbol:")?.split_once('#')?.1;
    let Some((symbol, position)) = tail.rsplit_once('@') else {
        return Some(tail);
    };
    let is_source_position = position.split_once(':').is_some_and(|(line, column)| {
        !line.is_empty()
            && !column.is_empty()
            && line.bytes().all(|byte| byte.is_ascii_digit())
            && column.bytes().all(|byte| byte.is_ascii_digit())
    });
    Some(if is_source_position { symbol } else { tail })
}

pub(in crate::service) fn protection_lineage(
    base: &ProtectionSnapshot,
    head: &ProtectionSnapshot,
) -> Vec<TestLineageView> {
    let mut base_flows = BTreeMap::<String, BTreeSet<String>>::new();
    let mut head_flows = BTreeMap::<String, BTreeSet<String>>::new();
    for flow in &base.flows {
        for test in &flow.tests {
            base_flows
                .entry(test.clone())
                .or_default()
                .insert(flow.flow.clone());
        }
    }
    for flow in &head.flows {
        for test in &flow.tests {
            head_flows
                .entry(test.clone())
                .or_default()
                .insert(flow.flow.clone());
        }
    }
    let base_executed = base.executed_test_identities();
    let head_executed = head.executed_test_identities();
    let tests = base_executed
        .iter()
        .chain(&head_executed)
        .cloned()
        .collect::<BTreeSet<_>>();
    tests
        .into_iter()
        .map(|test| {
            let before = base_flows.get(&test).cloned().unwrap_or_default();
            let after = head_flows.get(&test).cloned().unwrap_or_default();
            let lost_flows = before.difference(&after).cloned().collect::<Vec<_>>();
            let gained_flows = after.difference(&before).cloned().collect::<Vec<_>>();
            let present_before = base_executed.contains(&test);
            let present_after = head_executed.contains(&test);
            let phantom = present_before && present_after && !lost_flows.is_empty();
            TestLineageView {
                state: match (present_before, present_after) {
                    (true, true) => "unchanged",
                    (true, false) => "removed",
                    (false, true) => "added",
                    (false, false) => "unknown",
                }
                .into(),
                matched_on: "exact test identity".into(),
                protection_changed: !lost_flows.is_empty() || !gained_flows.is_empty(),
                phantom,
                test,
                lost_flows,
                gained_flows,
            }
        })
        .collect()
}
