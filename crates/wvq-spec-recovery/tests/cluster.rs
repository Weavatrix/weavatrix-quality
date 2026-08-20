//! Task 25: commits become capability changes, and structure beats wording.

use wvq_spec_recovery::{CapabilityCluster, ClusterBasis, CommitFacts, cluster};

fn commit(id: &str, index: u32, title: &str) -> CommitFacts {
    CommitFacts {
        id: id.into(),
        title: title.into(),
        index,
        ..CommitFacts::default()
    }
}

fn find<'a>(clusters: &'a [CapabilityCluster], commit_id: &str) -> &'a CapabilityCluster {
    clusters
        .iter()
        .find(|item| item.commits.iter().any(|id| id == commit_id))
        .expect("every commit lands in exactly one cluster")
}

#[test]
fn one_commit_is_not_one_requirement() {
    let commits = vec![
        CommitFacts {
            capability: Some("sankey".into()),
            ..commit("c1", 0, "add others endpoint")
        },
        CommitFacts {
            capability: Some("sankey".into()),
            ..commit("c2", 1, "group overflow values")
        },
        CommitFacts {
            capability: Some("sankey".into()),
            ..commit("c3", 2, "render Others node")
        },
    ];
    let clusters = cluster(&commits);
    assert_eq!(clusters.len(), 1, "three commits, one capability change");
    assert_eq!(clusters[0].basis, ClusterBasis::OpenSpecCapability);
    assert_eq!(clusters[0].capability.as_deref(), Some("sankey"));
    assert_eq!(clusters[0].commits, vec!["c1", "c2", "c3"]);
}

#[test]
fn structural_evidence_beats_title_similarity() {
    // Near-identical wording, different capabilities: these must not merge.
    let commits = vec![
        CommitFacts {
            capability: Some("sankey".into()),
            ..commit("c1", 0, "fix visual limit")
        },
        CommitFacts {
            capability: Some("auth".into()),
            ..commit("c2", 1, "fix visual limit")
        },
    ];
    let clusters = cluster(&commits);
    assert_eq!(clusters.len(), 2, "same words, different capabilities");
    assert!(
        clusters
            .iter()
            .all(|item| item.basis == ClusterBasis::OpenSpecCapability)
    );
    assert_ne!(find(&clusters, "c1").id, find(&clusters, "c2").id);
}

#[test]
fn unrelated_titles_merge_on_a_shared_endpoint() {
    let commits = vec![
        CommitFacts {
            endpoints: vec!["GET /api/sankey/others".into()],
            ..commit("c1", 0, "wire handler")
        },
        CommitFacts {
            endpoints: vec!["GET /api/sankey/others".into()],
            ..commit("c2", 1, "paginate response")
        },
    ];
    let clusters = cluster(&commits);
    assert_eq!(clusters.len(), 1);
    assert_eq!(clusters[0].basis, ClusterBasis::PublicApi);
    assert!(clusters[0].basis.is_structural());
    assert_eq!(clusters[0].endpoints, vec!["GET /api/sankey/others"]);
}

#[test]
fn the_priority_order_is_respected() {
    let capability_wins = CommitFacts {
        capability: Some("sankey".into()),
        issue: Some("WVQ-17".into()),
        endpoints: vec!["GET /x".into()],
        community: Some("analytics".into()),
        ..commit("c1", 0, "everything at once")
    };
    assert_eq!(
        cluster(&[capability_wins])[0].basis,
        ClusterBasis::OpenSpecCapability
    );

    let issue_wins = CommitFacts {
        issue: Some("WVQ-17".into()),
        endpoints: vec!["GET /x".into()],
        community: Some("analytics".into()),
        ..commit("c2", 0, "no capability")
    };
    assert_eq!(cluster(&[issue_wins])[0].basis, ClusterBasis::LinkedIssue);

    let community_wins = CommitFacts {
        community: Some("analytics".into()),
        components: vec!["Sankey".into()],
        ..commit("c3", 0, "no api")
    };
    assert_eq!(cluster(&[community_wins])[0].basis, ClusterBasis::Community);

    let neighborhood_wins = CommitFacts {
        neighbors: vec!["src/charts".into()],
        ..commit("c4", 0, "only a neighbourhood")
    };
    assert_eq!(
        cluster(&[neighborhood_wins])[0].basis,
        ClusterBasis::DependencyNeighborhood
    );
}

#[test]
fn adjacent_structureless_commits_group_before_wording_is_consulted() {
    let commits = vec![
        commit("c1", 0, "tidy sankey helpers"),
        commit("c2", 1, "drop unused import"),
    ];
    let clusters = cluster(&commits);
    assert_eq!(clusters.len(), 1);
    assert_eq!(clusters[0].basis, ClusterBasis::CommitAdjacency);
    assert!(!clusters[0].basis.is_structural());
    assert_eq!(clusters[0].commits, vec!["c1", "c2"]);
}

#[test]
fn title_similarity_is_the_last_deterministic_resort() {
    // Non-adjacent, structureless, but the wording matches.
    let commits = vec![
        commit("c1", 0, "sankey tooltip copy"),
        CommitFacts {
            capability: Some("auth".into()),
            ..commit("c2", 1, "unrelated")
        },
        commit("c3", 2, "sankey legend spacing"),
    ];
    let clusters = cluster(&commits);
    let grouped = find(&clusters, "c1");
    assert_eq!(grouped.basis, ClusterBasis::TitleSimilarity);
    assert_eq!(grouped.commits, vec!["c1", "c3"]);
    assert_eq!(
        find(&clusters, "c2").basis,
        ClusterBasis::OpenSpecCapability
    );
}

#[test]
fn a_commit_with_nothing_to_group_on_stays_unclustered() {
    let clusters = cluster(&[commit("c1", 0, "wip")]);
    assert_eq!(clusters.len(), 1);
    assert_eq!(
        clusters[0].basis,
        ClusterBasis::Unclustered,
        "noise-only wording must not invent a grouping"
    );
    assert_eq!(clusters[0].commits, vec!["c1"]);
}

#[test]
fn commit_provenance_and_hints_survive_clustering() {
    let commits = vec![
        CommitFacts {
            capability: Some("sankey".into()),
            components: vec!["Sankey".into()],
            endpoints: vec!["GET /api/sankey/others".into()],
            ..commit("c2", 1, "group overflow values")
        },
        CommitFacts {
            capability: Some("sankey".into()),
            components: vec!["Sankey".into(), "Legend".into()],
            ..commit("c1", 0, "add others endpoint")
        },
    ];
    let clusters = cluster(&commits);
    assert_eq!(clusters[0].commits, vec!["c1", "c2"], "sorted provenance");
    assert_eq!(clusters[0].components, vec!["Legend", "Sankey"]);
    assert_eq!(
        clusters[0].title_hints,
        vec!["add others endpoint", "group overflow values"],
        "titles stay hints attached to the cluster"
    );
}
