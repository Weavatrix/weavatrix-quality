//! Path identity helpers. Test vs source vs story must stay honest.

use std::fmt::Write as _;
use std::path::Path;

use sha2::{Digest, Sha256};

use crate::replies::{EvidenceReply, INLINE_LIMIT};

pub(in crate::service) fn test_path_from_node_id(id: &str) -> Option<String> {
    let raw = id
        .strip_prefix("file:")
        .or_else(|| id.strip_prefix("symbol:"))
        .unwrap_or(id);
    let path = raw.split('#').next().unwrap_or(raw);
    is_test_path(path).then(|| normalize_path(path))
}

pub(in crate::service) fn is_test_path(path: &str) -> bool {
    let path = normalize_path(path).to_ascii_lowercase();
    let file = path.rsplit('/').next().unwrap_or(&path);
    path.contains("/tests/")
        || path.starts_with("tests/")
        || path.contains("/__tests__/")
        || file.ends_with("_test.go")
        || file.starts_with("test_")
        || file.contains(".test.")
        || file.contains(".spec.")
        || file.contains(".stories.")
}

pub(in crate::service) fn is_story_path(path: &str) -> bool {
    normalize_path(path)
        .rsplit('/')
        .next()
        .unwrap_or(path)
        .to_ascii_lowercase()
        .contains(".stories.")
}

pub(in crate::service) fn normalize_path(path: &str) -> String {
    path.replace('\\', "/")
}
pub(in crate::service) fn safe_file_token(value: &str) -> String {
    let token = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
                ch
            } else {
                '-'
            }
        })
        .take(100)
        .collect::<String>();
    if token.is_empty() {
        "program".into()
    } else {
        token
    }
}
pub(in crate::service) fn relative_or_display(repo: &Path, path: &Path) -> String {
    path.strip_prefix(repo).ok().map_or_else(
        || path.display().to_string(),
        |relative| {
            if relative.as_os_str().is_empty() {
                ".".into()
            } else {
                relative.display().to_string()
            }
        },
    )
}

pub(in crate::service) fn evidence_from_bytes(handle: &str, bytes: &[u8]) -> EvidenceReply {
    let hash = sha256_hex(bytes);
    let inline_text = if bytes.len() <= INLINE_LIMIT {
        std::str::from_utf8(bytes).ok().map(ToOwned::to_owned)
    } else {
        None
    };
    EvidenceReply {
        handle: handle.to_owned(),
        kind: "bytes".into(),
        byte_len: bytes.len() as u64,
        content_hash: Some(hash),
        inline_text,
    }
}

pub(in crate::service) fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .fold(String::new(), |mut out, byte| {
            let _ = write!(out, "{byte:02x}");
            out
        })
}
