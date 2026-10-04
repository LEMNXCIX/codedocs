/// The project file tree, shared with the backend.
///
/// Declared once in the `codedocs-core` crate so both sides of the IPC boundary
/// agree on the shape. Re-exported here so existing `crate::types::FileEntry`
/// call sites keep resolving.
pub use codedocs_core::FileEntry;
