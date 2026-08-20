//! Spec §65.4 capability clustering.
//!
//! One commit is not one requirement. A commit sequence is grouped into
//! capability changes using the strongest structural evidence available, and
//! commit-title similarity is the last resort before an agent is asked anything.
//! Every cluster keeps the exact commits it was built from.

use std::collections::BTreeMap;

use serde::Serialize;

/// Stop-words that carry no capability meaning in a commit title.
const NOISE: &[&str] = &[
    "add", "adds", "added", "fix", "fixes", "fixed", "update", "updates", "chore", "feat",
    "refactor", "remove", "removes", "removed", "test", "tests", "wip", "the", "and", "for",
    "with", "into", "from", "that", "this", "when", "then",
];

/// Why a group of commits was put together. Lower rank is stronger evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ClusterBasis {
    /// Commits name the same existing `OpenSpec` capability.
    OpenSpecCapability,
    /// Commits reference the same issue or task.
    LinkedIssue,
    /// Commits touch the same public API or route.
    PublicApi,
    /// Commits fall in the same Weavatrix module or community.
    Community,
    /// Commits change the same owned component.
    Component,
    /// Commits sit in the same dependency neighbourhood.
    DependencyNeighborhood,
    /// Commits are adjacent in the sequence and share nothing structural.
    CommitAdjacency,
    /// Only the titles resemble each other. Weakest deterministic basis.
    TitleSimilarity,
    /// Nothing grouped this commit. Uncertainty stays uncertainty.
    Unclustered,
}

impl ClusterBasis {
    /// Stable token for transport.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::OpenSpecCapability => "openspec_capability",
            Self::LinkedIssue => "linked_issue",
            Self::PublicApi => "public_api",
            Self::Community => "community",
            Self::Component => "component",
            Self::DependencyNeighborhood => "dependency_neighborhood",
            Self::CommitAdjacency => "commit_adjacency",
            Self::TitleSimilarity => "title_similarity",
            Self::Unclustered => "unclustered",
        }
    }

    /// Whether this basis rests on structure rather than wording.
    #[must_use]
    pub fn is_structural(self) -> bool {
        self <= Self::DependencyNeighborhood
    }
}

/// One commit plus the revision-bound evidence attached to it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CommitFacts {
    /// Commit identity. Preserved verbatim in the cluster.
    pub id: String,
    /// Commit title. A grouping hint only, never a requirement.
    pub title: String,
    /// Position in the sequence, used for adjacency.
    pub index: u32,
    /// Existing `OpenSpec` capability this commit touches.
    pub capability: Option<String>,
    /// Linked issue or task.
    pub issue: Option<String>,
    /// Public endpoints or routes touched.
    pub endpoints: Vec<String>,
    /// Weavatrix module or community.
    pub community: Option<String>,
    /// Owned components changed.
    pub components: Vec<String>,
    /// Dependency neighbourhood.
    pub neighbors: Vec<String>,
}

/// A capability change assembled from one or more commits.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CapabilityCluster {
    /// Deterministic cluster identity.
    pub id: String,
    /// Why these commits belong together.
    pub basis: ClusterBasis,
    /// Exact commits, sorted. Provenance is never lost.
    pub commits: Vec<String>,
    /// Capability, when structural evidence named one.
    pub capability: Option<String>,
    /// Union of endpoints touched.
    pub endpoints: Vec<String>,
    /// Union of components changed.
    pub components: Vec<String>,
    /// Commit titles, kept as hints.
    pub title_hints: Vec<String>,
}

/// Group a commit sequence into capability changes.
///
/// Structural evidence wins over wording: two commits with near-identical titles
/// stay apart when they name different capabilities, and two commits with
/// unrelated titles merge when they name the same one.
#[must_use]
pub fn cluster(commits: &[CommitFacts]) -> Vec<CapabilityCluster> {
    let mut groups: BTreeMap<(ClusterBasis, String), Vec<&CommitFacts>> = BTreeMap::new();
    let mut unkeyed: Vec<&CommitFacts> = Vec::new();

    for commit in commits {
        match structural_key(commit) {
            Some(key) => groups.entry(key).or_default().push(commit),
            None => unkeyed.push(commit),
        }
    }

    for (key, members) in adjacency_then_titles(&mut unkeyed) {
        groups.entry(key).or_default().extend(members);
    }

    groups
        .into_iter()
        .map(|(key, members)| build(key.0, &key.1, &members))
        .collect()
}

