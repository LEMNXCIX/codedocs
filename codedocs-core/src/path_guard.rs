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
        let candidate = Path::new(raw);
        let canonical = candidate
            .canonicalize()
            .map_err(|source| WorkspaceError::Io {
                path: candidate.to_path_buf(),
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
        let path = self.resolve(raw)?;
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
}
