//! Shared core for CodeDocs.
//!
//! Everything here is plain Rust with no Tauri or Leptos dependency, which
//! means it compiles for both the WASM frontend and the native backend *and*
//! can be unit-tested with a plain `cargo test`.
//!
//! It exists to hold the three things that both sides of the IPC boundary need
//! to agree on:
//!
//! * [`FileEntry`] — the project tree shape.
//! * [`markdown`] — rendering that is safe to inject into the webview.
//! * [`path_guard`] / [`tree`] — filesystem access confined to the open folder.
//!
//! The filesystem modules are compiled for native targets only; the frontend
//! has no business walking directories.

pub mod markdown;
mod types;

#[cfg(not(target_arch = "wasm32"))]
pub mod path_guard;
#[cfg(not(target_arch = "wasm32"))]
pub mod tree;

pub use markdown::{
    extract_headings, live_spans, markdown_to_typst, render_markdown, Heading, LiveSpans, SpanTag,
};
pub use types::FileEntry;

#[cfg(not(target_arch = "wasm32"))]
pub use path_guard::{validate_new_name, Workspace, WorkspaceError};
#[cfg(not(target_arch = "wasm32"))]
pub use tree::{build_tree, should_skip_dir, FileTree, TreeError, Truncation, WalkLimits};

/// Extension the editor is willing to open.
pub const MARKDOWN_EXTENSION: &str = "md";

/// `true` for paths this app knows how to open.
pub fn is_markdown_path(path: &std::path::Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case(MARKDOWN_EXTENSION))
}
