//! Workspace boundary enforcement.
//!
//! Every filesystem command in the Tauri backend is reachable from JavaScript
//! running in the webview. Without a boundary check, a path such as
//! `../../../../etc/shadow` — or a `javascript:` URL executed by a hostile
//! markdown file — turns the app into an arbitrary-file-read/write/delete
//! primitive. This module is the single chokepoint that prevents it.

use std::fmt;
use std::path::{Component, Path, PathBuf};

/// Maximum length of a new file name, generous but bounded.
const MAX_NAME_LEN: usize = 255;

#[derive(Debug)]
pub enum WorkspaceError {
    /// The workspace root could not be resolved.
    InvalidRoot { path: PathBuf, reason: String },
    /// A path resolved outside the workspace root.
    Outside { path: PathBuf, root: PathBuf },
    /// A new file name was rejected.
    InvalidName { name: String, reason: &'static str },
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
}

impl fmt::Display for WorkspaceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WorkspaceError::InvalidRoot { path, reason } => {
                write!(
                    f,
                    "No se pudo abrir la carpeta '{}': {}",
                    path.display(),
                    reason
                )
            }
            WorkspaceError::Outside { path, root } => write!(
                f,
                "Ruta fuera de la carpeta del proyecto: '{}' (proyecto: '{}')",
                path.display(),
                root.display()
            ),
            WorkspaceError::InvalidName { name, reason } => {
                write!(f, "Nombre de archivo no válido '{}': {}", name, reason)
            }
            WorkspaceError::Io { path, source } => {
                write!(f, "Error de E/S en '{}': {}", path.display(), source)
            }
        }
    }
}

impl std::error::Error for WorkspaceError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            WorkspaceError::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

/// A canonicalised project root that all filesystem access is confined to.
#[derive(Debug, Clone)]
pub struct Workspace {
    root: PathBuf,
}

