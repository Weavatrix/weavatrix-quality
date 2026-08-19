//! `JUnit` XML from Vitest / Jest / Bun.

use quick_xml::Reader;
use quick_xml::events::{BytesStart, Event};

use crate::normalize::{
    ArtifactDescriptor, NormalizedTestRun, RuntimeError, TestCaseResult, TestStatus, seconds_to_ms,
};

/// Parse a `JUnit` document into a normalized run.
///
/// # Errors
///
/// Returns [`RuntimeError::Malformed`] or [`RuntimeError::Truncated`] when the
/// XML is incomplete or a `testcase` has no name.
pub fn parse_junit(xml: &str) -> Result<NormalizedTestRun, RuntimeError> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut suite_stack = Vec::<String>::new();
    let mut cases = Vec::new();
    let mut current: Option<OpenCase> = None;
    let mut buf = Vec::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(tag)) => {
                start_tag(&tag, &mut suite_stack, &mut current)?;
            }
            Ok(Event::Empty(tag)) => {
                match local_name(&tag).as_str() {
                    "testcase" => {
                        let open = open_case(&tag, &suite_stack)?;
                        cases.push(open.into_result());
                    }
                    "failure" => apply_status(&mut current, TestStatus::Fail, attr(&tag, "message")),
                    "error" => apply_status(&mut current, TestStatus::Error, attr(&tag, "message")),
                    "skipped" => apply_status(&mut current, TestStatus::Skip, attr(&tag, "message")),
                    "testsuite" => {
                        suite_stack.push(attr(&tag, "name").unwrap_or_default());
                        suite_stack.pop();
                    }
                    _ => {}
                }
            }
            Ok(Event::End(tag)) => match local_name_end(&tag).as_str() {
                "testsuite" => {
                    suite_stack.pop();
                }
                "testcase" => {
                    if let Some(open) = current.take() {
                        cases.push(open.into_result());
                    }
                }
                _ => {}
            },
            Ok(Event::Text(text)) => {
                if let Some(open) = current.as_mut()
                    && matches!(open.status, TestStatus::Fail | TestStatus::Error)
                {
                    let body = String::from_utf8_lossy(text.as_ref());
                    if !body.trim().is_empty() {
                        open.message = Some(match open.message.take() {
                            Some(existing) => format!("{existing}\n{body}"),
                            None => body.into_owned(),
                        });
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => {
                return Err(RuntimeError::Truncated {
                    kind: "junit".into(),
                });
            }
            _ => {}
        }
        buf.clear();
    }
    if current.is_some() {
        return Err(RuntimeError::Truncated {
            kind: "junit".into(),
        });
    }
    Ok(NormalizedTestRun {
        cases,
        coverage: None,
        raw_artifacts: vec![ArtifactDescriptor {
            kind: "junit".into(),
            path: None,
        }],
    })
}

fn start_tag(
    tag: &BytesStart<'_>,
    suite_stack: &mut Vec<String>,
    current: &mut Option<OpenCase>,
) -> Result<(), RuntimeError> {
    match local_name(tag).as_str() {
        "testsuite" => {
            suite_stack.push(attr(tag, "name").unwrap_or_default());
        }
        "testcase" => {
            *current = Some(open_case(tag, suite_stack)?);
        }
        "failure" => apply_status(current, TestStatus::Fail, attr(tag, "message")),
        "error" => apply_status(current, TestStatus::Error, attr(tag, "message")),
        "skipped" => apply_status(current, TestStatus::Skip, attr(tag, "message")),
        _ => {}
    }
    Ok(())
}

fn open_case(tag: &BytesStart<'_>, suite_stack: &[String]) -> Result<OpenCase, RuntimeError> {
    let name = attr(tag, "name").ok_or_else(|| RuntimeError::Malformed {
        kind: "junit".into(),
        message: "testcase missing name".into(),
    })?;
    Ok(OpenCase {
        name,
        suite: attr(tag, "classname")
            .or_else(|| suite_stack.last().cloned())
            .unwrap_or_default(),
        status: TestStatus::Pass,
        duration_ms: attr(tag, "time").as_deref().and_then(seconds_to_ms),
        message: None,
    })
}

struct OpenCase {
    name: String,
    suite: String,
    status: TestStatus,
    duration_ms: Option<u64>,
    message: Option<String>,
}

impl OpenCase {
    fn into_result(self) -> TestCaseResult {
        TestCaseResult {
            name: self.name,
            suite: self.suite,
            status: self.status,
            duration_ms: self.duration_ms,
            message: self.message,
        }
    }
}

fn apply_status(current: &mut Option<OpenCase>, status: TestStatus, message: Option<String>) {
    if let Some(open) = current {
        open.status = status;
        if open.message.is_none() {
            open.message = message;
        }
    }
}

fn local_name(tag: &BytesStart<'_>) -> String {
    String::from_utf8_lossy(tag.local_name().as_ref()).into_owned()
}

fn local_name_end(tag: &quick_xml::events::BytesEnd<'_>) -> String {
    String::from_utf8_lossy(tag.local_name().as_ref()).into_owned()
}

fn attr(tag: &BytesStart<'_>, key: &str) -> Option<String> {
    tag.attributes()
        .with_checks(false)
        .filter_map(Result::ok)
        .find(|item| item.key.as_ref() == key.as_bytes())
        .and_then(|item| String::from_utf8(item.value.into_owned()).ok())
}
