//! Closed-set command token validation.

use wvq_proof::AiCallKind;

use super::BusError;

pub(in crate::service) fn validate_purpose(purpose: &str) -> Result<(), BusError> {
    match purpose {
        "spec" | "implementation" | "review" => Ok(()),
        other => Err(BusError::Unknown {
            field: "purpose",
            value: other.to_owned(),
        }),
    }
}

pub(in crate::service) fn validate_scope(scope: &str) -> Result<(), BusError> {
    match scope {
        "impacted" | "all" => Ok(()),
        other => Err(BusError::Unknown {
            field: "scope",
            value: other.to_owned(),
        }),
    }
}

pub(in crate::service) fn validate_evidence_policy(policy: &str) -> Result<(), BusError> {
    match policy {
        "standard" | "minimal" | "none" => Ok(()),
        other => Err(BusError::Unknown {
            field: "evidence_policy",
            value: other.to_owned(),
        }),
    }
}

pub(in crate::service) fn validate_revision_ref(field: &'static str, reference: &str) -> Result<(), BusError> {
    if reference.is_empty()
        || reference.len() > 512
        || reference.starts_with('-')
        || reference
            .bytes()
            .any(|byte| matches!(byte, 0 | b'\n' | b'\r'))
    {
        return Err(BusError::Unknown {
            field,
            value: reference.to_owned(),
        });
    }
    if field == "base" && reference == "WORKTREE" {
        return Err(BusError::Unknown {
            field,
            value: reference.to_owned(),
        });
    }
    Ok(())
}

pub(in crate::service) fn parse_model_kind(kind: &str) -> Result<AiCallKind, BusError> {
    match kind {
        "planning" => Ok(AiCallKind::Planning),
        "runtime" => Ok(AiCallKind::Runtime),
        "browser_escape" => Ok(AiCallKind::BrowserEscape),
        "vision" => Ok(AiCallKind::Vision),
        other => Err(BusError::Unknown {
            field: "model kind",
            value: other.to_owned(),
        }),
    }
}
