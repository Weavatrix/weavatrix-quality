//! Versioned, fail-closed UI-integrity policy.
//!
//! Everything here exists to stop a true positive being silenced by accident.
//! Unknown fields, malformed matchers, out-of-range ratios, path escapes, and
//! exceptions without a reason are refused rather than ignored, and there is
//! deliberately no `accept_all`: an allowance must name what it allows.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use serde_yaml::{Mapping, Value};

use crate::UiError;
use crate::snapshot::UiNode;

/// Default ceiling on collected nodes per state.
pub const DEFAULT_MAX_NODES: u32 = 5_000;
/// Default geometry tolerance, in whole pixels.
pub const DEFAULT_GEOMETRY_TOLERANCE_PX: u32 = 1;
/// Default share of hit-test points a control may lose before it is occluded.
pub const DEFAULT_OCCLUSION_FAILURE_PERMILLE: u16 = 500;

/// Which node an allowance or exception applies to.
///
/// Every populated field must match. An empty matcher matches nothing: an
/// allowance that names no target would silence the whole detector.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NodeMatcher {
    /// ARIA role.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    /// Accessible name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accessible_name: Option<String>,
    /// Stable test id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub test_id: Option<String>,
    /// Framework component name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub component_hint: Option<String>,
    /// `id` attribute.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dom_id: Option<String>,
}

impl NodeMatcher {
    /// Whether every populated field matches `node`.
    #[must_use]
    pub fn matches(&self, node: &UiNode) -> bool {
        if self.is_empty() {
            return false;
        }
        equal_opt(self.role.as_deref(), node.role.as_deref())
            && equal_opt(
                self.accessible_name.as_deref(),
                node.accessible_name.as_deref(),
            )
            && equal_opt(self.test_id.as_deref(), node.test_id.as_deref())
            && equal_opt(
                self.component_hint.as_deref(),
                node.component_hint.as_deref(),
            )
            && equal_opt(self.dom_id.as_deref(), node.dom_id.as_deref())
    }

    fn is_empty(&self) -> bool {
        self.role.is_none()
            && self.accessible_name.is_none()
            && self.test_id.is_none()
            && self.component_hint.is_none()
            && self.dom_id.is_none()
    }
}

fn equal_opt(wanted: Option<&str>, found: Option<&str>) -> bool {
    match wanted {
        None => true,
        Some(wanted) => found.is_some_and(|found| found == wanted),
    }
}

/// One overlap the project has decided is intentional.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AllowedOverlap {
    /// Node that legitimately paints on top.
    pub top: NodeMatcher,
    /// Node it legitimately paints over.
    pub bottom: NodeMatcher,
    /// Why this is intentional.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// Who accepted it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reviewer: Option<String>,
    /// ISO date after which the allowance stops applying.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires: Option<String>,
}

/// One text truncation the project has decided is intentional.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AcceptedTruncation {
    /// Node whose ellipsis is intentional.
    pub target: NodeMatcher,
    /// Whether the full value must still reach assistive technology.
    #[serde(default)]
    pub requires_accessible_full_value: bool,
    /// Why this is intentional.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// Who accepted it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reviewer: Option<String>,
    /// ISO date after which the allowance stops applying.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires: Option<String>,
}

/// One finding a human explicitly accepted, by fingerprint.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UiException {
    /// Exact finding fingerprint.
    pub fingerprint: String,
    /// Why it is accepted. Required.
    pub reason: String,
    /// Who accepted it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reviewer: Option<String>,
    /// ISO date after which the exception stops applying.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires: Option<String>,
}

/// Complete UI-integrity policy for one repository.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UiIntegrityPolicy {
    /// Whether the axis runs at all.
    pub enabled: bool,
    /// Ceiling on collected nodes per state.
    pub max_nodes: u32,
    /// Geometry noise the detectors ignore, in whole pixels.
    pub geometry_tolerance_px: u32,
    /// Share of hit-test points a control may lose, in permille.
    pub occlusion_failure_permille: u16,
    /// Intentional overlaps.
    pub allowed_overlaps: Vec<AllowedOverlap>,
    /// Intentional text truncation.
    pub accepted_text_truncation: Vec<AcceptedTruncation>,
    /// Accepted findings, by fingerprint.
    pub exceptions: Vec<UiException>,
    /// Exceptions and allowances that expired, kept so they stay visible
    /// instead of silently continuing to suppress a finding.
    pub expired: Vec<String>,
}

