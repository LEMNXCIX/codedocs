//! Bounded project file-tree construction.
//!
//! Opening a folder must never hang the app, but it also must not refuse a
//! folder just because it is large. Three things used to make hanging possible
//! and are handled here:
//!
//! * **Unbounded recursion** into `.git`, `node_modules`, `target`, virtualenvs.
//! * **Unbounded entry count** — a single `node_modules` is ~10⁵ files.
//! * **Symlink cycles**, which make a naive recursive walk loop forever.
//!
//! The budget exists to stop a pathological walk, not to police how many notes
//! somebody keeps. When it is exhausted the walk **stops and reports what it
//! found** rather than failing outright: an earlier version returned an error
//! and threw away a perfectly good tree, which made a folder with 8 000 entries
//! and 200 Markdown files unopenable.

use std::collections::HashSet;
use std::fmt;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::types::FileEntry;

/// Directory names skipped regardless of where they appear.
const ALWAYS_SKIP: &[&str] = &[
    "node_modules",
    "target",
    "vendor",
    "dist",
    "build",
    ".venv",
    "venv",
    "__pycache__",
    ".cache",
    "coverage",
    "Pods",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WalkLimits {
    /// Maximum filesystem entries examined before the walk stops.
    pub max_entries: usize,
    /// Maximum directory nesting depth below the root.
    pub max_depth: usize,
}

impl Default for WalkLimits {
    fn default() -> Self {
        Self {
            max_entries: 200_000,
            max_depth: 24,
        }
    }
}

impl WalkLimits {
    /// Budget for interactive "open folder" calls.
    ///
    /// Generous on purpose. Reading a directory entry is cheap — a folder with
    /// 50 000 entries takes tens of milliseconds — and the budget is really
    /// there to stop walking something pathological like a mounted home
    /// directory, not to cap a personal notes folder. Truncation is reported to
    /// the user rather than refused, so a generous limit costs nothing.
    pub fn interactive() -> Self {
        Self {
            max_entries: 100_000,
            max_depth: 20,
        }
    }
}

/// Why the walk stopped before covering everything.
///
/// Serialised because it crosses the IPC boundary: the frontend needs to tell
/// the user the listing is partial.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", content = "value")]
pub enum Truncation {
    /// Ran out of entry budget.
    Entries { scanned: usize, limit: usize },
    /// Ran out of depth budget.
    Depth { limit: usize },
}

impl Truncation {
    pub fn message(&self) -> String {
        match self {
            Truncation::Entries { scanned, limit } => format!(
                "Se показа {} y se detuvo: la carpeta supera las {} entradas analizadas. \
                 Las notas que estén más adentro puede que no aparezcan.",
                scanned, limit
            ),
            Truncation::Depth { limit } => format!(
                "La carpeta tiene más de {limit} niveles de carpetas anidadas; \
                 se mostró hasta ese nivel."
            ),
        }
    }
}

/// A walk result: the tree, plus whether it covers the whole folder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FileTree {
    pub entries: Vec<FileEntry>,
    /// `Some` when the walk stopped early; the tree is then partial.
    pub truncated: Option<Truncation>,
}

impl FileTree {
    /// Number of markdown files in the tree.
    pub fn file_count(&self) -> usize {
        count_files(&self.entries)
    }
}

fn count_files(entries: &[FileEntry]) -> usize {
    entries.iter().map(FileEntry::file_count).sum()
}

#[derive(Debug)]
pub enum TreeError {
    RootMissing {
        path: PathBuf,
    },
    NotADirectory {
        path: PathBuf,
    },
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
}

impl fmt::Display for TreeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TreeError::RootMissing { path } => {
                write!(f, "La carpeta '{}' ya no existe", path.display())
            }
            TreeError::NotADirectory { path } => {
                write!(f, "'{}' no es una carpeta", path.display())
            }
            TreeError::Io { path, source } => {
                write!(f, "No se pudo leer '{}': {}", path.display(), source)
            }
        }
    }
}

