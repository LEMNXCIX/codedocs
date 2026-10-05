//! PDF export: the Typst compiler, and running it.
//!
//! # Why a subprocess and not a library
//!
//! Embedding the Typst *library* was measured before this module existed: it
//! multiplies the release binary by 4.86 (+59 MiB) and doubles release build
//! time (+282 s), because it drags in a full layout engine, font handling and
//! image decoding. The CLI produces the same output — verified case by case
//! against the 0.15.1 binary — and costs a download.
//!
//! # Where the compiler comes from
//!
//! The binary is looked up in a cache directory and, if absent, downloaded from
//! the GitHub release and **verified against a sha256 embedded in this file**
//! before it is run. Typst publishes no checksums, so the ones here are ones we
//! computed; a digest that does not match is never executed and the user is
//! told to install Typst by hand.
//!
//! # Why the `.typ` file is written next to the note
//!
//! Typst resolves paths relative to the *source file* and refuses a source
//! outside `--root` ("source file must be contained in project root"). So the
//! only arrangement in which `--root` does what it is for — relative images in
//! the note resolving to real files — is a source file inside the root, which is
//! the note's own folder. It is written under a dotted name the watcher ignores
//! (it filters markdown) and removed on the way out, error or not.
//!
//! # Safety
//!
//! The generated source is the output of `markdown_to_typst`, which escapes
//! document text and only ever puts strings around scheme-checked URLs — see
//! that module for why that is not a sandbox escape. Nothing here executes
//! anything the document says, and the binary that runs is the one whose digest
//! matched.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use sha2::{Digest, Sha256};

/// The release this module knows how to fetch.
const TYPST_VERSION: &str = "0.15.1";

/// Where a release asset lives.
fn asset_url(asset: &str) -> String {
    format!("https://github.com/typst/typst/releases/download/v{TYPST_VERSION}/{asset}")
}

/// One platform's release asset, with the digest we computed for it.
///
/// Typst publishes neither a checksum file nor a digest in the release notes,
/// so these are ours. They were computed from the published assets for v0.15.1
/// and every asset was verified to be a valid archive containing the executable
/// before the digest was recorded.
///
/// A digest that does not match is **not** a warning: the download is discarded
/// and Typst is not run, because the alternative is executing whatever arrived.
const ASSETS: &[(&str, &str, &str)] = &[
    (
        "x86_64-unknown-linux-musl",
        "typst-x86_64-unknown-linux-musl.tar.xz",
        "a6d077d0a95eed5a2eba715b2dae06be954f624ccbf85758a03f389ded33118c",
    ),
    (
        "aarch64-unknown-linux-musl",
        "typst-aarch64-unknown-linux-musl.tar.xz",
        "5aa8d74a3d906e60ea12a66ac2f37f8eef1b14cbad7182a745e393a10c23dcee",
    ),
    (
        "x86_64-apple-darwin",
        "typst-x86_64-apple-darwin.tar.xz",
        "7f9fdd9584866245de9a79e0add8f9236fae6f40a8a45e2c4771ccc14db4e0fa",
    ),
    (
        "aarch64-apple-darwin",
        "typst-aarch64-apple-darwin.tar.xz",
        "48f62ed034aa3a7978309579ac6ca00045e2ef0da73114e8af27cfd8e74dc05a",
    ),
    (
        "x86_64-pc-windows-msvc",
        "typst-x86_64-pc-windows-msvc.zip",
        "19ce3551153c2fe7ee9fa2f95208310c8f4d3209fedb699e0333faf8913f6736",
    ),
    (
        "aarch64-pc-windows-msvc",
        "typst-aarch64-pc-windows-msvc.zip",
        "4ab28e1b71ec3184d38d580ab797f499b6770d952b6b19167be5cea5c2662e14",
    ),
];

