//! Git log evidence for spec recovery. Fail closed on malformed commit ids.

use std::path::Path;

use wvq_spec_recovery::CommitFacts;

use super::super::{BusError, RevisionRange, git_output};

pub(super) struct RecoveryLogRecord {
    pub(super) id: String,
    pub(super) title: String,
    pub(super) body: String,
}

pub(in crate::service) fn recovery_log(
    repo: &Path,
    range: &RevisionRange,
) -> Result<Vec<RecoveryLogRecord>, BusError> {
    let revset = format!("{}..{}", range.merge_base, range.head_commit);
    let raw = git_output(
        repo,
        &[
            "log".into(),
            "--reverse".into(),
            "--format=%H%x1f%s%x1f%b%x1e".into(),
            revset,
            "--".into(),
        ],
    )?;
    let raw = String::from_utf8(raw)
        .map_err(|err| BusError::Intelligence(format!("Git log is not UTF-8: {err}")))?;
    let mut records = Vec::new();
    for record in raw.split('\u{1e}') {
        let record = record.trim_matches(['\r', '\n']);
        if record.is_empty() {
            continue;
        }
        let mut fields = record.splitn(3, '\u{1f}');
        let id = fields.next().unwrap_or_default().trim().to_owned();
        let title = fields.next().unwrap_or_default().trim().to_owned();
        let body = fields.next().unwrap_or_default().trim().to_owned();
        if id.len() != 40 || !id.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(BusError::Intelligence(format!(
                "Git log returned an invalid commit id `{id}`"
            )));
        }
        records.push(RecoveryLogRecord { id, title, body });
    }
    Ok(records)
}

pub(in crate::service) fn recovery_commits(
    repo: &Path,
    range: &RevisionRange,
    head_revision: &str,
    components: &[String],
    has_file_delta: bool,
) -> Result<Vec<CommitFacts>, BusError> {
    let mut facts = recovery_log(repo, range)?
        .into_iter()
        .enumerate()
        .map(|(index, record)| CommitFacts {
            id: record.id,
            title: record.title,
            index: u32::try_from(index).unwrap_or(u32::MAX),
            issue: linked_issue(&record.body),
            ..CommitFacts::default()
        })
        .collect::<Vec<_>>();
    if range.head_ref == "WORKTREE" && has_file_delta {
        facts.push(CommitFacts {
            id: head_revision.to_owned(),
            title: "working tree change".into(),
            index: u32::try_from(facts.len()).unwrap_or(u32::MAX),
            components: components.to_vec(),
            ..CommitFacts::default()
        });
    }
    Ok(facts)
}

fn linked_issue(text: &str) -> Option<String> {
    text.split(|character: char| !(character.is_ascii_alphanumeric() || character == '-'))
        .find(|token| {
            let Some((prefix, number)) = token.rsplit_once('-') else {
                return false;
            };
            !prefix.is_empty()
                && prefix
                    .chars()
                    .all(|character| character.is_ascii_uppercase())
                && !number.is_empty()
                && number.chars().all(|character| character.is_ascii_digit())
        })
        .map(ToOwned::to_owned)
}
