//! Shared UI building blocks.
//!
//! These were previously spread across `ui.rs` and `modals_impl.rs` with
//! duplicated markup; components that have exactly one caller now live next to
//! that caller instead.

mod modals_impl;

pub use modals_impl::{AlertModal, DeleteConfirmModal, RenameConfirmModal};
