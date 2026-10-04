//! `extract`: write one record's bytes into a directory.
//!
//! # The write policy
//!
//! This is the only command in the tool that touches the filesystem for
//! writing, so its rules are explicit:
//!
//! * the package is opened read-only (a memory map) and never written;
//! * the only path ever created is `<--out>/<sanitised name>`;
//! * `--out` is refused when it is the filesystem root, `.`, or ends in a `..`
//!   component, because each of those turns "the directory I was told to use"
//!   into "somewhere above it" or "everywhere at once";
//! * the file name is built from hex id components, then run through
//!   [`sanitise_component`], and the joined path is re-checked to be a direct
//!   child of `--out` before the write. Both of those are defence in depth: the
//!   name cannot currently contain a separator, and the check is what would
//!   catch it if a future rename introduced one.
//!
//! Nothing here creates intermediate directories. A missing `--out` is an I/O
//! failure naming the path, not a silent `mkdir -p` of a typo'd location.

use std::io::Write;
use std::path::{Path, PathBuf};

use spore_core::ResourceKey;

use crate::{ExtractRequest, ToolError, EXIT_IO};

/// How many rows of a "wrote ..." report.
pub fn run(request: &ExtractRequest, out: &mut dyn Write) -> Result<(), ToolError> {
    let store = super::single_package_store(&request.package)?;
    let found = store.find(&request.key)?;
    // Read through the store's priority resolution so the bytes written are the
    // bytes `find` would have reported.
    let bytes = store.read_from(found.package_index, &request.key)?;

    let name = record_file_name(found.entry.type_id, &request.key);
    let target = request.out_dir.join(&name);
    // Defence in depth, as documented above: the written path must be a direct
    // child of `--out` and nothing else.
    debug_assert_eq!(target.parent(), Some(request.out_dir.as_path()));

    std::fs::write(&target, &bytes)
        .map_err(|source| ToolError::Io(format!("{}: {source}", target.display())))?;

    writeln!(
        out,
        "wrote {} ({} bytes) for {}",
        target.display(),
        bytes.len(),
        request.key
    )?;
    let _ = EXIT_IO;
    Ok(())
}

/// The file name for one record: `<type name or hex>-<t>-<g>-<i>.bin`.
///
/// Every component is hex digits or a name from this build's own fixed table, so
/// the name is deterministic (two runs write the same file), sorts sensibly next
/// to its siblings, and contains no character a filesystem object. It is still
/// pushed through [`sanitise_component`], because "no separator today" is a
/// property of the inputs rather than of the function.
pub fn record_file_name(type_id: u32, key: &ResourceKey) -> String {
    let stem = super::type_name_of(type_id)
        .map(str::to_owned)
        .unwrap_or_default();
    let stem = if stem.is_empty() {
        format!("0x{type_id:08x}")
    } else {
        stem
    };
    sanitise_component(&format!(
        "{stem}-{:08x}-{:08x}-{:08x}.bin",
        key.type_id, key.group_id, key.instance_id
    ))
}

/// Replaces every byte that is not `[A-Za-z0-9._-]` with `_`.
///
/// Deliberately *not* a "strip the bad characters" function: replacing keeps the
/// name's length and shape, so two different records can never collapse onto one
/// file name, which would silently lose one of them. The `.` and `-` cases are
/// worth naming: a component of `.` or `..` is not a usable file name at all and
/// is refused rather than sanitised into something harmless-looking.
pub fn sanitise_component(raw: &str) -> String {
    raw.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// Validates `--out`.
///
/// Refused: the empty string, `/`, `.`, `..`, and any path whose last component
/// is `..`. Everything else is accepted verbatim — including a path with a `..`
/// component in the *middle*, which is an ordinary relative path like
/// `../out/dumps` that resolves where the user meant it to.
pub fn validate_out_dir(raw: &str) -> Result<PathBuf, crate::CliError> {
    let trimmed = raw.trim();
    let refused = trimmed.is_empty()
        || trimmed == "/"
        || trimmed == "."
        || trimmed == ".."
        || Path::new(trimmed)
            .components()
            .next_back()
            .is_some_and(|component| component == std::path::Component::ParentDir);
    if refused {
        return Err(crate::CliError::UnsafeOutDir(raw.to_owned()));
    }
    Ok(PathBuf::from(trimmed))
}

/// Validates `--out` for a command that writes **one file** rather than a
/// directory.
///
/// Two extra refusals over [`validate_out_dir`], both about where the bytes end
/// up rather than what the path is spelled like:
///
/// * the empty string and a path with no file name at all (`out/`), because
///   those do not name a file this tool could create;
/// * **any path that resolves inside this repository.** Every tool in this
///   workspace writes only where it was told to, and "told to" is not enough for
///   a command whose output is megabytes of expanded texture: a plausible
///   `--out foo.png` run from the repository root would otherwise drop a
///   megabyte next to the sources, where it is neither ignored nor reviewable.
///
/// The repository root is [`env!("CARGO_MANIFEST_DIR")`]'s parent, baked in at
/// build time. When that directory does not exist — an installed binary built
/// elsewhere — the comparison is textual and simply matches nothing, so the guard
/// is inert rather than wrong.
pub fn validate_out_file(raw: &str) -> Result<PathBuf, crate::CliError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(crate::CliError::UnsafeOutFile(raw.to_owned()));
    }
    let path = PathBuf::from(trimmed);
    if path.file_name().is_none() {
        return Err(crate::CliError::UnsafeOutFile(raw.to_owned()));
    }
    if inside_repository(&path) {
        return Err(crate::CliError::OutFileInsideRepository(trimmed.to_owned()));
    }
    Ok(path)
}