/// Strongest structural key for one commit, if any.
fn structural_key(commit: &CommitFacts) -> Option<(ClusterBasis, String)> {
    if let Some(capability) = &commit.capability {
        return Some((ClusterBasis::OpenSpecCapability, capability.clone()));
    }
    if let Some(issue) = &commit.issue {
        return Some((ClusterBasis::LinkedIssue, issue.clone()));
    }
    if let Some(endpoint) = smallest(&commit.endpoints) {
        return Some((ClusterBasis::PublicApi, endpoint));
    }
    if let Some(community) = &commit.community {
        return Some((ClusterBasis::Community, community.clone()));
    }
    if let Some(component) = smallest(&commit.components) {
        return Some((ClusterBasis::Component, component));
    }
    smallest(&commit.neighbors).map(|item| (ClusterBasis::DependencyNeighborhood, item))
}

/// Group the structureless remainder by adjacency, then by title wording.
fn adjacency_then_titles<'a>(
    unkeyed: &mut Vec<&'a CommitFacts>,
) -> Vec<((ClusterBasis, String), Vec<&'a CommitFacts>)> {
    unkeyed.sort_by_key(|commit| commit.index);
    let mut runs: Vec<Vec<&CommitFacts>> = Vec::new();
    for commit in unkeyed.iter().copied() {
        match runs.last_mut() {
            Some(run)
                if run
                    .last()
                    .is_some_and(|prev| commit.index == prev.index.saturating_add(1)) =>
            {
                run.push(commit);
            }
            _ => runs.push(vec![commit]),
        }
    }

    let mut out = Vec::new();
    let mut singletons: Vec<&CommitFacts> = Vec::new();
    for run in runs {
        if run.len() > 1 {
            let anchor = run[0].id.clone();
            out.push(((ClusterBasis::CommitAdjacency, anchor), run));
        } else {
            singletons.extend(run);
        }
    }

    let mut by_token: BTreeMap<String, Vec<&CommitFacts>> = BTreeMap::new();
    let mut alone: Vec<&CommitFacts> = Vec::new();
    for commit in singletons {
        match significant_token(&commit.title) {
            Some(token) => by_token.entry(token).or_default().push(commit),
            None => alone.push(commit),
        }
    }
    for (token, members) in by_token {
        if members.len() > 1 {
            out.push(((ClusterBasis::TitleSimilarity, token), members));
        } else {
            alone.extend(members);
        }
    }
    for commit in alone {
        out.push(((ClusterBasis::Unclustered, commit.id.clone()), vec![commit]));
    }
    out
}

fn build(basis: ClusterBasis, key: &str, members: &[&CommitFacts]) -> CapabilityCluster {
    let mut commits: Vec<String> = members.iter().map(|item| item.id.clone()).collect();
    commits.sort();
    let mut endpoints: Vec<String> = members
        .iter()
        .flat_map(|item| item.endpoints.clone())
        .collect();
    endpoints.sort();
    endpoints.dedup();
    let mut components: Vec<String> = members
        .iter()
        .flat_map(|item| item.components.clone())
        .collect();
    components.sort();
    components.dedup();
    let mut title_hints: Vec<String> = members.iter().map(|item| item.title.clone()).collect();
    title_hints.sort();
    title_hints.dedup();
    let capability = members.iter().find_map(|item| item.capability.clone());
    CapabilityCluster {
        id: format!("{}:{key}", basis.as_str()),
        basis,
        commits,
        capability,
        endpoints,
        components,
        title_hints,
    }
}

fn smallest(values: &[String]) -> Option<String> {
    values.iter().min().cloned()
}

/// First meaningful token of a title, lowercased. `None` when only noise remains.
fn significant_token(title: &str) -> Option<String> {
    title
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .map(str::to_ascii_lowercase)
        .find(|word| word.len() >= 4 && !NOISE.contains(&word.as_str()))
}