impl Workspace {
    /// Resolve and canonicalise `root`, which must be an existing directory.
    pub fn open(root: &Path) -> Result<Self, WorkspaceError> {
        let canonical = root.canonicalize().map_err(|source| WorkspaceError::Io {
            path: root.to_path_buf(),
            source,
        })?;
        if !canonical.is_dir() {
            return Err(WorkspaceError::InvalidRoot {
                path: root.to_path_buf(),
                reason: "no es una carpeta".into(),
            });
        }
        Ok(Self { root: canonical })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// `true` when `path` is the root itself or lies beneath it.
    ///
    /// Both sides should already be canonical; see [`Workspace::resolve`].
    pub fn contains(&self, path: &Path) -> bool {
        path == self.root || path.starts_with(&self.root)
    }

    /// Canonicalise `raw` and confirm it is inside the workspace.
    ///
    /// Canonicalisation resolves symlinks *before* the containment check, so a
    /// symlink inside the project that points at `/etc` is rejected rather than
    /// silently followed.
    pub fn resolve(&self, raw: &str) -> Result<PathBuf, WorkspaceError> {
        self.canonical_inside(Path::new(raw))
    }

    /// Canonicalise `path` and confirm the *result* is inside the workspace.
    ///
    /// Split out of [`Workspace::resolve`] so the raw-path wrapper stays the
    /// documented entry point while callers that already hold a `&Path` — the
    /// write path below — do not have to stringify it and lose the original on
    /// the error.
    fn canonical_inside(&self, path: &Path) -> Result<PathBuf, WorkspaceError> {
        let canonical = path.canonicalize().map_err(|source| WorkspaceError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        if self.contains(&canonical) {
            Ok(canonical)
        } else {
            Err(WorkspaceError::Outside {
                path: canonical,
                root: self.root.clone(),
            })
        }
    }

    /// Resolve the target of a write, whether or not the file is on disk yet.
    ///
    /// This is the difference between "open this document" and "save this
    /// document", and the two cannot share one check.
    ///
    /// [`Workspace::resolve_file`] canonicalises the whole path, and
    /// canonicalising something that does not exist fails with `ENOENT`. A save
    /// that goes through it can therefore only ever overwrite files that already
    /// exist, which is why saving a document with no file — `Ctrl+S` with nothing
    /// open, or the first save into a folder the app has never seen — used to
    /// fail with "No such file or directory" on a path the user had just picked
    /// in the native dialog.
    ///
    /// So this splits on whether there is a directory entry at `raw`:
    ///
    /// * **Something is there.** Hand it to [`Workspace::resolve_file`]
    ///   unchanged. Canonicalising the *file* is what resolves a symlink before
    ///   the containment test, so a link inside the project pointing at
    ///   `/etc/passwd` is refused instead of followed.
    /// * **Nothing is there.** There is no file to canonicalise, so
    ///   containment moves to the *parent directory* — which does exist — and the
    ///   last component has to survive [`validate_new_name`] before it is
    ///   joined onto the canonical parent.
    ///
    /// The second case is also the reason the first one cannot be skipped
    /// wholesale. Checking only the parent and joining the name would let a
    /// missing final component *be* a symlink: the parent would pass containment
    /// and the write would follow the link to its target. And
    /// `symlink_metadata` rather than `exists` is what decides the branch,
    /// because a dangling link exists as a directory entry but not as a file;
    /// treating it as a free name would aim the write at whatever it points at.
    ///
    /// Returns the path to write to, which is the *joined* path rather than a
    /// canonicalised one — the file does not exist, so there is nothing to
    /// canonicalise. It is built from an already-canonical directory and a bare
    /// name, so it cannot escape.
    pub fn resolve_write_target(&self, raw: &str) -> Result<PathBuf, WorkspaceError> {
        let candidate = Path::new(raw);
        if candidate.symlink_metadata().is_ok() {
            return self.resolve_file(raw);
        }

        let name = candidate
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| WorkspaceError::InvalidName {
                name: raw.to_string(),
                reason: "no es un nombre de archivo",
            })?;
        validate_new_name(name)?;

        let parent = candidate.parent().ok_or_else(|| WorkspaceError::Outside {
            path: candidate.to_path_buf(),
            root: self.root.clone(),
        })?;
        // The containment check lands on the directory here, which is why the
        // name had to be validated first: a validated bare name cannot introduce
        // a `..` or a separator to walk back out of it.
        let dir = self.resolve_dir(parent)?;
        Ok(dir.join(name))
    }

    /// Resolve a path that is expected to be an existing regular file.
    pub fn resolve_file(&self, raw: &str) -> Result<PathBuf, WorkspaceError> {
        let path = self.resolve(raw)?;
        if path == self.root {
            return Err(WorkspaceError::Outside {
                path,
                root: self.root.clone(),
            });
        }
        if !path.is_file() {
            return Err(WorkspaceError::InvalidRoot {
                path,
                reason: "no es un archivo".into(),
            });
        }
        Ok(path)
    }

    /// Resolve a folder inside the workspace that new entries may be created in.
    pub fn resolve_folder(&self, raw: &str) -> Result<PathBuf, WorkspaceError> {
        self.resolve_dir(Path::new(raw))
    }

    /// [`Workspace::resolve_folder`] for a caller that already has a `&Path`.
    fn resolve_dir(&self, raw: &Path) -> Result<PathBuf, WorkspaceError> {
        let path = self.canonical_inside(raw)?;
        if !path.is_dir() {
            return Err(WorkspaceError::InvalidRoot {
                path,
                reason: "no es una carpeta".into(),
            });
        }
        Ok(path)
    }

    /// Build the target path for a *new* file, validating `name` first.
    ///
    /// `name` must be a bare file name: no separators, no `..`, no absolute
    /// prefix, and a `.md` extension. This is what stops
    /// `rename_file("../evil", "../../.ssh/authorized_keys")` from escaping.
    pub fn resolve_new_file(&self, folder: &str, name: &str) -> Result<PathBuf, WorkspaceError> {
        validate_new_name(name)?;
        let dir = self.resolve_folder(folder)?;
        Ok(dir.join(name))
    }

    /// Build the rename target for an existing file inside the workspace.
    pub fn resolve_rename(
        &self,
        old_path: &str,
        new_name: &str,
    ) -> Result<(PathBuf, PathBuf), WorkspaceError> {
        validate_new_name(new_name)?;
        let source = self.resolve_file(old_path)?;
        let parent = source
            .parent()
            .ok_or_else(|| WorkspaceError::Outside {
                path: source.clone(),
                root: self.root.clone(),
            })?
            .to_path_buf();
        let target = parent.join(new_name);
        // `new_name` is a validated bare name, so the join cannot escape — but
        // assert it anyway rather than relying on that invariant silently.
        debug_assert!(target.starts_with(&self.root));
        if !target.starts_with(&self.root) {
            return Err(WorkspaceError::Outside {
                path: target,
                root: self.root.clone(),
            });
        }
        Ok((source, target))
    }
}

/// Reject anything that is not a plain `.md` file name.
pub fn validate_new_name(name: &str) -> Result<(), WorkspaceError> {
    let reject = |reason: &'static str| WorkspaceError::InvalidName {
        name: name.to_string(),
        reason,
    };