impl std::error::Error for TreeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            TreeError::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

/// `true` for directories that are never worth descending into.
///
/// Public because the file watcher needs the same rule: a watcher that descended
/// into `node_modules` would register thousands of inotify watches for trees the
/// sidebar never shows.
pub fn should_skip_dir(name: &str) -> bool {
    name.starts_with('.') || ALWAYS_SKIP.iter().any(|s| s.eq_ignore_ascii_case(name))
}

/// Build the markdown file tree rooted at `root`.
///
/// Returns [`FileTree`], whose `truncated` field says whether the budget cut the
/// walk short. An unreadable root is still an error; an exhausted budget is not.
pub fn build_tree(root: &Path, limits: WalkLimits) -> Result<FileTree, TreeError> {
    if !root.exists() {
        return Err(TreeError::RootMissing {
            path: root.to_path_buf(),
        });
    }
    if !root.is_dir() {
        return Err(TreeError::NotADirectory {
            path: root.to_path_buf(),
        });
    }

    // Canonicalise up front so the visited-set and the returned paths agree.
    let canonical_root = root.canonicalize().map_err(|source| TreeError::Io {
        path: root.to_path_buf(),
        source,
    })?;

    let mut walker = Walker {
        limits,
        scanned: 0,
        visited: HashSet::from([canonical_root.clone()]),
        truncated: None,
    };

    let entries = walker.walk(&canonical_root, 0)?;

    Ok(FileTree {
        entries,
        truncated: walker.truncated,
    })
}

struct Walker {
    limits: WalkLimits,
    scanned: usize,
    /// Canonical directory paths already visited, so a symlink cycle terminates
    /// instead of recursing forever.
    visited: HashSet<PathBuf>,
    /// Set once the budget is spent; stops the walk as soon as it unwinds.
    truncated: Option<Truncation>,
}

impl Walker {
    fn walk(&mut self, dir: &Path, depth: usize) -> Result<Vec<FileEntry>, TreeError> {
        if self.truncated.is_some() {
            return Ok(Vec::new());
        }
        if depth >= self.limits.max_depth {
            self.truncated = Some(Truncation::Depth {
                limit: self.limits.max_depth,
            });
            return Ok(Vec::new());
        }

        let read = std::fs::read_dir(dir).map_err(|source| TreeError::Io {
            path: dir.to_path_buf(),
            source,
        })?;

        let mut dirs: Vec<(PathBuf, String)> = Vec::new();
        let mut files: Vec<FileEntry> = Vec::new();

        for entry in read.flatten() {
            // Check the budget before doing any work for this entry, so a
            // partial walk stops promptly instead of unwinding a deep stack.
            if self.scanned >= self.limits.max_entries {
                self.truncated = Some(Truncation::Entries {
                    scanned: self.scanned,
                    limit: self.limits.max_entries,
                });
                break;
            }
            self.scanned += 1;

            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();

            // `file_type()` does not follow symlinks, which is what we want: a
            // symlinked directory is treated as a link, not as a folder to
            // recurse into blindly.
            let Ok(file_type) = entry.file_type() else {
                continue;
            };

            if file_type.is_dir() {
                if !should_skip_dir(&name) {
                    dirs.push((path, name));
                }
                continue;
            }

            // Symlinks to files are fine to show; symlinked directories are
            // skipped above and handled by the cycle guard where relevant.
            if is_markdown(&path) {
                files.push(FileEntry::file(name, path.to_string_lossy()));
            }
        }

        // Directories first, then files, each alphabetically — the ordering the
        // sidebar has always displayed.
        dirs.sort_by(|a, b| a.1.cmp(&b.1));
        files.sort_by(|a, b| a.name.cmp(&b.name));

        let mut out: Vec<FileEntry> = Vec::with_capacity(dirs.len() + files.len());
        for (path, name) in dirs {
            if self.truncated.is_some() {
                break;
            }
            // Canonicalise before recursing so symlink cycles are detected.
            let Ok(canonical) = path.canonicalize() else {
                continue;
            };
            if !self.visited.insert(canonical.clone()) {
                continue;
            }
            let children = self.walk(&canonical, depth + 1)?;
            self.visited.remove(&canonical);
            // Only surface folders that actually contain markdown.
            if !children.is_empty() {
                out.push(FileEntry::dir(name, path.to_string_lossy(), children));
            }
        }
        out.extend(files);
        Ok(out)
    }
}