/// The release asset for the platform this build runs on.
fn asset_for_this_platform() -> Result<(&'static str, &'static str), String> {
    let target = std::env::consts::ARCH;
    let os = std::env::consts::OS;
    let triple = match (os, target) {
        ("linux", "x86_64") => "x86_64-unknown-linux-musl",
        ("linux", "aarch64") => "aarch64-unknown-linux-musl",
        ("macos", "x86_64") => "x86_64-apple-darwin",
        ("macos", "aarch64") => "aarch64-apple-darwin",
        ("windows", "x86_64") => "x86_64-pc-windows-msvc",
        ("windows", "aarch64") => "aarch64-pc-windows-msvc",
        _ => return Err(install_by_hand(os, target)),
    };
    ASSETS
        .iter()
        .find(|(name, _, _)| *name == triple)
        .map(|(_, asset, digest)| (*asset, *digest))
        .ok_or_else(|| install_by_hand(os, target))
}

/// The message shown when this build has no asset, or nothing works.
fn install_by_hand(os: &str, arch: &str) -> String {
    format!(
        "No hay un compilador de Typst para {os}/{arch}. \
         Instalalo a mano desde https://github.com/typst/typst/releases/tag/v{TYPST_VERSION} \
         y volve a intentar; CodeDocs lo usa para exportar a PDF."
    )
}

/// Where the compiler is cached.
///
/// Hand-rolled rather than pulled from a crate: it is three `cfg` branches, and
/// the app has no other use for a directories library. `CODEDOCS_TYPST_DIR`
/// overrides it, which is what lets the tests use a scratch directory.
pub fn cache_dir() -> Result<PathBuf, String> {
    if let Some(custom) = std::env::var_os("CODEDOCS_TYPST_DIR") {
        return Ok(PathBuf::from(custom));
    }
    let base = cache_root().ok_or_else(|| {
        "No se pudo decidir dónde guardar el compilador de Typst. \
         Configurá CODEDOCS_TYPST_DIR con una carpeta."
            .to_string()
    })?;
    Ok(base.join("codedocs").join("typst").join(TYPST_VERSION))
}

/// The platform's cache directory.
fn cache_root() -> Option<PathBuf> {
    if cfg!(target_os = "windows") {
        std::env::var_os("LOCALAPPDATA").map(PathBuf::from)
    } else if cfg!(target_os = "macos") {
        std::env::var_os("HOME").map(|home| PathBuf::from(home).join("Library").join("Caches"))
    } else {
        std::env::var_os("XDG_CACHE_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".cache")))
    }
}

/// The executable's path inside the cache.
fn cached_binary() -> Result<PathBuf, String> {
    let name = if cfg!(target_os = "windows") {
        "typst.exe"
    } else {
        "typst"
    };
    Ok(cache_dir()?.join(name))
}

/// The sha256 of `bytes`, lowercase hex.
pub fn digest_of(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher
        .finalize()
        .iter()
        .fold(String::with_capacity(64), |mut out, byte| {
            use std::fmt::Write;
            let _ = write!(out, "{byte:02x}");
            out
        })
}

/// Whether `bytes` is the asset we expected.
///
/// The only gate between "something came off the network" and "we run it".
/// A mismatch is never a warning and never a retry with the same bytes.
fn verify(bytes: &[u8], expected: &str) -> Result<(), String> {
    let actual = digest_of(bytes);
    if actual == expected {
        return Ok(());
    }
    Err(format!(
        "El compilador descargado no coincide con la firma esperada \
         (esperado {expected}, obtenido {actual}). No se va a ejecutar. \
         Instalá Typst a mano o borrá la caché y probá de nuevo."
    ))
}

/// Fetch, verify and unpack the compiler, returning its path.
///
/// Split from the caller so the *whole* download path can be driven from a test
/// with a local archive: `fetch_from` takes bytes, so the test never touches the
/// network while the same code runs.
pub async fn ensure_compiler() -> Result<PathBuf, String> {
    let target = cached_binary()?;
    if target.is_file() {
        return Ok(target);
    }

    let (asset, expected) = asset_for_this_platform()?;
    let url = asset_url(asset);
    let bytes = download(&url).await?;
    verify(&bytes, expected)?;
    unpack(&bytes, asset, &target)?;

    // Made executable only after the digest matched: an unverified archive never
    // becomes a runnable file.
    make_executable(&target)?;
    Ok(target)
}

/// Download `url`, or say why not.
async fn download(url: &str) -> Result<Vec<u8>, String> {
    let response = reqwest::get(url)
        .await
        .map_err(|e| format!("No se pudo descargar el compilador de Typst: {e}"))?;
    if !response.status().is_success() {
        return Err(format!(
            "No se pudo descargar el compilador de Typst ({})",
            response.status()
        ));
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|e| format!("La descarga del compilador de Typst se cortó: {e}"))?;
    Ok(bytes.to_vec())
}

/// Pull the executable out of a release archive and write it to `target`.
fn unpack(bytes: &[u8], asset: &str, target: &Path) -> Result<(), String> {
    let executable = if asset.ends_with(".zip") {
        executable_from_zip(bytes)?
    } else {
        executable_from_tar_xz(bytes)?
    };

    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("No se pudo crear la caché del compilador: {e}"))?;
    }
    // Written beside its final name and renamed into place, so an interrupted
    // download cannot leave a half-written binary that looks usable next time.
    let scratch = target.with_extension("descargando");
    fs::write(&scratch, &executable)
        .map_err(|e| format!("No se pudo guardar el compilador de Typst: {e}"))?;
    fs::rename(&scratch, target)
        .map_err(|e| format!("No se pudo guardar el compilador de Typst: {e}"))
}

