//! Exporting the current buffer as a PDF.
//!
//! Split out of `commands.rs` for one reason: everything worth testing here —
//! the destination being the user's choice, the note being inside the
//! workspace, the generated source not surviving the export — does not need a
//! webview. What stays in the command is the native dialog.
//!
//! # The destination is the user's choice, and that is deliberate
//!
//! Every other write goes through [`Workspace`], because the frontend renders
//! untrusted markdown and a command that trusts a path is an arbitrary-write
//! primitive. This one cannot: the PDF goes where the user pointed the save
//! dialog, and a PDF in their Downloads folder is the entire point. So what is
//! checked is the *note* — it has to be a file inside the workspace, the same
//! rule every other command follows — and the compiler, which is only run once
//! its digest has matched.
//!
//! The generated `.typ` is a different matter: Typst refuses a source outside
//! its own `--root`, and the root has to be the note's folder for a relative
//! image in the note to resolve. So it is written there, under a dotted name the
//! watcher ignores, and deleted on the way out — including when the compile
//! fails, which is the path that would otherwise leave litter in the user's
//! notes.

use std::path::{Path, PathBuf};

use codedocs_core::Workspace;

/// Starting name offered by the save dialog: the note's name with `.pdf`.
pub fn suggested_name(note: &Path) -> String {
    let stem = note
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        // A note named `.md` has a stem of `.md`, which would suggest a file
        // called `.md.pdf` — hidden, and not what anyone wants to be offered.
        .filter(|stem| !stem.is_empty() && !stem.starts_with('.'))
        .unwrap_or_else(|| "documento".to_string());
    format!("{stem}.pdf")
}