impl Default for UiIntegrityPolicy {
    /// Disabled, with the documented defaults. A repository that never
    /// configured the axis gets `not_applicable`, not a silent pass.
    fn default() -> Self {
        Self {
            enabled: false,
            max_nodes: DEFAULT_MAX_NODES,
            geometry_tolerance_px: DEFAULT_GEOMETRY_TOLERANCE_PX,
            occlusion_failure_permille: DEFAULT_OCCLUSION_FAILURE_PERMILLE,
            allowed_overlaps: Vec::new(),
            accepted_text_truncation: Vec::new(),
            exceptions: Vec::new(),
            expired: Vec::new(),
        }
    }
}

impl UiIntegrityPolicy {
    /// Geometry tolerance as pixels.
    #[must_use]
    pub fn tolerance(&self) -> f64 {
        f64::from(self.geometry_tolerance_px)
    }

    /// Whether `top` painting over `bottom` was explicitly accepted.
    #[must_use]
    pub fn overlap_allowed(&self, top: &UiNode, bottom: &UiNode) -> bool {
        self.allowed_overlaps
            .iter()
            .any(|rule| rule.top.matches(top) && rule.bottom.matches(bottom))
    }

    /// Whether truncation on this node was explicitly accepted.
    ///
    /// An allowance that requires the accessible full value only applies while
    /// that value is actually present: an ellipsis nobody can expand is still a
    /// finding.
    #[must_use]
    pub fn truncation_accepted(&self, node: &UiNode) -> bool {
        self.accepted_text_truncation.iter().any(|rule| {
            rule.target.matches(node)
                && (!rule.requires_accessible_full_value
                    || node
                        .accessible_name
                        .as_deref()
                        .is_some_and(|name| !name.trim().is_empty()))
        })
    }

    /// Active exception fingerprints.
    #[must_use]
    pub fn excepted(&self) -> BTreeSet<&str> {
        self.exceptions
            .iter()
            .map(|item| item.fingerprint.as_str())
            .collect()
    }
}

/// Parse the `ui_integrity` section of `.weavatrix-quality/config.yaml`.
///
/// `today` is the caller's ISO date, used to retire expired allowances. It is
/// passed in rather than read from the clock so the parse stays deterministic.
///
/// # Errors
///
/// Returns [`UiError::Policy`] for an unknown field, a non-mapping section, a
/// matcher with no fields, a ratio or tolerance out of range, an invalid or
/// missing date, an exception with no reason, or any attempt to accept
/// everything at once.
pub fn parse_policy(section: &Value, today: &str) -> Result<UiIntegrityPolicy, UiError> {
    let root = section
        .as_mapping()
        .ok_or_else(|| UiError::Policy("ui_integrity must be a mapping".into()))?;
    known_keys(
        root,
        "ui_integrity",
        &[
            "enabled",
            "max_nodes",
            "geometry_tolerance_px",
            "occlusion_failure_ratio",
            "allowed_overlaps",
            "accepted_text_truncation",
            "exceptions",
        ],
    )?;

    let mut policy = UiIntegrityPolicy {
        enabled: bool_field(root, "enabled", true)?,
        max_nodes: bounded_u32(
            root,
            "max_nodes",
            DEFAULT_MAX_NODES,
            1,
            crate::snapshot::MAX_NODES,
        )?,
        geometry_tolerance_px: bounded_u32(
            root,
            "geometry_tolerance_px",
            DEFAULT_GEOMETRY_TOLERANCE_PX,
            0,
            64,
        )?,
        occlusion_failure_permille: ratio_permille(root, "occlusion_failure_ratio")?,
        ..UiIntegrityPolicy::default()
    };

    parse_allowed_overlaps(root, today, &mut policy)?;
    parse_accepted_truncation(root, today, &mut policy)?;
    parse_exceptions(root, today, &mut policy)?;
    Ok(policy)
}

fn parse_allowed_overlaps(
    root: &Mapping,
    today: &str,
    policy: &mut UiIntegrityPolicy,
) -> Result<(), UiError> {
    for (index, item) in sequence(root, "allowed_overlaps")?.iter().enumerate() {
        let map = mapping(item, "allowed_overlaps", index)?;
        known_keys(
            map,
            "allowed_overlaps",
            &["top", "bottom", "reason", "reviewer", "expires"],
        )?;
        let rule = AllowedOverlap {
            top: matcher(map, "top", "allowed_overlaps", index)?,
            bottom: matcher(map, "bottom", "allowed_overlaps", index)?,
            reason: optional_text(map, "reason", "allowed_overlaps", index)?,
            reviewer: optional_text(map, "reviewer", "allowed_overlaps", index)?,
            expires: expiry(map, "allowed_overlaps", index)?,
        };
        if is_expired(rule.expires.as_deref(), today) {
            policy.expired.push(format!(
                "allowed_overlaps[{index}] expired {}",
                rule.expires.unwrap_or_default()
            ));
            continue;
        }
        policy.allowed_overlaps.push(rule);
    }
    Ok(())
}