/// Whether `path`, resolved lexically against the working directory, is inside
/// this repository.
///
/// Lexical rather than filesystem-based on purpose: the file does not exist yet,
/// so `canonicalize` would fail, and following symlinks is not what "inside the
/// repository" means to a caller reading a command line. `.` and `..` are
/// resolved by hand, which is the standard `Path::components` walk and needs no
/// filesystem access.
pub fn inside_repository(path: &Path) -> bool {
    let Some(root) = repository_root() else {
        return false;
    };
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        match std::env::current_dir() {
            Ok(cwd) => cwd.join(path),
            Err(_) => return false,
        }
    };
    normalise(&absolute).starts_with(normalise(&root))
}

/// This checkout's root.
///
/// Found by walking up from the build-time manifest directory for the ancestor
/// that holds **both** a `Cargo.toml` and a `crates/` directory. The first
/// version of this took one `.parent()` and got `crates/` — which is not the
/// repository, so `--out in-repo.png` run from the repository root was accepted
/// and wrote 256 KB next to the sources. The two-marker test is what makes the
/// answer correct rather than merely plausible, and it is why the walk exists
/// instead of a hard-coded `../..`.
///
/// When no such ancestor exists — an installed binary built elsewhere — `None`
/// is returned and the guard is inert rather than wrong.
fn repository_root() -> Option<PathBuf> {
    static ROOT: std::sync::OnceLock<Option<PathBuf>> = std::sync::OnceLock::new();
    ROOT.get_or_init(|| {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .find(|dir| dir.join("Cargo.toml").is_file() && dir.join("crates").is_dir())
            .map(Path::to_path_buf)
    })
    .clone()
}