/// The export, minus the dialog.
///
/// Takes the note's path *inside the workspace* rather than a resolved one, so
/// the check that it belongs to the workspace is part of what is tested. The
/// picker belongs to the command in `commands.rs`, with every other command.
pub async fn export_to_pdf(
    workspace: &Workspace,
    note_path: &str,
    content: &str,
    destination: &str,
) -> Result<String, String> {
    let note = workspace
        .resolve_file(note_path)
        .map_err(|e| e.to_string())?;
    let folder = note
        .parent()
        .ok_or_else(|| format!("Ruta inválida: '{}'", note.display()))?;
    let destination = PathBuf::from(destination);
    if let Some(parent) = destination.parent() {
        if !parent.as_os_str().is_empty() && !parent.is_dir() {
            return Err(format!(
                "No se pudo escribir el PDF: la carpeta '{}' no existe",
                parent.display()
            ));
        }
    }

    crate::pdf::export_pdf(content, folder, &destination).await?;

    // The compiler reported success; this asks the question that matters. The
    // embedded library produced an empty file, with no error, when it was built
    // without the font feature. Reading the header rather than the length is
    // what makes this a *PDF* check and not a "the file exists" check.
    let written = std::fs::read(&destination).unwrap_or_default();
    if !crate::pdf::looks_like_pdf(&written) {
        return Err(format!(
            "La exportación a PDF no produjo un PDF ({} bytes)",
            written.len()
        ));
    }
    Ok(destination.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    use crate::state::AppState;

    /// A fresh directory, canonicalised the way `Workspace::open` needs.
    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "codedocs-export-{tag}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir.canonicalize().unwrap()
    }

    /// The compiler these tests drive, if there is one.
    ///
    /// `CODEDOCS_TYPST_BIN` points at a real `typst`. Without it a test says so
    /// out loud and passes — a *silent* skip is how an empty-PDF bug hides.
    fn real_compiler() -> Option<PathBuf> {
        std::env::var_os("CODEDOCS_TYPST_BIN").map(PathBuf::from)
    }

    /// Everything a test needs: a workspace with one note, and a destination
    /// outside it.
    fn workspace_with_note(tag: &str) -> (Workspace, String, PathBuf) {
        let notes = scratch(&format!("{tag}-notes"));
        let state = AppState::default();
        let workspace = state.open_workspace(&notes).unwrap();
        fs::write(notes.join("nota.md"), "# Nota\n\nCon **negrita**.\n").unwrap();
        // The destination is outside the workspace on purpose: that is what a
        // save dialog produces, and refusing it would make the feature useless.
        let destination = scratch(&format!("{tag}-out")).join("el.pdf");
        (
            workspace,
            notes.join("nota.md").to_string_lossy().into_owned(),
            destination,
        )
    }

    #[test]
    fn the_suggested_name_follows_the_note() {
        assert_eq!(
            suggested_name(Path::new("/notas/bitácora.md")),
            "bitácora.pdf"
        );
        // A note whose name is only an extension still gets a usable name.
        assert_eq!(suggested_name(Path::new("/notas/.md")), "documento.pdf");
    }

    #[tokio::test]
    async fn a_saved_note_exports_to_the_chosen_path() {
        if real_compiler().is_none() {
            eprintln!("SKIPPED: export to a chosen path. Set CODEDOCS_TYPST_BIN.");
            return;
        }
        let (workspace, note, destination) = workspace_with_note("basic");

        let written = export_to_pdf(&workspace, &note, "# Nota\n", destination.to_str().unwrap())
            .await
            .expect("the export should succeed");

        assert_eq!(written, destination.to_string_lossy());
        let bytes = fs::read(&destination).unwrap();
        assert!(
            crate::pdf::looks_like_pdf(&bytes),
            "not a PDF: {} bytes",
            bytes.len()
        );
        assert!(bytes.len() > 1000, "suspiciously small: {}", bytes.len());
    }

    #[tokio::test]
    async fn a_note_outside_the_workspace_is_refused() {
        let (workspace, _, destination) = workspace_with_note("outside");
        let outside = scratch("outside-note");
        let planted = outside.join("plantada.md");
        fs::write(&planted, "# No es tuya\n").unwrap();

        let err = export_to_pdf(
            &workspace,
            planted.to_str().unwrap(),
            "# No es tuya\n",
            destination.to_str().unwrap(),
        )
        .await
        .expect_err("a note outside the workspace must be refused");

        assert!(
            err.contains("fuera de la carpeta del proyecto"),
            "unexpected error: {err}"
        );
        assert!(!destination.exists(), "nothing may be written");
    }

    #[tokio::test]
    async fn a_destination_whose_folder_is_gone_is_refused_before_any_work() {
        let (workspace, note, _) = workspace_with_note("gonefolder");
        let missing = scratch("gonefolder-missing")
            .join("no-existe")
            .join("el.pdf");

        let err = export_to_pdf(&workspace, &note, "# Nota\n", missing.to_str().unwrap())
            .await
            .expect_err("a missing folder must be refused");

        assert!(err.contains("no existe"), "{err}");
    }

    #[tokio::test]
    async fn the_generated_source_does_not_survive_a_successful_export() {
        if real_compiler().is_none() {
            eprintln!("SKIPPED: source cleanup. Set CODEDOCS_TYPST_BIN.");
            return;
        }
        let (workspace, note, destination) = workspace_with_note("cleanup");

        export_to_pdf(&workspace, &note, "# Nota\n", destination.to_str().unwrap())
            .await
            .expect("the export should succeed");

        let folder = Path::new(&note).parent().unwrap();
        let leftovers = stray_files(folder);
        assert!(leftovers.is_empty(), "left behind: {leftovers:?}");
    }

    #[tokio::test]
    async fn the_generated_source_is_removed_even_when_the_compile_fails() {
        if real_compiler().is_none() {
            eprintln!("SKIPPED: cleanup on failure. Set CODEDOCS_TYPST_BIN.");
            return;
        }
        let (workspace, note, destination) = workspace_with_note("failure");

        // Math Typst cannot resolve: the compile fails *after* the source file
        // has been written, which is the path that would leave litter.
        let outcome = export_to_pdf(
            &workspace,
            &note,
            "$x$ y $\\comandoinexistente$\n",
            destination.to_str().unwrap(),
        )
        .await;

        assert!(outcome.is_err(), "unresolvable math should fail the export");
        let folder = Path::new(&note).parent().unwrap();
        let leftovers = stray_files(folder);
        assert!(
            leftovers.is_empty(),
            "left behind after a failure: {leftovers:?}"
        );
        assert!(!destination.exists() || fs::metadata(&destination).unwrap().len() > 0);
    }

    #[tokio::test]
    async fn an_empty_document_still_produces_a_pdf() {
        if real_compiler().is_none() {
            eprintln!("SKIPPED: empty document export. Set CODEDOCS_TYPST_BIN.");
            return;
        }
        let (workspace, note, destination) = workspace_with_note("empty");

        export_to_pdf(&workspace, &note, "", destination.to_str().unwrap())
            .await
            .expect("an empty note still has a page");

        let bytes = fs::read(&destination).unwrap();
        assert!(
            crate::pdf::looks_like_pdf(&bytes),
            "not a PDF: {} bytes",
            bytes.len()
        );
    }

    /// Everything in `folder` except the note itself.
    fn stray_files(folder: &Path) -> Vec<String> {
        fs::read_dir(folder)
            .unwrap()
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name != "nota.md")
            .collect()
    }
}
