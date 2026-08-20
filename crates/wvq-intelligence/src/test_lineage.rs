//! Spec §71 test lineage.
//!
//! The rule that makes this worth having:
//!
//! > same test file/name ≠ same protection
//!
//! A test can survive a change by name and source and still stop reaching the
//! production path it was believed to protect. Lineage therefore tracks the
//! *dynamic* protection a test provides, not only where its source lives.

use std::collections::BTreeSet;

use serde::Serialize;

/// What happened to a test between two revisions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TestLineageState {
    /// Same identity, same source.
    Unchanged,
    /// Same identity, changed source.
    Modified,
    /// Same test under a new name or path.
    Renamed,
    /// One base test became several head tests.
    Split,
    /// Several base tests became one head test.
    Merged,
    /// Head-only.
    Added,
    /// Base-only. The protection it carried is gone unless replaced.
    Removed,
    /// Continuity could not be established. WVQ does not guess.
    Unmatched,
}

impl TestLineageState {
    /// Stable token for transport.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unchanged => "unchanged",
            Self::Modified => "modified",
            Self::Renamed => "renamed",
            Self::Split => "split",
            Self::Merged => "merged",
            Self::Added => "added",
            Self::Removed => "removed",
            Self::Unmatched => "unmatched",
        }
    }

    /// Whether the test still exists on head in some form.
    #[must_use]
    pub fn survives(self) -> bool {
        !matches!(self, Self::Removed | Self::Unmatched)
    }
}

/// One test at one revision, with what it actually executed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TestFacts {
    /// Test identity within its revision.
    pub id: String,
    /// Revision this observation belongs to.
    pub revision: String,
    /// Stable test name.
    pub name: String,
    /// Source path. Used last, never first.
    pub path: String,
    /// Digest of the test body.
    pub body_digest: String,
    /// Git rename evidence: the base id this head test came from.
    pub renamed_from: Option<String>,
    /// Graph nodes the test measurably executed.
    pub covered_nodes: Vec<String>,
    /// Obligations it proved.
    pub covered_obligations: Vec<String>,
    /// Flows it reached.
    pub covered_flows: Vec<String>,
}

/// One lineage record. Source continuity and protection continuity are separate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TestLineage {
    /// Source-level lineage.
    pub state: TestLineageState,
    /// Base test id, when there is one.
    pub base: Option<String>,
    /// Head test id, when there is one.
    pub head: Option<String>,
    /// What the two sides were matched on.
    pub matched_on: &'static str,
    /// Whether the dynamic protection changed even though the test survived.
    pub protection_changed: bool,
    /// Flows the test used to reach and no longer does.
    pub lost_flows: Vec<String>,
    /// Flows it newly reaches.
    pub gained_flows: Vec<String>,
    /// Obligations it used to prove and no longer does.
    pub lost_obligations: Vec<String>,
}

impl TestLineage {
    /// Whether the test exists on head but protects less than it did.
    ///
    /// This is the phantom-test case from spec §109: green, present, and no
    /// longer guarding what everyone believes it guards.
    #[must_use]
    pub fn is_phantom(&self) -> bool {
        self.state.survives() && !self.lost_flows.is_empty()
    }
}

/// Track every test across the two revisions.
///
/// Matching order follows spec §71: recorded Git rename, then stable name, then
/// body fingerprint, then the obligations the test covered. Paths are consulted
/// last so a moved test keeps its lineage.
#[must_use]
pub fn track_lineage(base: &[TestFacts], head: &[TestFacts]) -> Vec<TestLineage> {
    let mut out = Vec::new();
    let mut used: BTreeSet<usize> = BTreeSet::new();

    for base_test in base {
        let matched = find_match(base_test, head, &used);
        if let Some((index, matched_on)) = matched {
            // One base test claimed by several head tests is a split.
            let siblings: Vec<usize> = head
                .iter()
                .enumerate()
                .filter(|(other, item)| {
                    *other != index
                        && !used.contains(other)
                        && item.renamed_from.as_deref() == Some(base_test.id.as_str())
                })
                .map(|(other, _)| other)
                .collect();
            used.insert(index);
            if siblings.is_empty() {
                out.push(record(base_test, &head[index], matched_on, None));
            } else {
                out.push(record(
                    base_test,
                    &head[index],
                    matched_on,
                    Some(TestLineageState::Split),
                ));
                for sibling in siblings {
                    used.insert(sibling);
                    out.push(record(
                        base_test,
                        &head[sibling],
                        "git_rename",
                        Some(TestLineageState::Split),
                    ));
                }
            }
            continue;
        }
        // Nothing free matched. A head test that already absorbed everything
        // this one protected is a merge, not a removal.
        if let Some(index) = find_absorbing(base_test, head) {
            out.push(record(
                base_test,
                &head[index],
                "absorbed_protection",
                Some(TestLineageState::Merged),
            ));
            continue;
        }
        out.push(TestLineage {
            state: TestLineageState::Removed,
            base: Some(base_test.id.clone()),
            head: None,
            matched_on: "none",
            protection_changed: true,
            lost_flows: sorted(&base_test.covered_flows),
            gained_flows: Vec::new(),
            lost_obligations: sorted(&base_test.covered_obligations),
        });
    }

    for (index, head_test) in head.iter().enumerate() {
        if used.contains(&index) {
            continue;
        }
        out.push(TestLineage {
            state: TestLineageState::Added,
            base: None,
            head: Some(head_test.id.clone()),
            matched_on: "none",
            protection_changed: true,
            lost_flows: Vec::new(),
            gained_flows: sorted(&head_test.covered_flows),
            lost_obligations: Vec::new(),
        });
    }

    mark_merges(&mut out);
    out.sort_by(|left, right| {
        left.base
            .cmp(&right.base)
            .then_with(|| left.head.cmp(&right.head))
    });
    out
}

