//! State that outlives a single command invocation.
//!
//! Two things must: the folder the user opened, which is the boundary every
//! filesystem operation is confined to, and the watcher attached to it. Both
//! live in one managed [`AppState`] so there is exactly one of each, rather
//! than a module-level `static` that any command can quietly overwrite.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, RwLock};
use std::time::{Duration, Instant};

use codedocs_core::Workspace;
use notify::RecommendedWatcher;

/// Reported when a lock was poisoned by a panic in an earlier command.
///
/// Surfacing it beats panicking a second time: the app stays usable and the
/// message names what actually went wrong.
pub const LOCK_POISONED: &str = "El estado interno de la app quedó bloqueado por un error anterior";

/// Shown by filesystem commands invoked before any folder was opened.
pub const NO_WORKSPACE: &str = "No hay ninguna carpeta abierta";

/// How long a write made by this process counts as "ours".
///
/// Long enough to cover the watcher still delivering the event for our own
/// write, short enough that a genuine external edit shows up almost at once.
const SELF_WRITE_WINDOW: Duration = Duration::from_millis(1500);

#[derive(Default)]
pub struct AppState {
    /// The open folder. `None` until a folder is opened.
    pub workspace: RwLock<Option<Workspace>>,
    /// Exactly one watcher, replaced — never accumulated — on folder switch.
    pub watcher: Mutex<Option<RecommendedWatcher>>,
    writes: WriteLog,
}

impl AppState {
    /// The open workspace, or a message explaining that none is open.
    pub fn current_workspace(&self) -> Result<Workspace, String> {
        self.workspace
            .read()
            .map_err(|_| LOCK_POISONED.to_string())?
            .clone()
            .ok_or_else(|| NO_WORKSPACE.to_string())
    }

    /// Make `workspace` the open one and hand back a copy of it.
    ///
    /// The frontend fires `list_markdown_files` and `watch_folder` as two
    /// independent tasks when a folder is opened, so neither may assume the
    /// other already installed the workspace.
    pub fn open_workspace(&self, folder: &Path) -> Result<Workspace, String> {
        let workspace = Workspace::open(folder).map_err(|e| e.to_string())?;
        let mut slot = self
            .workspace
            .write()
            .map_err(|_| LOCK_POISONED.to_string())?;
        *slot = Some(workspace.clone());
        Ok(workspace)
    }

    /// A copy of the write log for the watcher callback to share.
    pub fn write_log(&self) -> WriteLog {
        self.writes.clone()
    }

    /// Note that this process just wrote `path`, so the watcher can ignore the
    /// event its own write produced.
    pub fn note_write(&self, path: &Path) {
        self.writes.note(path);
    }

    /// Install `watcher`, dropping whatever was being watched before.
    pub fn set_watcher(&self, watcher: RecommendedWatcher) -> Result<(), String> {
        let mut slot = self.watcher.lock().map_err(|_| LOCK_POISONED.to_string())?;
        *slot = Some(watcher);
        Ok(())
    }

    /// Stop watching. Assigning `None` drops the watcher and its event thread.
    pub fn clear_watcher(&self) -> Result<(), String> {
        let mut slot = self.watcher.lock().map_err(|_| LOCK_POISONED.to_string())?;
        *slot = None;
        Ok(())
    }
}

/// Paths this process wrote recently, shared with the watcher callback.
///
/// The callback runs on a notify thread and has no way to tell whether an event
/// came from the user or from our own `save_file`. Without this guard the app
/// loops forever: save → watcher → frontend reload → autosave → save.
#[derive(Clone, Default)]
pub struct WriteLog(Arc<Mutex<HashSet<(PathBuf, Instant)>>>);

impl WriteLog {
    /// Record that this process just wrote `path`.
    pub fn note(&self, path: &Path) {
        let mut entries = self.lock();
        Self::prune(&mut entries);
        entries.insert((path.to_path_buf(), Instant::now()));
    }

    /// `true` when this process wrote `path` within the self-write window.
    pub fn is_recent(&self, path: &Path) -> bool {
        let mut entries = self.lock();
        Self::prune(&mut entries);
        entries.iter().any(|(written, _)| written == path)
    }

    /// Forget expired entries so a long session cannot grow the set forever.
    fn prune(entries: &mut HashSet<(PathBuf, Instant)>) {
        let now = Instant::now();
        entries.retain(|(_, at)| now.saturating_duration_since(*at) < SELF_WRITE_WINDOW);
    }

    fn lock(&self) -> MutexGuard<'_, HashSet<(PathBuf, Instant)>> {
        // The notify thread has nowhere to report a failure to, so a poisoned
        // lock is recovered from instead of allowed to take the watcher down;
        // the worst case is one event that gets through unfiltered.
        self.0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}
