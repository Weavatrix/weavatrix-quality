//! Shadow evaluation of selected vs full protection. Not a published speedup claim.

#![forbid(unsafe_code)]

mod case;
mod evaluate;
mod suites;

pub use case::{Ecosystem, FindingLabel, KnownBug, ShadowCase, ten_x_publication_blocked_reason};
pub use evaluate::{ShadowReport, evaluate};
pub use suites::{case_from_runner, go_service_case, node_bun_backend_case, ts_frontend_case};