fn find_match(
    base_test: &TestFacts,
    head: &[TestFacts],
    used: &BTreeSet<usize>,
) -> Option<(usize, &'static str)> {
    let free = |index: &usize| !used.contains(index);

    let by_rename = head
        .iter()
        .enumerate()
        .find(|(index, item)| {
            free(index) && item.renamed_from.as_deref() == Some(base_test.id.as_str())
        })
        .map(|(index, _)| (index, "git_rename"));
    if by_rename.is_some() {
        return by_rename;
    }
    let by_name = head
        .iter()
        .enumerate()
        .find(|(index, item)| free(index) && item.name == base_test.name)
        .map(|(index, _)| (index, "test_name"));
    if by_name.is_some() {
        return by_name;
    }
    let by_body = head
        .iter()
        .enumerate()
        .find(|(index, item)| {
            free(index) && !item.body_digest.is_empty() && item.body_digest == base_test.body_digest
        })
        .map(|(index, _)| (index, "body_fingerprint"));
    if by_body.is_some() {
        return by_body;
    }
    head.iter()
        .enumerate()
        .find(|(index, item)| {
            free(index)
                && item
                    .covered_obligations
                    .iter()
                    .any(|obligation| base_test.covered_obligations.contains(obligation))
        })
        .map(|(index, _)| (index, "covered_obligation"))
}

/// A head test that already covers everything this base test protected.
///
/// Requiring full coverage of both flows and obligations keeps an unrelated test
/// from being mistaken for a replacement: a genuine removal must stay a removal.
fn find_absorbing(base_test: &TestFacts, head: &[TestFacts]) -> Option<usize> {
    if base_test.covered_flows.is_empty() {
        return None;
    }
    head.iter().position(|item| {
        base_test
            .covered_flows
            .iter()
            .all(|flow| item.covered_flows.contains(flow))
            && base_test
                .covered_obligations
                .iter()
                .all(|obligation| item.covered_obligations.contains(obligation))
    })
}

fn record(
    base_test: &TestFacts,
    head_test: &TestFacts,
    matched_on: &'static str,
    forced: Option<TestLineageState>,
) -> TestLineage {
    let lost_flows = difference(&base_test.covered_flows, &head_test.covered_flows);
    let gained_flows = difference(&head_test.covered_flows, &base_test.covered_flows);
    let lost_obligations = difference(
        &base_test.covered_obligations,
        &head_test.covered_obligations,
    );
    let protection_changed = !lost_flows.is_empty() || !gained_flows.is_empty();

    let state = forced.unwrap_or_else(|| {
        if base_test.name != head_test.name || base_test.path != head_test.path {
            TestLineageState::Renamed
        } else if base_test.body_digest == head_test.body_digest {
            TestLineageState::Unchanged
        } else {
            TestLineageState::Modified
        }
    });

    TestLineage {
        state,
        base: Some(base_test.id.clone()),
        head: Some(head_test.id.clone()),
        matched_on,
        protection_changed,
        lost_flows,
        gained_flows,
        lost_obligations,
    }
}

/// Several base tests folded into one head test.
fn mark_merges(records: &mut [TestLineage]) {
    let merged: Vec<String> = records
        .iter()
        .filter_map(|item| item.head.clone())
        .filter(|head_id| {
            records
                .iter()
                .filter(|other| other.head.as_ref() == Some(head_id) && other.base.is_some())
                .count()
                > 1
        })
        .collect();
    for item in records.iter_mut() {
        if let Some(head_id) = &item.head
            && item.base.is_some()
            && merged.contains(head_id)
            && item.state != TestLineageState::Split
        {
            item.state = TestLineageState::Merged;
        }
    }
}

fn difference(left: &[String], right: &[String]) -> Vec<String> {
    let right_set: BTreeSet<&String> = right.iter().collect();
    let mut out: Vec<String> = left
        .iter()
        .filter(|item| !right_set.contains(item))
        .cloned()
        .collect();
    out.sort();
    out.dedup();
    out
}

fn sorted(values: &[String]) -> Vec<String> {
    let set: BTreeSet<&String> = values.iter().collect();
    set.into_iter().cloned().collect()
}
