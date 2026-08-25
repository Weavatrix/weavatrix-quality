//! Adaptive responsive-width search over measured base/head deltas.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::{ResponsivePolicy, UiFindingState, UiIntegrityDelta, UiIntegrityFinding};

/// Device and layout widths always seeded inside the configured range.
///
/// CSS/container breakpoints still get ±1 neighbours so a transition can be
/// isolated to one pixel. These sentinels fill the holes a fluid layout has
/// when the range bounds agree and bisection would otherwise never run.
/// They are not a full viewport matrix: a width that is not a sentinel and
/// not next to a parsed breakpoint is still unmeasured until two adjacent
/// probes disagree.
pub const RESPONSIVE_SENTINEL_WIDTHS: [u32; 8] =
    [360, 390, 414, 480, 640, 768, 1_024, 1_280];

/// Initial widths derived from sentinels, CSS/container breakpoints, and bounds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResponsiveProbePlan {
    /// Widths to measure in ascending order.
    pub widths: Vec<u32>,
    /// True when the configured run budget could not include every seed.
    pub truncated: bool,
}

/// One measured base/head delta at an exact viewport width.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResponsiveProbe {
    /// CSS viewport width.
    pub width: u32,
    /// Ratcheted findings at this width.
    pub delta: UiIntegrityDelta,
}

/// A responsive regression and the exact measured width interval where it exists.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResponsiveFailureInterval {
    /// New or returned classification.
    pub state: UiFindingState,
    /// First failing width in the configured range.
    pub first_width: u32,
    /// Last failing width in the configured range.
    pub last_width: u32,
    /// Whether a passing point or the configured lower bound proves the start.
    pub lower_boundary_exact: bool,
    /// Whether a passing point or the configured upper bound proves the end.
    pub upper_boundary_exact: bool,
    /// Representative measured finding at the first failing width.
    pub finding: UiIntegrityFinding,
}

/// Build the bounded seed set: range bounds, sentinels, then CSS ±1 neighbours.
///
/// Truncation never drops the configured min/max: those bounds are what make
/// a failure interval exact at the edge of the search. Extra CSS neighbours
/// go first when the probe budget is tight, then extra sentinels.
#[must_use]
pub fn responsive_probe_plan(
    policy: &ResponsivePolicy,
    breakpoints: &BTreeSet<u32>,
) -> ResponsiveProbePlan {
    if !policy.enabled {
        return ResponsiveProbePlan {
            widths: Vec::new(),
            truncated: false,
        };
    }
    let range = policy.min_width..=policy.max_width;
    let bounds = BTreeSet::from([policy.min_width, policy.max_width]);
    let mut seeds = BTreeSet::new();
    for width in RESPONSIVE_SENTINEL_WIDTHS {
        if range.contains(&width) {
            seeds.insert(width);
        }
    }
    let mut neighbours = BTreeSet::new();
    for breakpoint in breakpoints.range(range.clone()).copied() {
        seeds.insert(breakpoint);
        for width in [breakpoint.saturating_sub(1), breakpoint.saturating_add(1)] {
            if range.contains(&width) {
                neighbours.insert(width);
            }
        }
    }
    for width in &bounds {
        seeds.remove(width);
        neighbours.remove(width);
    }
    for width in &seeds {
        neighbours.remove(width);
    }

    let limit = usize::try_from(policy.max_probes).unwrap_or(usize::MAX);
    let mut widths = BTreeSet::new();
    let truncated = absorb(&mut widths, &bounds, limit)
        || absorb(&mut widths, &seeds, limit)
        || absorb(&mut widths, &neighbours, limit);
    ResponsiveProbePlan {
        widths: widths.into_iter().collect(),
        truncated,
    }
}

fn absorb(dest: &mut BTreeSet<u32>, source: &BTreeSet<u32>, limit: usize) -> bool {
    for width in source {
        if dest.len() >= limit {
            return true;
        }
        dest.insert(*width);
    }
    false
}

