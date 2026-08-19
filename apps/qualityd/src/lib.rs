//! Quality Studio HTTP surface. Spec §30 and §58.
//!
//! HTTP is a transport over `wvq-command-bus`, exactly like the CLI and MCP.
//! No quality policy is decided here.

#![forbid(unsafe_code)]

mod http;
mod studio;

pub use http::{
    HttpRequest, HttpResponse, MAX_BODY_BYTES, MAX_HEAD_BYTES, read_request, serve, serve_one,
};
pub use studio::Studio;
