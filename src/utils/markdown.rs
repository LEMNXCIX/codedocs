//! Markdown rendering for the frontend.
//!
//! The implementation now lives in the shared `codedocs-core` crate, which is
//! also where its test suite is. This module is a thin re-export so existing
//! call sites keep working.

pub use codedocs_core::markdown::{extract_headings, Heading};