    if name.is_empty() {
        return Err(reject("está vacío"));
    }
    if name.len() > MAX_NAME_LEN {
        return Err(reject("es demasiado largo"));
    }
    if name.trim() != name {
        return Err(reject("empieza o termina con un espacio"));
    }
    if name == "." || name == ".." {
        return Err(reject("no es un nombre válido"));
    }
    // A bare name has exactly one `Component`, and it is `Normal`.
    let mut components = Path::new(name).components();
    let is_bare =
        matches!(components.next(), Some(Component::Normal(_))) && components.next().is_none();
    if !is_bare {
        return Err(reject("no puede contener rutas ni separadores"));
    }
    // Windows-invalid characters, rejected everywhere for consistency.
    if name
        .chars()
        .any(|c| matches!(c, '<' | '>' | ':' | '"' | '|' | '?' | '*') || (c as u32) < 0x20)
    {
        return Err(reject("contiene caracteres no permitidos"));
    }
    // Trailing dots and spaces are silently stripped by Windows, which would
    // desynchronise the tree the UI displays from what is on disk.
    if name.ends_with('.') || name.ends_with(' ') {
        return Err(reject("no puede terminar en punto"));
    }
    let lower = name.to_ascii_lowercase();
    if !lower.ends_with(".md") {
        return Err(reject("la extensión debe ser .md"));
    }
    // Reject `.md` — extension present but no actual name, which would create
    // a hidden file the sidebar filters out.
    let stem = &name[..name.len() - 3];
    if stem.trim().is_empty() {
        return Err(reject("falta el nombre del archivo"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("codedocs-ws-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create temp dir");
        dir.canonicalize().expect("canonicalize temp dir")
    }

    fn write(dir: &Path, rel: &str, body: &str) -> PathBuf {
        let path = dir.join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&path, body).unwrap();
        path
    }

    #[test]
    fn accepts_paths_inside_the_workspace() {
        let dir = temp_dir("inside");
        let file = write(&dir, "notes/a.md", "# hi");
        let ws = Workspace::open(&dir).unwrap();
        assert_eq!(ws.resolve_file(file.to_str().unwrap()).unwrap(), file);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn rejects_dotdot_traversal() {
        let dir = temp_dir("traverse");
        let ws = Workspace::open(&dir).unwrap();
        let escape = format!("{}/../../etc/passwd", dir.display());
        assert!(matches!(
            ws.resolve(&escape),
            Err(WorkspaceError::Outside { .. }) | Err(WorkspaceError::Io { .. })
        ));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn rejects_absolute_path_outside_root() {
        let outside = temp_dir("outside");
        let secret = write(&outside, "secret.md", "top secret");
        let dir = temp_dir("root2");
        let ws = Workspace::open(&dir).unwrap();
        assert!(matches!(
            ws.resolve_file(secret.to_str().unwrap()),
            Err(WorkspaceError::Outside { .. })
        ));
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&outside);
    }

    #[test]
    fn rejects_workspace_root_as_a_file_target() {
        let dir = temp_dir("rootfile");
        let ws = Workspace::open(&dir).unwrap();
        assert!(ws.resolve_file(dir.to_str().unwrap()).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlink_pointing_outside() {
        let outside = temp_dir("symtarget");
        let secret = write(&outside, "secret.md", "top secret");
        let dir = temp_dir("symroot");
        std::os::unix::fs::symlink(&secret, dir.join("link.md")).unwrap();
        let ws = Workspace::open(&dir).unwrap();
        // Canonicalisation resolves the link before the containment check.
        assert!(matches!(
            ws.resolve_file(dir.join("link.md").to_str().unwrap()),
            Err(WorkspaceError::Outside { .. })
        ));
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&outside);
    }

    #[test]
    fn new_file_name_validation() {
        assert!(validate_new_name("notes.md").is_ok());
        assert!(validate_new_name("Con espacios y acentos ñ.md").is_ok());

        for bad in [
            "",
            "   ",
            "..",
            ".",
            "../evil.md",
            "sub/dir.md",
            "/abs/path.md",
            "no-extension",
            "notes.txt",
            "notes.md.bak",
            "bad<name>.md",
            "bad|name.md",
            "trailing.",
            "C:evil.md",
            ".md",
        ] {
            assert!(
                validate_new_name(bad).is_err(),
                "should have rejected: {bad:?}"
            );
        }
    }

    #[test]
    fn rename_keeps_the_file_in_place() {
        let dir = temp_dir("rename");
        let file = write(&dir, "old.md", "# hi");
        let ws = Workspace::open(&dir).unwrap();
        let (src, dst) = ws
            .resolve_rename(file.to_str().unwrap(), "new.md")
            .expect("rename should be allowed");
        assert_eq!(src, file);
        assert_eq!(dst, dir.join("new.md"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn rename_rejects_escaping_names() {
        let dir = temp_dir("renameescape");
        let file = write(&dir, "old.md", "# hi");
        let ws = Workspace::open(&dir).unwrap();
        for bad in [
            "../evil.md",
            "../../.ssh/authorized_keys",
            "sub/evil.md",
            "evil",
        ] {
            assert!(
                ws.resolve_rename(file.to_str().unwrap(), bad).is_err(),
                "rename should have rejected: {bad:?}"
            );
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn new_file_resolves_inside_workspace() {
        let dir = temp_dir("newfile");
        write(&dir, "sub/keep.md", "# hi");
        let ws = Workspace::open(&dir).unwrap();
        let target = ws
            .resolve_new_file(dir.join("sub").to_str().unwrap(), "fresh.md")
            .expect("new file should resolve");
        assert_eq!(target, dir.join("sub/fresh.md"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn new_file_rejects_escaping_names() {
        let dir = temp_dir("newfilescape");
        let ws = Workspace::open(&dir).unwrap();
        assert!(ws
            .resolve_new_file(dir.to_str().unwrap(), "../evil.md")
            .is_err());
        assert!(ws
            .resolve_new_file(dir.to_str().unwrap(), "/etc/evil.md")
            .is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ---- Write targets: the file may not be on disk yet --------------------

    #[test]
    fn write_target_resolves_a_new_file_inside_the_workspace() {
        let dir = temp_dir("writetarget");
        write(&dir, "sub/keep.md", "# hi");
        let ws = Workspace::open(&dir).unwrap();
        // The regression: canonicalising this path failed with ENOENT, so a save
        // of a document with no file could only ever fail.
        let fresh = dir.join("sub/fresh.md");
        assert!(!fresh.exists(), "the fixture must not create the file");
        let target = ws
            .resolve_write_target(fresh.to_str().unwrap())
            .expect("a new file inside the workspace should resolve");
        assert_eq!(target, fresh);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn write_target_resolves_a_new_file_at_the_workspace_root() {
        let dir = temp_dir("writeroot");
        let ws = Workspace::open(&dir).unwrap();
        let target = ws
            .resolve_write_target(dir.join("notas.md").to_str().unwrap())
            .expect("a new file at the root should resolve");
        assert_eq!(target, dir.join("notas.md"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn write_target_resolves_the_same_new_file_twice() {
        let dir = temp_dir("writetwice");
        let ws = Workspace::open(&dir).unwrap();
        let raw = dir.join("notas.md").to_str().unwrap().to_string();

        // First resolution: the file does not exist, so the parent carries the
        // containment check.
        let first = ws.resolve_write_target(&raw).unwrap();
        std::fs::write(&first, "# hola").unwrap();

        // Second resolution: now it exists, so it goes through `resolve_file`.
        // Both paths have to keep working or the second save of the same
        // document — the autosave, every time — would fail.
        let second = ws.resolve_write_target(&raw).unwrap();
        assert_eq!(first, second);
        assert_eq!(std::fs::read_to_string(&second).unwrap(), "# hola");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn write_target_rejects_a_new_file_outside_the_workspace() {
        let outside = temp_dir("writeoutside");
        let dir = temp_dir("writeinside");
        let ws = Workspace::open(&dir).unwrap();
        let escape = outside.join("plantado.md");
        assert!(
            ws.resolve_write_target(escape.to_str().unwrap()).is_err(),
            "a new file outside the workspace must be refused"
        );
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&outside);
    }

    #[test]
    fn write_target_rejects_dotdot_in_the_name() {
        let dir = temp_dir("writedotdot");
        // A real intermediate directory, so a rejection cannot come from a
        // parent that does not exist: `sub/../escaped.md` has to be refused for a
        // reason that has nothing to do with ENOENT.
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        let ws = Workspace::open(&dir).unwrap();

        // A `..` that reaches the *parent* is resolved before the containment
        // check, so it lands inside and is not itself a rejection — what matters
        // is that it is gone from the path that gets returned.
        let landed = ws
            .resolve_write_target(dir.join("sub/../notas.md").to_str().unwrap())
            .expect("a mid-path '..' is canonicalised, not carried through");
        assert_eq!(
            landed,
            dir.join("notas.md"),
            "the '..' survived into the target"
        );

        // A `..` in the last position has no file name at all, and one that
        // walks out of the parent is caught by the containment check.
        for bad in [
            "..",
            ".",
            "sub/..",
            "sub/.",
            "../escaped.md",
            "/etc/evil.md",
        ] {
            let raw = dir.join(bad);
            assert!(
                ws.resolve_write_target(raw.to_str().unwrap()).is_err(),
                "should have rejected: {bad:?}"
            );
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn write_target_rejects_a_name_that_is_not_markdown() {
        let dir = temp_dir("writetxt");
        let ws = Workspace::open(&dir).unwrap();
        // Inside the workspace, with a parent that exists, so containment alone
        // would let it through. The app only lists `.md`, so a save that created
        // `notas.txt` would produce a document the sidebar can never show again.
        let err = ws
            .resolve_write_target(dir.join("notas.txt").to_str().unwrap())
            .expect_err("a name that is not markdown should be refused");
        assert!(
            matches!(err, WorkspaceError::InvalidName { .. }),
            "unexpected error: {err}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn write_target_rejects_a_new_file_under_a_symlinked_parent() {
        // The hole the two-case split exists to close: canonicalising only the
        // parent resolves this to a directory *outside* the workspace, so the
        // check has to follow the link and fail there — a bare name validation
        // alone would pass it and the write would land outside.
        let outside = temp_dir("parenttarget");
        let dir = temp_dir("parentroot");
        let ws = Workspace::open(&dir).unwrap();
        std::os::unix::fs::symlink(&outside, dir.join("escape")).unwrap();
        let escape = dir.join("escape/plantado.md");
        assert!(
            ws.resolve_write_target(escape.to_str().unwrap()).is_err(),
            "a parent that symlinks out of the workspace must be refused"
        );
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&outside);
    }

    #[cfg(unix)]
    #[test]
    fn write_target_still_rejects_an_existing_symlink_pointing_outside() {
        // The behaviour the new branch must not cost us: `save_file` now calls
        // `resolve_write_target`, so the existing-file case is the one that keeps
        // a link pointing at `/etc/passwd` from being written through.
        let outside = temp_dir("linktarget");
        let secret = write(&outside, "secret.md", "top secret");
        let dir = temp_dir("linkroot");
        std::os::unix::fs::symlink(&secret, dir.join("link.md")).unwrap();
        let ws = Workspace::open(&dir).unwrap();
        assert!(matches!(
            ws.resolve_write_target(dir.join("link.md").to_str().unwrap()),
            Err(WorkspaceError::Outside { .. })
        ));
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&outside);
    }

    #[cfg(unix)]
    #[test]
    fn write_target_refuses_a_dangling_symlink() {
        // It exists as a directory entry but not as a file. Resolving it as a
        // "free name" would aim the write at whatever it points at, so it has to
        // take the existing-entry branch and fail there.
        let outside = temp_dir("danglingtarget");
        let dir = temp_dir("danglingroot");
        let ws = Workspace::open(&dir).unwrap();
        std::os::unix::fs::symlink(outside.join("nunca.md"), dir.join("link.md")).unwrap();
        assert!(ws
            .resolve_write_target(dir.join("link.md").to_str().unwrap())
            .is_err());
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&outside);
    }
}