/// Choose the next midpoint where two measured widths disagree.
///
/// Repeating this until `None` isolates every observed transition to one CSS
/// pixel. Equal endpoints are not recursively scanned: sentinels and
/// CSS/container seeds are what make the search cheap instead of a fixed
/// full viewport matrix.
#[must_use]
pub fn next_responsive_probe(policy: &ResponsivePolicy, probes: &[ResponsiveProbe]) -> Option<u32> {
    if probes.len() >= usize::try_from(policy.max_probes).unwrap_or(usize::MAX) {
        return None;
    }
    let ordered = probes
        .iter()
        .map(|probe| (probe.width, signature(probe)))
        .collect::<BTreeMap<_, _>>();
    let mut best = None;
    for ((left_width, left), (right_width, right)) in ordered.iter().zip(ordered.iter().skip(1)) {
        if left == right || right_width.saturating_sub(*left_width) <= 1 {
            continue;
        }
        let gap = right_width - left_width;
        let midpoint = left_width + gap / 2;
        if !ordered.contains_key(&midpoint)
            && best.is_none_or(|(best_gap, best_width)| {
                gap > best_gap || (gap == best_gap && midpoint < best_width)
            })
        {
            best = Some((gap, midpoint));
        }
    }
    best.map(|(_, width)| width)
}

/// Collapse responsive findings into deterministic measured intervals.
#[must_use]
pub fn responsive_failure_intervals(
    policy: &ResponsivePolicy,
    probes: &[ResponsiveProbe],
) -> Vec<ResponsiveFailureInterval> {
    let ordered = probes
        .iter()
        .map(|probe| (probe.width, probe))
        .collect::<BTreeMap<_, _>>();
    let mut identities: BTreeMap<(UiFindingState, String), UiIntegrityFinding> = BTreeMap::new();
    for probe in ordered.values() {
        for (state, finding) in classified_findings(&probe.delta) {
            identities
                .entry((state, finding.responsive_identity()))
                .or_insert_with(|| finding.clone());
        }
    }

    let widths = ordered.keys().copied().collect::<Vec<_>>();
    let mut intervals = Vec::new();
    for ((state, identity), _) in identities {
        let present = widths
            .iter()
            .map(|width| {
                ordered[width]
                    .delta
                    .new
                    .iter()
                    .map(|finding| (UiFindingState::New, finding))
                    .chain(
                        ordered[width]
                            .delta
                            .returned
                            .iter()
                            .map(|finding| (UiFindingState::Returned, finding)),
                    )
                    .find(|(candidate_state, finding)| {
                        *candidate_state == state && finding.responsive_identity() == identity
                    })
                    .map(|(_, finding)| finding.clone())
            })
            .collect::<Vec<_>>();
        let mut index = 0;
        while index < widths.len() {
            if present[index].is_none() {
                index += 1;
                continue;
            }
            let start = index;
            while index + 1 < widths.len() && present[index + 1].is_some() {
                index += 1;
            }
            let end = index;
            let Some(finding) = present[start].clone() else {
                index += 1;
                continue;
            };
            intervals.push(ResponsiveFailureInterval {
                state,
                first_width: widths[start],
                last_width: widths[end],
                lower_boundary_exact: widths[start] == policy.min_width
                    || start > 0 && widths[start] - widths[start - 1] == 1,
                upper_boundary_exact: widths[end] == policy.max_width
                    || end + 1 < widths.len() && widths[end + 1] - widths[end] == 1,
                finding,
            });
            index += 1;
        }
    }
    intervals.sort_by(|left, right| {
        (
            left.finding.order_key(),
            left.state,
            left.first_width,
            left.last_width,
        )
            .cmp(&(
                right.finding.order_key(),
                right.state,
                right.first_width,
                right.last_width,
            ))
    });
    intervals
}

fn signature(probe: &ResponsiveProbe) -> BTreeSet<(UiFindingState, String)> {
    classified_findings(&probe.delta)
        .map(|(state, finding)| (state, finding.responsive_identity()))
        .collect()
}

fn classified_findings(
    delta: &UiIntegrityDelta,
) -> impl Iterator<Item = (UiFindingState, &UiIntegrityFinding)> {
    delta
        .new
        .iter()
        .filter(|finding| finding.check.is_responsive())
        .map(|finding| (UiFindingState::New, finding))
        .chain(
            delta
                .returned
                .iter()
                .filter(|finding| finding.check.is_responsive())
                .map(|finding| (UiFindingState::Returned, finding)),
        )
}
