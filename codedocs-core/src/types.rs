use serde::{Deserialize, Serialize};

/// A node in the project file tree, as produced by the backend and consumed by
/// the frontend. Declared once here so both sides of the IPC boundary agree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    #[serde(default)]
    pub children: Vec<FileEntry>,
}

impl FileEntry {
    pub fn dir(name: impl Into<String>, path: impl Into<String>, children: Vec<FileEntry>) -> Self {
        Self {
            name: name.into(),
            path: path.into(),
            is_dir: true,
            children,
        }
    }

    pub fn file(name: impl Into<String>, path: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            path: path.into(),
            is_dir: false,
            children: Vec::new(),
        }
    }

    /// Total number of files (not directories) at or below this node.
    pub fn file_count(&self) -> usize {
        if !self.is_dir {
            return 1;
        }
        self.children.iter().map(Self::file_count).sum()
    }
}