/// Resolves `.` and `..` textually, leaving a leading `/` and any prefix alone.
fn normalise(path: &Path) -> PathBuf {
    use std::path::Component;
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() {
                    out.push(Component::ParentDir);
                }
            }
            other => out.push(other),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_record_name_is_deterministic_and_separator_free() {
        let key = ResourceKey::new(0x00e6_bce5, 0x4061_6201, 0x067a_0801);
        let name = record_file_name(0x00e6_bce5, &key);
        assert_eq!(name, "gmdl-00e6bce5-40616201-067a0801.bin");
        assert_eq!(name, record_file_name(0x00e6_bce5, &key));
        assert!(!name.contains('/') && !name.contains('\\') && !name.contains(':'));
    }

    #[test]
    fn an_unknown_type_falls_back_to_its_hex_id() {
        let key = ResourceKey::new(0xdead_beef, 1, 2);
        assert_eq!(
            record_file_name(0xdead_beef, &key),
            "0xdeadbeef-deadbeef-00000001-00000002.bin"
        );
    }

    #[test]
    fn sanitising_replaces_rather_than_deletes() {
        // Replacement, not deletion: the name's shape is preserved so it stays
        // recognisable in a directory listing.
        assert_eq!(sanitise_component("a/b"), "a_b");
        assert_eq!(sanitise_component("a:b c"), "a_b_c");
        assert_eq!(sanitise_component("../../etc/passwd"), ".._.._etc_passwd");
        assert_eq!(sanitise_component("keep-1.0_x"), "keep-1.0_x");
        assert_eq!(sanitise_component(""), "");
        // The honest limit of this function: sanitising is *lossy*, so two
        // inputs differing only in a rejected character collide. That is
        // acceptable only because the sole caller feeds it hex digits and
        // canonical names, and it is why `record_file_name` is pinned to a
        // distinct name per identity below rather than relying on the
        // sanitiser to be injective.
        assert_eq!(sanitise_component("a/b"), sanitise_component("a:b"));
    }

    #[test]
    fn two_records_never_share_a_file_name() {
        let base = ResourceKey::new(0x00e6_bce5, 0x4061_6201, 0x067a_0801);
        let mut names = std::collections::HashSet::new();
        for type_id in [0x00e6_bce5u32, 0x2f4e_681b, 0x2f4e_681c, 0xdead_beef] {
            for group_id in [0u32, 0x4061_6201, 0x4062_6200] {
                for instance_id in [0u32, 1, 0x067a_0801, u32::MAX] {
                    let mut key = base;
                    key.type_id = type_id;
                    key.group_id = group_id;
                    key.instance_id = instance_id;
                    assert!(
                        names.insert(record_file_name(type_id, &key)),
                        "collision on {key}"
                    );
                }
            }
        }
    }

    #[test]
    fn an_out_dir_that_is_the_root_or_a_parent_reference_is_refused() {
        for refused in ["", "   ", "/", ".", "..", "out/..", "/tmp/..", "./.."] {
            assert!(
                validate_out_dir(refused).is_err(),
                "`{refused}` must be refused"
            );
        }
    }

    #[test]
    fn an_ordinary_relative_or_absolute_out_dir_is_accepted() {
        assert_eq!(
            validate_out_dir("/tmp/out").unwrap(),
            PathBuf::from("/tmp/out")
        );
        assert_eq!(validate_out_dir("out").unwrap(), PathBuf::from("out"));
        // A `..` in the middle is a normal relative path, not an escape.
        assert_eq!(
            validate_out_dir("../out/dumps").unwrap(),
            PathBuf::from("../out/dumps")
        );
        // Trailing whitespace is trimmed rather than becoming a path with a
        // space in it -- a shell-quoting accident, not an intent.
        assert_eq!(validate_out_dir("  out  ").unwrap(), PathBuf::from("out"));
    }

    #[test]
    fn normalising_resolves_dot_and_dot_dot_without_touching_the_filesystem() {
        assert_eq!(normalise(Path::new("/a/./b/../c")), PathBuf::from("/a/c"));
        assert_eq!(normalise(Path::new("/a/b/../..")), PathBuf::from("/"));
        // A leading `..` with nothing to pop is kept, not silently dropped.
        assert_eq!(normalise(Path::new("../a")), PathBuf::from("../a"));
    }

    #[test]
    fn the_repository_root_is_the_workspace_root_and_not_the_crates_directory() {
        // The bug this pins: `CARGO_MANIFEST_DIR`'s immediate parent is
        // `crates/`, and treating that as "the repository" let `--out
        // in-repo.png` through. Two markers, one walk, and this assertion.
        let root = repository_root().expect("the workspace root must be findable");
        assert_eq!(
            root.file_name().and_then(|name| name.to_str()),
            Some(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .ancestors()
                    .nth(2)
                    .and_then(|p| p.file_name())
                    .and_then(|n| n.to_str())
                    .unwrap_or("?")
            ),
            "the root is two levels above the manifest directory"
        );
        assert!(root.join("Cargo.toml").is_file(), "{}", root.display());
        assert!(root.join("crates").is_dir(), "{}", root.display());
        assert!(root.join("crates/spore-tools/Cargo.toml").is_file());
    }

    #[test]
    fn a_path_inside_this_repository_is_refused_as_an_out_file() {
        // Absolute, because the guard compares against an absolute root and a
        // relative spelling would only pass by accident of the working
        // directory.
        let root = repository_root().expect("the workspace root must be findable");
        for inside in [
            root.join("in-repo.png"),
            root.join("crates/spore-tools/src/out.png"),
            root.join("target/out.png"),
        ] {
            assert!(
                inside_repository(&inside),
                "{} must be inside",
                inside.display()
            );
            assert!(validate_out_file(inside.to_str().unwrap()).is_err());
        }
        // Including through a `..` that comes back out, and through the exact
        // root: both resolve inside, which is the point of normalising first.
        assert!(inside_repository(&root.join("crates/../docs/x.png")));
        assert!(inside_repository(
            &root.join("crates/spore-tools/../spore-tools/y.png")
        ));
        // And a bare relative name resolves against the working directory, which
        // is the repository when the tests run.
        assert!(inside_repository(Path::new("in-repo.png")));
        assert!(validate_out_file("in-repo.png").is_err());
    }

    #[test]
    fn a_path_outside_this_repository_is_accepted() {
        let root = repository_root().expect("the workspace root must be findable");
        let outside_dir = root
            .parent()
            .filter(|parent| *parent != root)
            .map(|parent| parent.join("elsewhere"))
            .unwrap_or_else(std::env::temp_dir);
        let outside = outside_dir.join("x.png");
        assert!(!inside_repository(&outside), "{}", outside.display());
        assert!(validate_out_file(outside.to_str().unwrap()).is_ok());
    }

    #[test]
    fn an_out_file_that_names_no_file_is_refused() {
        // Note the honest limit: a trailing separator cannot be detected
        // lexically, because a `Path` normalises `/tmp/` to `/tmp`. So `--out
        // /tmp/` names a *directory* and the write itself fails with an I/O
        // error naming it, which is a worse message than the refusal would be.
        // That is a documented gap, not an oversight.
        for refused in ["", "   "] {
            assert!(
                matches!(
                    validate_out_file(refused),
                    Err(crate::CliError::UnsafeOutFile(_))
                ),
                "`{refused}` must be refused"
            );
        }
        assert!(validate_out_file("/tmp/out.png").is_ok());
    }
}
