//! `go test -json` NDJSON.

use std::collections::BTreeMap;

use serde::Deserialize;
use serde_json::Value;

use crate::normalize::{
    ArtifactDescriptor, NormalizedTestRun, RuntimeError, TestCaseResult, TestStatus, seconds_to_ms,
};

#[derive(Debug, Deserialize)]
struct GoEvent {
    #[serde(rename = "Action")]
    action: String,
    #[serde(rename = "Package")]
    package: Option<String>,
    #[serde(rename = "Test")]
    test: Option<String>,
    #[serde(rename = "Elapsed")]
    elapsed: Option<Value>,
    #[serde(rename = "Output")]
    output: Option<String>,
}

#[derive(Clone)]
struct OpenGo {
    status: Option<TestStatus>,
    duration_ms: Option<u64>,
    message: String,
}

/// Parse `go test -json` output.
///
/// # Errors
///
/// Truncated last line, missing `Action`, or a test that started but never
/// finished all fail closed.
pub fn parse_go_json(jsonl: &str) -> Result<NormalizedTestRun, RuntimeError> {
    let mut open = BTreeMap::<(String, String), OpenGo>::new();
    let mut finished = Vec::new();
    let lines: Vec<&str> = jsonl.lines().collect();
    for (index, raw) in lines.iter().enumerate() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        let event: GoEvent = match serde_json::from_str(line) {
            Ok(event) => event,
            Err(_) if index + 1 == lines.len() => {
                return Err(RuntimeError::Truncated {
                    kind: "go-json".into(),
                });
            }
            Err(err) => {
                return Err(RuntimeError::Malformed {
                    kind: "go-json".into(),
                    message: err.to_string(),
                });
            }
        };
        if event.action.is_empty() {
            return Err(RuntimeError::Malformed {
                kind: "go-json".into(),
                message: "event missing Action".into(),
            });
        }
        let Some(test) = event.test.clone() else {
            continue;
        };
        let suite = event.package.clone().unwrap_or_default();
        let key = (suite.clone(), test.clone());
        let finished_case = {
            let slot = open.entry(key.clone()).or_insert_with(|| OpenGo {
                status: None,
                duration_ms: None,
                message: String::new(),
            });
            match event.action.as_str() {
                "output" => {
                    if let Some(output) = event.output {
                        slot.message.push_str(&output);
                    }
                }
                "pass" => finish(slot, TestStatus::Pass, event.elapsed.as_ref()),
                "fail" => finish(slot, TestStatus::Fail, event.elapsed.as_ref()),
                "skip" => finish(slot, TestStatus::Skip, event.elapsed.as_ref()),
                "run" | "pause" | "cont" | "start" | "bench" => {}
                other => {
                    return Err(RuntimeError::Malformed {
                        kind: "go-json".into(),
                        message: format!("unknown Action `{other}`"),
                    });
                }
            }
            slot.status.map(|status| {
                (
                    status,
                    slot.duration_ms,
                    std::mem::take(&mut slot.message),
                )
            })
        };
        if let Some((status, duration_ms, message)) = finished_case {
            open.remove(&key);
            finished.push(TestCaseResult {
                name: test,
                suite,
                status,
                duration_ms,
                message: (!message.is_empty()).then_some(message),
            });
        }
    }
    if open.values().any(|item| item.status.is_none()) {
        return Err(RuntimeError::Truncated {
            kind: "go-json".into(),
        });
    }
    Ok(NormalizedTestRun {
        cases: finished,
        coverage: None,
        raw_artifacts: vec![ArtifactDescriptor {
            kind: "go-json".into(),
            path: None,
        }],
    })
}

fn finish(slot: &mut OpenGo, status: TestStatus, elapsed: Option<&Value>) {
    slot.status = Some(status);
    slot.duration_ms = elapsed.and_then(|value| match value {
        Value::Number(number) => seconds_to_ms(&number.to_string()),
        Value::String(raw) => seconds_to_ms(raw),
        _ => None,
    });
}