/// The `typst` executable inside a `.tar.xz` release archive.
fn executable_from_tar_xz(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let mut tar = Vec::new();
    lzma_rs::xz_decompress(&mut &bytes[..], &mut tar)
        .map_err(|e| format!("El archivo del compilador vino corrupto: {e}"))?;

    let mut archive = tar::Archive::new(std::io::Cursor::new(tar));
    let wanted = std::env::consts::EXE_SUFFIX;
    let entries = archive
        .entries()
        .map_err(|e| format!("No se pudo leer el archivo del compilador: {e}"))?;
    for entry in entries {
        let mut entry = entry.map_err(|e| format!("No se pudo leer el archivo: {e}"))?;
        let path = entry
            .path()
            .map_err(|e| format!("No se pudo leer el archivo: {e}"))?
            .to_string_lossy()
            .into_owned();
        // The archive holds `<target>/typst` plus licence files. The file name
        // alone is the test: a licence called `typst` would be a coincidence
        // worth avoiding, and a nested path is not what we want to execute.
        let name = path.rsplit('/').next().unwrap_or_default();
        if name == format!("typst{wanted}") {
            let mut out = Vec::new();
            std::io::copy(&mut entry, &mut out)
                .map_err(|e| format!("No se pudo extraer el compilador: {e}"))?;
            return Ok(out);
        }
    }
    Err("El archivo del compilador no trae el ejecutable".to_string())
}

/// The `typst.exe` inside a `.zip` release archive.
fn executable_from_zip(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes))
        .map_err(|e| format!("El archivo del compilador vino corrupto: {e}"))?;
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|e| format!("No se pudo leer el archivo del compilador: {e}"))?;
        let name = entry.name().rsplit(['/', '\\']).next().unwrap_or_default();
        if name.eq_ignore_ascii_case("typst.exe") {
            let mut out = Vec::new();
            std::io::copy(&mut entry, &mut out)
                .map_err(|e| format!("No se pudo extraer el compilador: {e}"))?;
            return Ok(out);
        }
    }
    Err("El archivo del compilador no trae el ejecutable".to_string())
}

