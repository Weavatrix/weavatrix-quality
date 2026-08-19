//! Task 1: typed quality contracts must round-trip and reject empty identity.

use wvq_domain::{
    ArtifactId, ChangeId, CheckId, ContentHash, FindingState, ObligationId, ProgramId, ProofId,
    QualityFinding, RequirementId, RunId, ScenarioId, Severity, SubjectRef,
};

#[test]
fn requirement_id_round_trips() {
    let id = wvq_domain::RequirementId::new("sankey.visual-limit").unwrap();
    let json = serde_json::to_string(&id).unwrap();
    let back: wvq_domain::RequirementId = serde_json::from_str(&json).unwrap();
    assert_eq!(id, back);
}

#[test]
fn typed_ids_serialize_as_bare_strings() {
    let id = RequirementId::new("sankey.visual-limit").unwrap();
    let json = serde_json::to_string(&id).unwrap();
    assert_eq!(json, "\"sankey.visual-limit\"");
}

#[test]
fn empty_requirement_id_is_rejected() {
    assert!(RequirementId::new("").is_err());
    assert!(RequirementId::new("   ").is_err());
    assert!(serde_json::from_str::<RequirementId>("\"\"").is_err());
}

#[test]
fn each_task1_id_round_trips() {
    assert_id_roundtrip(&ChangeId::new("sankey-others").unwrap());
    assert_id_roundtrip(&RequirementId::new("sankey.visual-limit").unwrap());
    assert_id_roundtrip(&ScenarioId::new("overflow-grouped").unwrap());
    assert_id_roundtrip(&ObligationId::new("others-visible").unwrap());
    assert_id_roundtrip(&ProgramId::new("tp-others-admin").unwrap());
    assert_id_roundtrip(&RunId::new("run-001").unwrap());
    assert_id_roundtrip(&ProofId::new("proof-r18-s3").unwrap());
    assert_id_roundtrip(&ArtifactId::new("art-screenshot-1").unwrap());
    assert_id_roundtrip(&CheckId::new("WVQ-DEAD-001").unwrap());
    assert_id_roundtrip(&ContentHash::new("0123456789abcdef").unwrap());
}

#[test]
fn content_hash_rejects_non_hex() {
    assert!(ContentHash::new("not-a-hash").is_err());
    assert!(ContentHash::new("abc").is_err());
    assert!(ContentHash::new("").is_err());
}

#[test]
fn quality_finding_round_trips() {
    let finding = QualityFinding {
        check: CheckId::new("WVQ-ARCH-001").unwrap(),
        severity: Severity::Error,
        state: FindingState::New,
        subject: SubjectRef::File("ui/sankey.tsx".into()),
        summary: "new blocking architecture fingerprint".into(),
        weavatrix_fingerprint: Some("aabbccddeeff0011".into()),
    };
    let json = serde_json::to_string(&finding).unwrap();
    let back: QualityFinding = serde_json::from_str(&json).unwrap();
    assert_eq!(finding, back);
}

#[test]
fn finding_enums_use_snake_case() {
    assert_eq!(
        serde_json::to_string(&Severity::Warn).unwrap(),
        "\"warn\""
    );
    assert_eq!(
        serde_json::to_string(&FindingState::ApproachingBudget).unwrap(),
        "\"approaching_budget\""
    );
}

fn assert_id_roundtrip<T>(id: &T)
where
    T: serde::Serialize + serde::de::DeserializeOwned + PartialEq + std::fmt::Debug,
{
    let json = serde_json::to_string(id).unwrap();
    let back = serde_json::from_str::<T>(&json).unwrap();
    assert_eq!(id, &back);
}
