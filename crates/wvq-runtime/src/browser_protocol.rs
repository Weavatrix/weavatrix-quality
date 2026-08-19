//! Line-oriented protocol between Rust and the thin Playwright bridge.
//!
//! Methods: `initialize`, `prepare`, `execute_step`, `observe`, `finish`, `cancel`.
//! Unknown methods fail closed. The bridge contains no AI.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::program::{Observation, ProgramError, TestProgram};

/// One request from the host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BridgeRequest {
    /// Handshake. Declares protocol schema.
    Initialize {
        /// JSON-RPC-like id.
        id: u64,
        /// Must be `1`.
        schema_v: u32,
    },
    /// Load a validated program.
    Prepare {
        /// Request id.
        id: u64,
        /// Program document.
        program: TestProgram,
    },
    /// Run one IR step.
    ExecuteStep {
        /// Request id.
        id: u64,
        /// 0-based step index.
        index: u32,
    },
    /// Structured observation (policy applied by the host).
    Observe {
        /// Request id.
        id: u64,
        /// Whether the last assertion failed.
        failed: bool,
    },
    /// End the run.
    Finish {
        /// Request id.
        id: u64,
    },
    /// Cooperative cancel.
    Cancel {
        /// Request id.
        id: u64,
    },
}

/// One reply to the host.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BridgeReply {
    /// Successful result.
    Ok {
        /// Request id.
        id: u64,
        /// Optional structured body.
        #[serde(default)]
        body: Value,
    },
    /// Tool/protocol error (not an LLM decision).
    Error {
        /// Request id.
        id: u64,
        /// Stable error token.
        error: String,
    },
}

/// Decode one newline-delimited request. Unknown methods fail closed.
///
/// # Errors
///
/// Malformed JSON, missing `method`, or an unknown method name.
pub fn decode_request(line: &str) -> Result<BridgeRequest, ProgramError> {
    let value: Value =
        serde_json::from_str(line).map_err(|err| ProgramError::Malformed(err.to_string()))?;
    let method = value
        .get("method")
        .and_then(Value::as_str)
        .ok_or_else(|| ProgramError::Malformed("missing method".into()))?;
    let id = value
        .get("id")
        .and_then(Value::as_u64)
        .ok_or_else(|| ProgramError::Malformed("missing id".into()))?;
    let params = value.get("params").cloned().unwrap_or(Value::Null);
    match method {
        "initialize" => {
            let schema_v = params.get("schema_v").and_then(Value::as_u64).unwrap_or(1);
            let schema_v =
                u32::try_from(schema_v).map_err(|_| ProgramError::UnknownSchema(u32::MAX))?;
            if schema_v != 1 {
                return Err(ProgramError::UnknownSchema(schema_v));
            }
            Ok(BridgeRequest::Initialize { id, schema_v })
        }
        "prepare" => {
            let program = params
                .get("program")
                .ok_or_else(|| ProgramError::Malformed("prepare requires params.program".into()))?;
            let program = TestProgram::from_json(&program.to_string())?;
            Ok(BridgeRequest::Prepare { id, program })
        }
        "execute_step" => {
            let index = params.get("index").and_then(Value::as_u64).ok_or_else(|| {
                ProgramError::Malformed("execute_step requires params.index".into())
            })?;
            Ok(BridgeRequest::ExecuteStep {
                id,
                index: u32::try_from(index)
                    .map_err(|_| ProgramError::Invalid("execute_step index out of range".into()))?,
            })
        }
        "observe" => {
            let failed = params
                .get("failed")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            Ok(BridgeRequest::Observe { id, failed })
        }
        "finish" => Ok(BridgeRequest::Finish { id }),
        "cancel" => Ok(BridgeRequest::Cancel { id }),
        other => Err(ProgramError::Invalid(format!(
            "unknown bridge method `{other}`"
        ))),
    }
}

/// Encode a reply as one JSON line.
///
/// # Errors
///
/// Returns [`ProgramError::Malformed`] if the reply cannot be serialized.
pub fn encode_reply(reply: &BridgeReply) -> Result<String, ProgramError> {
    serde_json::to_string(reply).map_err(|err| ProgramError::Malformed(err.to_string()))
}

/// Observation body for `observe` replies.
#[must_use]
pub fn observe_body(observation: &Observation) -> Value {
    serde_json::to_value(observation).unwrap_or(Value::Null)
}
