use leptos::prelude::*;

use crate::types::FileEntry;
use crate::utils::markdown::Heading;

/// Whether the on-disk file matches the buffer.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub enum SaveState {
    /// No file open.
    #[default]
    Idle,
    /// Buffer differs from disk.
    Dirty,
    /// A write is in flight.
    Saving,
    /// Buffer matches disk.
    Saved,
    /// The last write failed; the message is shown to the user.
    Failed(String),
}

impl SaveState {
    pub fn label(&self) -> String {
        match self {
            SaveState::Idle => String::new(),
            SaveState::Dirty => "Sin guardar".into(),
            SaveState::Saving => "Guardando…".into(),
            SaveState::Saved => "Guardado".into(),
            SaveState::Failed(_) => "Error al guardar".into(),
        }
    }

    pub fn is_error(&self) -> bool {
        matches!(self, SaveState::Failed(_))
    }
}

/// Autosave debounce, in milliseconds.
pub(crate) const AUTOSAVE_DELAY_MS: i32 = 1500;

/// All editor state, provided through context.
///
/// Lived here rather than as locals in `Layout` so the autosave effect, the
/// file watcher and the UI all read one source of truth. The previous code kept
/// these as separate locals in the root component, which is why the autosave
/// effect could not tell "the user typed" from "the watcher reloaded the file"
/// and ended up writing the file back to itself forever.
#[derive(Clone, Copy)]
pub struct EditorState {
    pub path: RwSignal<Option<String>>,
    pub files: RwSignal<Vec<FileEntry>>,
    pub selected_file: RwSignal<Option<String>>,
    pub content: RwSignal<String>,
    /// Last content known to be on disk. Autosave compares against this so a
    /// reload triggered by our own write does not schedule another write.
    pub last_saved: RwSignal<String>,
    pub save_state: RwSignal<SaveState>,
    /// Monotonic counter identifying the most recent file-open request.
    /// Responses from superseded requests are discarded, so clicking two files
    /// quickly cannot leave file A's content sitting under file B's name.
    pub open_token: RwSignal<u64>,
    pub headings: RwSignal<Vec<Heading>>,
    pub source_mode: RwSignal<bool>,
    pub is_dark: RwSignal<bool>,
    pub sidebar_width: RwSignal<f64>,
    pub is_resizing_sidebar: RwSignal<bool>,
    pub toast: RwSignal<Option<String>>,
}

impl EditorState {
    pub fn new() -> Self {
        Self {
            path: RwSignal::new(None),
            files: RwSignal::new(Vec::new()),
            selected_file: RwSignal::new(None),
            content: RwSignal::new(String::new()),
            last_saved: RwSignal::new(String::new()),
            save_state: RwSignal::new(SaveState::Idle),
            open_token: RwSignal::new(0),
            headings: RwSignal::new(Vec::new()),
            source_mode: RwSignal::new(false),
            is_dark: RwSignal::new(false),
            sidebar_width: RwSignal::new(280.0),
            is_resizing_sidebar: RwSignal::new(false),
            toast: RwSignal::new(None),
        }
    }

    pub fn notify(&self, message: impl Into<String>) {
        self.toast.set(Some(message.into()));
    }
}

impl Default for EditorState {
    fn default() -> Self {
        Self::new()
    }
}

/// Word count, treating whitespace as the separator.
pub fn word_count(content: &str) -> usize {
    if content.trim().is_empty() {
        0
    } else {
        content.split_whitespace().count()
    }
}

/// Character count.
///
/// Counts Unicode scalar values, not bytes: `len()` would report roughly twice
/// the real number for Spanish text with accents or ñ.
pub fn char_count(content: &str) -> usize {
    content.chars().count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn word_count_ignores_extra_whitespace() {
        assert_eq!(word_count(""), 0);
        assert_eq!(word_count("   \n\t "), 0);
        assert_eq!(word_count("one two three"), 3);
        assert_eq!(word_count("  one   two  "), 2);
    }

    #[test]
    fn char_count_counts_characters_not_bytes() {
        assert_eq!(char_count("hola"), 4);
        // "ñ" and "á" are two bytes each in UTF-8.
        assert_eq!(char_count("ñáé"), 3);
        assert_eq!(char_count("ñoño"), 4);
        assert_eq!("ñ".len(), 2);
    }

    #[test]
    fn save_state_labels() {
        assert_eq!(SaveState::Idle.label(), "");
        assert_eq!(SaveState::Dirty.label(), "Sin guardar");
        assert!(SaveState::Failed("boom".into()).is_error());
        assert!(!SaveState::Saved.is_error());
    }
}
