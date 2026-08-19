//! Non-empty typed identifiers and content hashes.

use serde::{Deserialize, Deserializer, Serialize};
use std::fmt::{self, Display};
use std::str::FromStr;
use thiserror::Error;

/// Why a typed identity was rejected.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum IdError {
    /// Empty or whitespace-only identity.
    #[error("identity must be a non-empty string")]
    Empty,
    /// Content hash must be even-length lowercase hexadecimal.
    #[error("content hash must be even-length lowercase hexadecimal")]
    InvalidHash,
}

macro_rules! typed_id {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            /// Parse a non-empty identity. Whitespace is rejected.
            ///
            /// # Errors
            ///
            /// Returns [`IdError::Empty`] when `raw` is empty or contains whitespace.
            pub fn new(raw: impl AsRef<str>) -> Result<Self, IdError> {
                Ok(Self(parse_nonempty(raw.as_ref())?))
            }

            /// Borrow the canonical string.
            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(self.as_str())
            }
        }

        impl FromStr for $name {
            type Err = IdError;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                Self::new(s)
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                let raw = String::deserialize(deserializer)?;
                Self::new(raw).map_err(serde::de::Error::custom)
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                self.as_str()
            }
        }
    };
}

typed_id!(
    /// `OpenSpec` / quality change identity (`sankey-others`).
    ChangeId
);
typed_id!(
    /// Requirement identity (`sankey.visual-limit`).
    RequirementId
);
typed_id!(
    /// Scenario identity (`overflow-grouped`).
    ScenarioId
);
typed_id!(
    /// Sealed test-obligation identity (`others-visible`).
    ObligationId
);
typed_id!(
    /// Versioned `TestProgram` identity.
    ProgramId
);
typed_id!(
    /// One execution of a program or registered suite.
    RunId
);
typed_id!(
    /// Immutable revision-bound proof identity.
    ProofId
);
typed_id!(
    /// Content-addressed artifact handle.
    ArtifactId
);
typed_id!(
    /// Stable quality-check identity (`WVQ-DEAD-001`).
    CheckId
);
typed_id!(
    /// Identity of a sealed oracle (`oseal-<digest-prefix>`).
    OracleSealId
);
typed_id!(
    /// Weavatrix snapshot / Git revision identity. Opaque; not re-hashed by WVQ.
    RevisionId
);
typed_id!(
    /// One recorded human verification decision.
    HumanDecisionId
);

/// Digest of canonical bytes. Stored as lowercase even-length hex.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub struct ContentHash(String);

impl ContentHash {
    /// Parse a lowercase hexadecimal digest of even length.
    ///
    /// # Errors
    ///
    /// Returns [`IdError::Empty`] when `raw` is empty or contains whitespace, or
    /// [`IdError::InvalidHash`] when it is not even-length lowercase hex.
    pub fn new(raw: impl AsRef<str>) -> Result<Self, IdError> {
        Ok(Self(parse_hex(raw.as_ref())?))
    }

    /// Borrow the hex digest.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Display for ContentHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for ContentHash {
    type Err = IdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::new(s)
    }
}

impl<'de> Deserialize<'de> for ContentHash {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        Self::new(raw).map_err(serde::de::Error::custom)
    }
}

impl AsRef<str> for ContentHash {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

fn parse_nonempty(raw: &str) -> Result<String, IdError> {
    if raw.is_empty() || raw.chars().any(char::is_whitespace) {
        return Err(IdError::Empty);
    }
    Ok(raw.to_owned())
}

fn parse_hex(raw: &str) -> Result<String, IdError> {
    let body = parse_nonempty(raw)?;
    if body.len() % 2 != 0 || !body.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')) {
        return Err(IdError::InvalidHash);
    }
    Ok(body)
}