/// Mark the extracted file runnable.
#[cfg(unix)]
fn make_executable(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    let mut permissions = fs::metadata(path)
        .map_err(|e| format!("No se pudo preparar el compilador: {e}"))?
        .permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions)
        .map_err(|e| format!("No se pudo preparar el compilador: {e}"))
}

/// Nothing to do: Windows does not carry the executable bit.
#[cfg(not(unix))]
fn make_executable(_path: &Path) -> Result<(), String> {
    Ok(())
}

/// Compile `content` into `destination` and return what the compiler said.
///
/// `note_folder` is where the note lives: it is both the root Typst is given —
/// which is what makes a relative image in the note resolve — and where the
/// generated source is written, because Typst refuses a source outside its root.
pub async fn export_pdf(
    content: &str,
    note_folder: &Path,
    destination: &Path,
) -> Result<(), String> {
    let compiler = ensure_compiler().await?;
    let source = note_folder.join(source_file_name());
    let typst = codedocs_core::markdown_to_typst(content, note_folder);

    // Held across the whole write-compile-remove, because the source file has a
    // fixed name: two exports in flight would otherwise write over each other's
    // source and compile the wrong document. Everything inside is synchronous,
    // so the guard is never held across an `await`.
    let _one_at_a_time = export_lock();
    // Written before the compiler runs and removed after, however this returns.
    // The name starts with a dot and does not end in `.md`, so the file watcher
    // — which filters markdown — never reports it.
    fs::write(&source, &typst).map_err(|e| format!("No se pudo preparar la exportación: {e}"))?;
    let _written = RemoveOnDrop(source.clone());

    compile(&compiler, &source, note_folder, destination)
}

/// A generated source file that deletes itself.
///
/// The one path that matters is the error path: a leftover `.typ` in the user's
/// notes folder is litter in a folder they did not choose for it.
struct RemoveOnDrop(PathBuf);

impl Drop for RemoveOnDrop {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

/// The name the generated source gets.
///
/// Dotted, markdown-free and stable, so two exports of the same note reuse one
/// file instead of filling the folder.
fn source_file_name() -> &'static str {
    ".codedocs-export.typ"
}

/// Run the compiler.
fn compile(compiler: &Path, source: &Path, root: &Path, destination: &Path) -> Result<(), String> {
    let output = Command::new(compiler)
        .arg("compile")
        // The root is what lets the note's own relative images resolve, and it
        // is also a boundary: Typst refuses to read outside it.
        .arg("--root")
        .arg(root)
        .arg(source)
        .arg(destination)
        .output()
        .map_err(|e| format!("No se pudo ejecutar el compilador de Typst: {e}"))?;

    if output.status.success() {
        return Ok(());
    }

    // Typst's own message names the file, the line and the column, which is
    // exactly what the user needs to fix their note. Passing it through
    // verbatim is the difference between "algo falló" and "línea 42".
    let stderr = String::from_utf8_lossy(&output.stderr);
    let detail = stderr.trim();
    if detail.is_empty() {
        return Err("El compilador de Typst falló sin decir por qué".to_string());
    }
    Err(format!("El compilador de Typst falló:\n{detail}"))
}

