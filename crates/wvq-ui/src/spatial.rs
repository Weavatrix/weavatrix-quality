//! Bounded candidate-pair search for overlap detection.
//!
//! A full pairwise scan is `O(n²)`: at the 5 000-node default that is 12.5
//! million comparisons for every state, on every revision, at every viewport.
//! A sweep line over the x axis gives the same answer in `O(n log n + k)`,
//! where `k` is the number of pairs that actually share horizontal extent.
//!
//! The sweep is also *bounded*: it stops emitting after
//! [`MAX_CANDIDATE_PAIRS`] and says so, because silently dropping candidates
//! would read as "no overlaps found".

use crate::snapshot::Rect;

/// Ceiling on emitted candidate pairs for one state.
pub const MAX_CANDIDATE_PAIRS: usize = 200_000;

/// Result of one sweep.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CandidatePairs {
    /// Index pairs whose rectangles genuinely intersect, `left < right`.
    pub pairs: Vec<(usize, usize)>,
    /// True when [`MAX_CANDIDATE_PAIRS`] stopped the sweep early.
    pub truncated: bool,
}

/// Every intersecting pair among `rects`, found with a sweep line.
///
/// Entries are sorted by left edge, then swept while an active set holds only
/// the rectangles whose right edge is still ahead of the cursor. Two boxes that
/// never share horizontal extent are never compared.
#[must_use]
pub fn overlapping_pairs(rects: &[Option<Rect>]) -> CandidatePairs {
    // Carry the rectangle alongside its index so the sweep never re-indexes and
    // never has to unwrap.
    let mut order: Vec<(usize, Rect)> = rects
        .iter()
        .enumerate()
        .filter_map(|(index, rect)| {
            rect.filter(|rect| !rect.is_empty())
                .map(|rect| (index, rect))
        })
        .collect();
    order.sort_by(|(left_index, left), (right_index, right)| {
        left.x
            .partial_cmp(&right.x)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| left_index.cmp(right_index))
    });

    let mut out = CandidatePairs::default();
    // Rectangles whose right edge is still ahead of the cursor. Anything that
    // ended before the current left edge can never intersect again.
    let mut active: Vec<(usize, Rect)> = Vec::new();
    for (index, current) in order {
        active.retain(|(_, rect)| rect.right() > current.x);
        for (other, candidate) in &active {
            if candidate.intersection(&current).is_none() {
                continue;
            }
            if out.pairs.len() >= MAX_CANDIDATE_PAIRS {
                out.truncated = true;
                out.pairs.sort_unstable();
                out.pairs.dedup();
                return out;
            }
            out.pairs.push(if *other < index {
                (*other, index)
            } else {
                (index, *other)
            });
        }
        active.push((index, current));
    }
    out.pairs.sort_unstable();
    out.pairs.dedup();
    out
}