/// `true` for paths this app knows how to open.
fn is_markdown(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case(crate::MARKDOWN_EXTENSION))
}

/// `true` when `path` is inside `root` after canonicalisation. Convenience for
/// callers that already hold both.
pub fn is_inside(root: &Path, path: &Path) -> bool {
    let Ok(canonical_root) = root.canonicalize() else {
        return false;
    };
    let Ok(canonical_path) = path.canonicalize() else {
        return false;
    };
    canonical_path == canonical_root || canonical_path.starts_with(canonical_root)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "codedocs-tree-{tag}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir.canonicalize().unwrap()
    }

    fn touch(dir: &Path, rel: &str) {
        let path = dir.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "# x").unwrap();
    }

    #[test]
    fn finds_nested_markdown_files() {
        let dir = scratch("nested");
        touch(&dir, "a.md");
        touch(&dir, "sub/b.md");
        touch(&dir, "sub/deep/c.md");
        let tree = build_tree(&dir, WalkLimits::default()).unwrap();

        // Top level: `sub` (directory) before `a.md` (file).
        assert_eq!(tree.entries.len(), 2);
        assert_eq!(tree.entries[0].name, "sub");
        assert_eq!(tree.entries[1].name, "a.md");

        // Inside `sub`: `deep` (directory) before `b.md` (file).
        let sub = &tree.entries[0].children;
        assert_eq!(sub.len(), 2);
        assert_eq!(sub[0].name, "deep");
        assert_eq!(sub[0].children[0].name, "c.md");
        assert_eq!(sub[1].name, "b.md");
        assert_eq!(tree.truncated, None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn ignores_non_markdown_files() {
        let dir = scratch("nonmd");
        touch(&dir, "keep.md");
        touch(&dir, "notes.txt");
        touch(&dir, "image.png");
        let tree = build_tree(&dir, WalkLimits::default()).unwrap();
        assert_eq!(tree.entries.len(), 1);
        assert_eq!(tree.entries[0].name, "keep.md");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn directories_come_before_files() {
        let dir = scratch("order");
        touch(&dir, "zzz.md");
        touch(&dir, "aaa/b.md");
        let tree = build_tree(&dir, WalkLimits::default()).unwrap();
        assert_eq!(tree.entries[0].name, "aaa");
        assert_eq!(tree.entries[1].name, "zzz.md");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn empty_folders_are_omitted() {
        let dir = scratch("empty");
        touch(&dir, "a.md");
        std::fs::create_dir_all(dir.join("nothing-here")).unwrap();
        let tree = build_tree(&dir, WalkLimits::default()).unwrap();
        assert_eq!(tree.entries.len(), 1);
        assert_eq!(tree.entries[0].name, "a.md");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn errors_on_missing_root() {
        let dir = scratch("missing");
        let gone = dir.join("does-not-exist");
        assert!(matches!(
            build_tree(&gone, WalkLimits::default()),
            Err(TreeError::RootMissing { .. })
        ));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn errors_when_root_is_a_file() {
        let dir = scratch("isfile");
        touch(&dir, "a.md");
        let file = dir.join("a.md");
        assert!(matches!(
            build_tree(&file, WalkLimits::default()),
            Err(TreeError::NotADirectory { .. })
        ));
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ---- The reported problem: a large folder must still open ----

    #[test]
    fn large_folder_opens_instead_of_failing() {
        // 12 000 entries, well past the old 8 000 budget, and only a handful are
        // markdown. This used to be rejected outright and produced no tree.
        let dir = scratch("large");
        for i in 0..12_000 {
            touch(&dir, &format!("f{i:05}.txt"));
        }
        for i in 0..200 {
            touch(&dir, &format!("notes/note{i:04}.md"));
        }

        let tree = build_tree(&dir, WalkLimits::interactive()).unwrap();
        assert_eq!(tree.truncated, None, "should fit in the interactive budget");
        assert_eq!(tree.file_count(), 200);
        assert!(tree.entries.iter().any(|e| e.name == "notes"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn budget_stops_the_walk_but_keeps_what_was_found() {
        let dir = scratch("truncate");
        for i in 0..200 {
            touch(&dir, &format!("f{i:04}.md"));
        }
        let limits = WalkLimits {
            max_entries: 25,
            max_depth: 8,
        };
        let tree = build_tree(&dir, limits).unwrap();

        assert!(tree.truncated.is_some(), "the budget should be reported");
        // The point of the change: partial results beat no results.
        assert!(
            !tree.entries.is_empty(),
            "a truncated walk must keep what it found"
        );
        assert!(
            tree.file_count() < 200,
            "the walk should have stopped early, found {}",
            tree.file_count()
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn truncation_message_is_actionable() {
        let message = Truncation::Entries {
            scanned: 100_000,
            limit: 100_000,
        }
        .message();
        assert!(
            message.contains("100000")
                || message.contains("100 000")
                || message.contains("entradas")
        );
        assert!(Truncation::Depth { limit: 20 }.message().contains("20"));
    }

    #[test]
    fn depth_limit_is_reported_not_silent() {
        let dir = scratch("depth");
        touch(&dir, "a/b/c/d/e/f/g/h/i/j/deep.md");
        touch(&dir, "shallow.md");
        let limits = WalkLimits {
            max_entries: 10_000,
            max_depth: 3,
        };
        let tree = build_tree(&dir, limits).unwrap();
        assert_eq!(tree.truncated, Some(Truncation::Depth { limit: 3 }));
        // What was within reach is still returned.
        assert_eq!(tree.entries.len(), 1);
        assert_eq!(tree.entries[0].name, "shallow.md");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn depth_limit_is_high_enough_for_real_notes_folders() {
        // 15 levels of nesting used to exceed the interactive budget of 8 and
        // truncate a legitimate structure.
        let dir = scratch("deepnotes");
        let deep = (0..15)
            .map(|i| format!("n{i}"))
            .collect::<Vec<_>>()
            .join("/");
        touch(&dir, &format!("{deep}/deep.md"));
        let tree = build_tree(&dir, WalkLimits::interactive()).unwrap();
        assert_eq!(
            tree.truncated, None,
            "15 levels should be well within budget"
        );
        assert_eq!(tree.file_count(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn skips_hidden_and_vendor_directories() {
        let dir = scratch("skips");
        touch(&dir, "visible.md");
        touch(&dir, ".git/config.md");
        touch(&dir, "node_modules/pkg/readme.md");
        touch(&dir, "target/debug/out.md");
        let tree = build_tree(&dir, WalkLimits::default()).unwrap();
        assert_eq!(
            tree.entries.len(),
            1,
            "hidden/vendor dirs leaked into the tree: {:?}",
            tree.entries
        );
        assert_eq!(tree.entries[0].name, "visible.md");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn symlink_cycle_terminates() {
        let dir = scratch("cycle");
        touch(&dir, "sub/real.md");
        // sub/loop -> the project root, so naive recursion never ends.
        std::os::unix::fs::symlink(dir.canonicalize().unwrap(), dir.join("sub/loop")).unwrap();

        let tree = build_tree(&dir, WalkLimits::interactive()).unwrap();
        // It completed; the important part is that it returned at all.
        assert!(tree.entries.iter().any(|e| e.name == "sub"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn is_inside_helper() {
        let dir = scratch("inside");
        touch(&dir, "a.md");
        assert!(is_inside(&dir, &dir.join("a.md")));
        assert!(!is_inside(&dir.join("a.md"), &dir));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