fn parse_accepted_truncation(
    root: &Mapping,
    today: &str,
    policy: &mut UiIntegrityPolicy,
) -> Result<(), UiError> {
    const SECTION: &str = "accepted_text_truncation";
    for (index, item) in sequence(root, SECTION)?.iter().enumerate() {
        let map = mapping(item, SECTION, index)?;
        known_keys(
            map,
            SECTION,
            &[
                "target",
                "requires_accessible_full_value",
                "reason",
                "reviewer",
                "expires",
            ],
        )?;
        let rule = AcceptedTruncation {
            target: matcher(map, "target", SECTION, index)?,
            requires_accessible_full_value: bool_field(
                map,
                "requires_accessible_full_value",
                false,
            )?,
            reason: optional_text(map, "reason", SECTION, index)?,
            reviewer: optional_text(map, "reviewer", SECTION, index)?,
            expires: expiry(map, SECTION, index)?,
        };
        if is_expired(rule.expires.as_deref(), today) {
            policy.expired.push(format!(
                "{SECTION}[{index}] expired {}",
                rule.expires.unwrap_or_default()
            ));
            continue;
        }
        policy.accepted_text_truncation.push(rule);
    }
    Ok(())
}

fn parse_exceptions(
    root: &Mapping,
    today: &str,
    policy: &mut UiIntegrityPolicy,
) -> Result<(), UiError> {
    for (index, item) in sequence(root, "exceptions")?.iter().enumerate() {
        let map = mapping(item, "exceptions", index)?;
        known_keys(
            map,
            "exceptions",
            &["fingerprint", "reason", "reviewer", "expires"],
        )?;
        let fingerprint = required_text(map, "fingerprint", "exceptions", index)?;
        if !fingerprint.starts_with("ui:") {
            return Err(UiError::Policy(format!(
                "ui_integrity exceptions[{index}] fingerprint `{fingerprint}` is not a \
                 UI-integrity fingerprint"
            )));
        }
        let rule = UiException {
            fingerprint,
            reason: required_text(map, "reason", "exceptions", index)?,
            reviewer: optional_text(map, "reviewer", "exceptions", index)?,
            expires: expiry(map, "exceptions", index)?,
        };
        if is_expired(rule.expires.as_deref(), today) {
            policy.expired.push(format!(
                "exception {} expired {}",
                rule.fingerprint,
                rule.expires.unwrap_or_default()
            ));
            continue;
        }
        policy.exceptions.push(rule);
    }
    Ok(())
}

fn known_keys(map: &Mapping, section: &str, allowed: &[&str]) -> Result<(), UiError> {
    for key in map.keys() {
        let Some(name) = key.as_str() else {
            return Err(UiError::Policy(format!(
                "ui_integrity {section} has a non-string key"
            )));
        };
        // `accept_all` is not merely unknown; it is the shape of allowance this
        // policy refuses to have, so it gets its own message.
        if name == "accept_all" {
            return Err(UiError::Policy(format!(
                "ui_integrity {section} may not accept everything; \
                 list the exact overlaps or fingerprints instead"
            )));
        }
        if !allowed.contains(&name) {
            return Err(UiError::Policy(format!(
                "ui_integrity {section} has unknown field `{name}`"
            )));
        }
    }
    Ok(())
}

fn bool_field(map: &Mapping, key: &str, default: bool) -> Result<bool, UiError> {
    match map.get(Value::from(key)) {
        None => Ok(default),
        Some(value) => value
            .as_bool()
            .ok_or_else(|| UiError::Policy(format!("ui_integrity {key} must be true or false"))),
    }
}

fn bounded_u32(
    map: &Mapping,
    key: &str,
    default: u32,
    lowest: u32,
    highest: usize,
) -> Result<u32, UiError> {
    let highest = u32::try_from(highest).unwrap_or(u32::MAX);
    match map.get(Value::from(key)) {
        None => Ok(default),
        Some(value) => value
            .as_u64()
            .and_then(|raw| u32::try_from(raw).ok())
            .filter(|found| (lowest..=highest).contains(found))
            .ok_or_else(|| {
                UiError::Policy(format!(
                    "ui_integrity {key} must be an integer between {lowest} and {highest}"
                ))
            }),
    }
}

