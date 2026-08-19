//! Weavatrix-backed code evidence for Weavatrix Quality.
//!
//! This crate does **not** parse source and does **not** build a second code
//! graph. [`weavatrix_rust`] is the only repository/code authority. WVQ stores
//! revision-bound references (`repository` + `revision` + counts), never a
//! duplicate `Graph`.

#![forbid(unsafe_code)]

mod weavatrix;

pub use weavatrix::{
    CodeEvidenceProvider, IntelligenceError, RepoEvidence, WeavatrixProvider,
};
