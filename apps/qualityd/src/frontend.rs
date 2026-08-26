//! Exception-first Quality Studio pages. Spec §31 and §58.
//!
//! These bytes are a projection of the JSON API. They do not decide verdicts.

use crate::http::HttpResponse;

/// Default cockpit. Passing proofs stay behind `suppressed_passing`.
#[must_use]
pub fn cockpit() -> HttpResponse {
    HttpResponse::html(include_str!("../web/index.html"))
}

/// Fetch-and-render script. Unknown API fields are shown, never computed.
#[must_use]
pub fn script() -> HttpResponse {
    HttpResponse::javascript(include_str!("../web/studio.js"))
}