/// Parse a `0.0 … 1.0` ratio into permille. Rejects negatives, values above
/// one, and non-finite numbers so a typo cannot disable a gate.
fn ratio_permille(map: &Mapping, key: &str) -> Result<u16, UiError> {
    let Some(value) = map.get(Value::from(key)) else {
        return Ok(DEFAULT_OCCLUSION_FAILURE_PERMILLE);
    };
    let ratio = value
        .as_f64()
        .filter(|found| found.is_finite() && (0.0..=1.0).contains(found))
        .ok_or_else(|| {
            UiError::Policy(format!(
                "ui_integrity {key} must be a number between 0.0 and 1.0"
            ))
        })?;
    // Round to the nearest permille; 0.5 becomes exactly 500. The ratio is
    // already clamped to `0.0 ..= 1.0`, so the cast is exact.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    Ok((ratio * 1000.0).round() as u16)
}

fn sequence<'a>(map: &'a Mapping, key: &str) -> Result<&'a [Value], UiError> {
    match map.get(Value::from(key)) {
        None => Ok(&[]),
        Some(value) => value
            .as_sequence()
            .map(Vec::as_slice)
            .ok_or_else(|| UiError::Policy(format!("ui_integrity {key} must be a list"))),
    }
}

fn mapping<'a>(value: &'a Value, section: &str, index: usize) -> Result<&'a Mapping, UiError> {
    value.as_mapping().ok_or_else(|| {
        UiError::Policy(format!("ui_integrity {section}[{index}] must be a mapping"))
    })
}

fn matcher(map: &Mapping, key: &str, section: &str, index: usize) -> Result<NodeMatcher, UiError> {
    let value = map.get(Value::from(key)).ok_or_else(|| {
        UiError::Policy(format!("ui_integrity {section}[{index}] requires `{key}`"))
    })?;
    let target = mapping(value, section, index)?;
    known_keys(
        target,
        &format!("{section}[{index}].{key}"),
        &[
            "role",
            "accessible_name",
            "test_id",
            "component_hint",
            "dom_id",
        ],
    )?;
    let mut matcher = NodeMatcher::default();
    for (field, slot) in [
        ("role", &mut matcher.role),
        ("accessible_name", &mut matcher.accessible_name),
        ("test_id", &mut matcher.test_id),
        ("component_hint", &mut matcher.component_hint),
        ("dom_id", &mut matcher.dom_id),
    ] {
        if let Some(found) = target.get(Value::from(field)) {
            let text = found
                .as_str()
                .map(str::trim)
                .filter(|text| !text.is_empty())
                .ok_or_else(|| {
                    UiError::Policy(format!(
                        "ui_integrity {section}[{index}].{key}.{field} must be a non-empty string"
                    ))
                })?;
            if text.contains("..") || text.contains('/') || text.contains('\\') {
                return Err(UiError::Policy(format!(
                    "ui_integrity {section}[{index}].{key}.{field} must be a semantic identity, \
                     not a path"
                )));
            }
            *slot = Some(text.to_owned());
        }
    }
    if matcher.is_empty() {
        return Err(UiError::Policy(format!(
            "ui_integrity {section}[{index}].{key} names no node; an allowance must say \
             exactly what it allows"
        )));
    }
    Ok(matcher)
}

fn required_text(map: &Mapping, key: &str, section: &str, index: usize) -> Result<String, UiError> {
    optional_text(map, key, section, index)?.ok_or_else(|| {
        UiError::Policy(format!(
            "ui_integrity {section}[{index}] requires a non-empty `{key}`"
        ))
    })
}

fn optional_text(
    map: &Mapping,
    key: &str,
    section: &str,
    index: usize,
) -> Result<Option<String>, UiError> {
    match map.get(Value::from(key)) {
        None => Ok(None),
        Some(value) => value
            .as_str()
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map(|text| Some(text.to_owned()))
            .ok_or_else(|| {
                UiError::Policy(format!(
                    "ui_integrity {section}[{index}].{key} must be a non-empty string"
                ))
            }),
    }
}

fn expiry(map: &Mapping, section: &str, index: usize) -> Result<Option<String>, UiError> {
    let Some(raw) = optional_text(map, "expires", section, index)? else {
        return Ok(None);
    };
    if !valid_iso_date(&raw) {
        return Err(UiError::Policy(format!(
            "ui_integrity {section}[{index}].expires must be an ISO `YYYY-MM-DD` date"
        )));
    }
    Ok(Some(raw))
}

fn is_expired(expires: Option<&str>, today: &str) -> bool {
    expires.is_some_and(|date| date < today)
}

/// ISO `YYYY-MM-DD`, validated structurally so string ordering is date ordering.
fn valid_iso_date(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        return false;
    }
    let digits = |range: std::ops::Range<usize>| bytes[range].iter().all(u8::is_ascii_digit);
    if !digits(0..4) || !digits(5..7) || !digits(8..10) {
        return false;
    }
    let month: u32 = value[5..7].parse().unwrap_or(0);
    let day: u32 = value[8..10].parse().unwrap_or(0);
    (1..=12).contains(&month) && (1..=31).contains(&day)
}