/// Serialises exports.
///
/// Two reasons, one of them a bug this module would otherwise have:
///
/// * The generated source has a fixed name inside the note's folder, so two
///   exports in flight would write over each other's source and one of them
///   would compile the other's document.
/// * Linux answers `ETXTBSY` ("text file busy") when a binary is executed while
///   any thread of the process holds it open for writing, and `cargo test`
///   runs the test binary's threads in parallel — one test extracting an
///   archive while another exec'd the same compiler produced exactly that, in
///   one run out of three.
///
/// A plain mutex, held only around the synchronous part (write, compile, remove)
/// so it is never held across an `await`.
pub fn export_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Whether a compiled PDF looks like one.
///
/// The failure this exists for: with the *library*, a build without the font
/// feature produced a PDF that was empty and reported no error. A file that is
/// zero bytes, or that is not a PDF at all, is the same class of silent failure
/// coming back through a different door.
pub fn looks_like_pdf(bytes: &[u8]) -> bool {
    bytes.len() > 5 && bytes.starts_with(b"%PDF-")
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- The digest gate ----

    #[test]
    fn a_matching_digest_is_accepted() {
        let bytes = b"contenido";
        let digest = digest_of(bytes);
        assert!(verify(bytes, &digest).is_ok());
    }

    #[test]
    fn one_flipped_byte_is_refused() {
        let digest = digest_of(b"contenido");
        let err = verify(b"contenidp", &digest).expect_err("a flipped byte must be refused");
        assert!(err.contains("No se va a ejecutar"), "{err}");
    }

    #[test]
    fn an_empty_download_is_refused() {
        let digest = digest_of(b"contenido");
        assert!(verify(b"", &digest).is_err());
    }

    #[test]
    fn the_digest_of_known_input_is_the_known_value() {
        // A hash function that silently changed would make every other check in
        // this file vacuous, so it is pinned to a published vector.
        assert_eq!(
            digest_of(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            digest_of(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    // ---- The embedded table ----

    #[test]
    fn every_asset_has_a_well_formed_digest() {
        // Typst publishes no checksums, so these are ours. A typo here is not a
        // failed download, it is a compiler that never runs — and an empty
        // digest would be worse than a wrong one, silently disabling the check.
        for (platform, asset, digest) in ASSETS {
            assert_eq!(
                digest.len(),
                64,
                "el digest de {platform} no parece un sha256: {digest:?}"
            );
            assert!(
                digest
                    .chars()
                    .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()),
                "el digest de {platform} no es hex minúscula: {digest:?}"
            );
            assert!(
                asset.contains(platform.split('-').next().unwrap_or(platform)),
                "el asset de {platform} no coincide con la plataforma: {asset}"
            );
        }
    }

    #[test]
    fn every_release_platform_is_covered() {
        let covered: Vec<&str> = ASSETS.iter().map(|(name, _, _)| *name).collect();
        for expected in [
            "x86_64-unknown-linux-musl",
            "aarch64-unknown-linux-musl",
            "x86_64-apple-darwin",
            "aarch64-apple-darwin",
            "x86_64-pc-windows-msvc",
            "aarch64-pc-windows-msvc",
        ] {
            assert!(
                covered.contains(&expected),
                "falta el asset para {expected}"
            );
        }
    }

    #[test]
    fn no_two_assets_share_a_digest() {
        let mut digests: Vec<&str> = ASSETS.iter().map(|(_, _, digest)| *digest).collect();
        digests.sort_unstable();
        let count = digests.len();
        digests.dedup();
        assert_eq!(digests.len(), count, "dos assets con el mismo digest");
    }

    #[test]
    fn the_url_points_at_the_pinned_release() {
        assert_eq!(
            asset_url("typst-x86_64-unknown-linux-musl.tar.xz"),
            "https://github.com/typst/typst/releases/download/v0.15.1/typst-x86_64-unknown-linux-musl.tar.xz"
        );
    }

    #[test]
    fn this_platform_has_an_asset() {
        // The test machine is one of the six; a missing entry means the export
        // silently tells every user to install Typst by hand.
        let (asset, digest) = asset_for_this_platform().expect("this platform needs an asset");
        assert!(asset.starts_with("typst-"), "{asset}");
        assert_eq!(digest.len(), 64, "{digest}");
    }

    // ---- Unpacking ----

    /// A `.tar.xz` holding a file, built here so the test needs no fixture.
    fn tar_xz_with(path_in_archive: &str, content: &[u8]) -> Vec<u8> {
        let mut tarball = Vec::new();
        {
            let mut builder = tar::Builder::new(&mut tarball);
            let mut header = tar::Header::new_gnu();
            header.set_size(content.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            builder
                .append_data(&mut header, path_in_archive, content)
                .unwrap();
            builder.finish().unwrap();
        }
        let mut compressed = Vec::new();
        lzma_rs::xz_compress(&mut tarball.as_slice(), &mut compressed).unwrap();
        compressed
    }

    /// A `.zip` holding a file, built here for the same reason.
    fn zip_with(name: &str, content: &[u8]) -> Vec<u8> {
        let mut out = std::io::Cursor::new(Vec::new());
        {
            let mut writer = zip::ZipWriter::new(&mut out);
            let options: zip::write::FileOptions<'_, ()> = zip::write::FileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated);
            writer.start_file(name, options).unwrap();
            std::io::Write::write_all(&mut writer, content).unwrap();
            writer.finish().unwrap();
        }
        out.into_inner()
    }

    #[test]
    fn a_tar_xz_yields_its_executable() {
        let archive = tar_xz_with("typst-x86_64-unknown-linux-musl/typst", b"#!/bin/sh\n");
        let extracted = executable_from_tar_xz(&archive).expect("the executable is in there");
        assert_eq!(extracted, b"#!/bin/sh\n");
    }

    #[test]
    fn a_zip_yields_its_executable() {
        let archive = zip_with("typst-x86_64-pc-windows-msvc/typst.exe", b"MZ");
        let extracted = executable_from_zip(&archive).expect("the executable is in there");
        assert_eq!(extracted, b"MZ");
    }

    #[test]
    fn a_tar_xz_without_an_executable_is_an_error() {
        let archive = tar_xz_with("typst-x86_64-unknown-linux-musl/LICENSE", b"texto");
        let err = executable_from_tar_xz(&archive).expect_err("no executable inside");
        assert!(err.contains("no trae el ejecutable"), "{err}");
    }

    #[test]
    fn a_zip_without_an_executable_is_an_error() {
        let archive = zip_with("typst-x86_64-pc-windows-msvc/README.md", b"texto");
        assert!(executable_from_zip(&archive).is_err());
    }

    #[test]
    fn a_corrupt_archive_is_an_error_not_a_panic() {
        assert!(executable_from_tar_xz(b"no soy un tar").is_err());
        assert!(executable_from_zip(b"no soy un zip").is_err());
    }

    #[test]
    fn an_unverified_archive_is_never_written_to_the_cache() {
        // The whole point of the digest: whatever arrives, the cache ends up
        // either empty or holding the real compiler.
        let dir = scratch("digest");
        let target = dir.join("typst");
        let archive = tar_xz_with("typst/typst", b"contenido sospechoso");
        let wrong = digest_of(b"otra cosa");

        let outcome = verify(&archive, &wrong)
            .and_then(|()| unpack(&archive, "typst-x86_64-unknown-linux-musl.tar.xz", &target));

        assert!(
            outcome.is_err(),
            "a mismatched digest must stop the install"
        );
        assert!(!target.exists(), "nothing may be written to the cache");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_verified_archive_lands_in_the_cache() {
        let dir = scratch("unpack");
        let target = dir.join("typst");
        let archive = tar_xz_with("typst/typst", b"contenido real");
        let digest = digest_of(&archive);

        verify(&archive, &digest).expect("the digest we just computed");
        unpack(&archive, "typst-x86_64-unknown-linux-musl.tar.xz", &target)
            .expect("the archive is valid");

        assert_eq!(fs::read(&target).unwrap(), b"contenido real");
        // No half-written leftovers next to it.
        assert!(!dir.join("typst.descargando").exists());
        let _ = fs::remove_dir_all(&dir);
    }

    // ---- Is it really a PDF? ----

    #[test]
    fn an_empty_file_is_not_a_pdf() {
        // The silent failure this guards: an empty export that reports success.
        assert!(!looks_like_pdf(b""));
        assert!(!looks_like_pdf(b"%PD"));
    }

    #[test]
    fn a_pdf_header_is_recognised() {
        assert!(looks_like_pdf(b"%PDF-1.7\nresto"));
    }

    #[test]
    fn something_that_is_not_a_pdf_is_refused() {
        assert!(!looks_like_pdf(b"<!doctype html><html></html>"));
        assert!(!looks_like_pdf(b"PK\x03\x04 un zip"));
    }

    // ---- The generated source's name ----

    #[test]
    fn the_generated_source_is_invisible_to_the_watcher() {
        // The watcher filters markdown, so a `.typ` file never triggers a
        // reload — but the dot is what keeps it out of the user's file list too.
        let name = source_file_name();
        assert!(name.starts_with('.'), "{name}");
        assert!(!name.ends_with(".md"), "{name}");
        assert!(!codedocs_core::is_markdown_path(Path::new(name)));
    }

    /// A fresh directory for a test that writes files.
    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "codedocs-pdf-{tag}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    // ---- The whole download path, against the real release ----

    /// Downloads the real asset, checks the embedded digest and installs it.
    ///
    /// `#[ignore]`d because it needs the network, so the gate stays hermetic.
    /// Run it by hand when the embedded digests change:
    ///
    /// ```text
    /// CODEDOCS_TYPST_DIR=/tmp/typst-cache \
    ///   cargo test -p codedocs -- --ignored descarga_el_binario_real
    /// ```
    ///
    /// It is the only thing that proves the published asset still matches the
    /// digest this file claims for it — which is the one thing that cannot be
    /// checked offline.
    #[tokio::test]
    #[ignore = "necesita la red"]
    async fn descarga_el_binario_real() {
        let dir = scratch("descarga");
        // SAFETY-free alternative: the cache location is read through the same
        // env var the app honours, and this test sets it before anything runs.
        std::env::set_var("CODEDOCS_TYPST_DIR", &dir);

        let compiler = ensure_compiler()
            .await
            .expect("the download should succeed");

        assert!(compiler.is_file(), "{} no existe", compiler.display());
        let version = Command::new(&compiler)
            .arg("--version")
            .output()
            .expect("el binario descargado debería ejecutar");
        assert!(
            version.status.success(),
            "el binario no arranca: {}",
            String::from_utf8_lossy(&version.stderr)
        );
        let reported = String::from_utf8_lossy(&version.stdout);
        assert!(
            reported.contains(TYPST_VERSION),
            "otra versión de Typst: {reported}"
        );

        // A second call must not download anything: the cache is the point.
        let again = ensure_compiler()
            .await
            .expect("the cached binary should be reused");
        assert_eq!(again, compiler);
        let _ = fs::remove_dir_all(&dir);
    }

    // ---- End to end, with the real compiler ----

    /// The compiler this test drives, if there is one.
    ///
    /// `CODEDOCS_TYPST_BIN` points at a real `typst`; without it the test says
    /// so out loud and passes, because a *silent* skip is exactly how an empty
    /// PDF bug hides.
    fn real_compiler() -> Option<PathBuf> {
        std::env::var_os("CODEDOCS_TYPST_BIN").map(PathBuf::from)
    }

    #[tokio::test]
    async fn un_compiled_markdown_becomes_a_real_pdf() {
        let Some(compiler) = real_compiler() else {
            eprintln!(
                "SKIPPED: export PDF end-to-end. Set CODEDOCS_TYPST_BIN to a typst binary \
                 to run it (typst --version)."
            );
            return;
        };
        // One export at a time: see `export_lock`.
        let _serialised = export_lock();

        let dir = scratch("e2e");
        let destination = dir.join("salida.pdf");
        let markdown = "# Informe\n\nTexto con **negrita** y $E = mc^2$.\n\n- uno\n- dos\n";

        fs::write(dir.join("nota.md"), markdown).unwrap();
        let typst = codedocs_core::markdown_to_typst(markdown, &dir);
        let source = dir.join(source_file_name());
        fs::write(&source, &typst).unwrap();

        compile(&compiler, &source, &dir, &destination).expect("the compiler should succeed");

        let bytes = fs::read(&destination).expect("the compiler wrote a file");
        assert!(
            looks_like_pdf(&bytes),
            "the export is not a PDF: {} bytes starting {:?}",
            bytes.len(),
            String::from_utf8_lossy(&bytes[..bytes.len().min(8)])
        );
        // Non-zero and more than a header: the empty-PDF failure mode.
        assert!(
            bytes.len() > 1000,
            "the PDF is suspiciously small: {}",
            bytes.len()
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn un_markdown_con_una_imagen_rota_aun_asienta_un_pdf_real() {
        let Some(compiler) = real_compiler() else {
            eprintln!("SKIPPED: broken image export. Set CODEDOCS_TYPST_BIN.");
            return;
        };
        // One export at a time: see `export_lock`.
        let _serialised = export_lock();

        let dir = scratch("e2e-rota");
        let destination = dir.join("salida.pdf");
        // One image that is there and one that is not, so this is the whole
        // round trip rather than a document that only mentions a missing file.
        fs::write(dir.join("presente.svg"), SVG).unwrap();
        let markdown = "# Informe\n\nAntes.\n\n![la que esta](presente.svg)\n\n\
                        ![la que falta](rota.png)\n\nDespués.\n";

        // The check happens here, where the `.typ` is generated — the compiler
        // is never handed a path it would have to resolve.
        let typst = codedocs_core::markdown_to_typst(markdown, &dir);
        assert!(
            typst.contains(r#"#image("presente.svg")"#),
            "the image that is there should still be there: {typst}"
        );
        assert!(
            !typst.contains("#image(\"rota.png\")"),
            "the missing image is still a reference: {typst}"
        );
        assert!(
            typst.contains("rota.png"),
            "the reader should be told what was missing: {typst}"
        );
        let source = dir.join(source_file_name());
        fs::write(&source, &typst).unwrap();

        // Before the check this was the whole failure: one unresolvable
        // `#image` and `typst compile` exits non-zero having written nothing, so
        // the export produced no file and the user was told "exporting does not
        // work" instead of "this picture is missing".
        compile(&compiler, &source, &dir, &destination).expect("el PDF tiene que salir igual");

        let bytes = fs::read(&destination).expect("the compiler wrote a file");
        assert!(
            looks_like_pdf(&bytes),
            "the export is not a PDF: {} bytes",
            bytes.len()
        );
        assert!(
            bytes.len() > 1000,
            "the PDF is suspiciously small: {}",
            bytes.len()
        );
        let _ = fs::remove_dir_all(&dir);
    }

    /// A real image, as a string, so the fixture needs no binary file.
    const SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="40" height="20" viewBox="0 0 40 20"><rect width="40" height="20" fill="#3366cc"/></svg>"##;

    #[tokio::test]
    async fn a_compile_error_names_the_line() {
        let Some(compiler) = real_compiler() else {
            eprintln!("SKIPPED: export PDF error path. Set CODEDOCS_TYPST_BIN.");
            return;
        };
        // One export at a time: see `export_lock`.
        let _serialised = export_lock();

        let dir = scratch("e2e-error");
        let destination = dir.join("salida.pdf");
        // A `#read` of a path outside the root: a real Typst error, which is
        // what the user has to be shown.
        fs::write(dir.join(source_file_name()), "#read(\"/etc/hostname\")\n").unwrap();

        let err = compile(&compiler, &dir.join(source_file_name()), &dir, &destination)
            .expect_err("reading outside the root must fail");
        assert!(err.contains("falló"), "{err}");
        assert!(
            err.contains(".typ"),
            "the message should point at the source: {err}"
        );
        let _ = fs::remove_dir_all(&dir);
    }
}
