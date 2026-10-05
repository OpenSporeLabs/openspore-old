//! Locating the inputs: installed packages, and the committed tiny fixtures.
//!
//! A missing game install is an ordinary state, not an error. Every
//! constructor here that can come up empty returns an empty value, and every
//! comparison checks [`Corpus::is_present`] before it does anything expensive.
//!
//! # Root resolution
//!
//! `OPENSPORE_ROOT` wins if it is set; otherwise the workspace root is derived
//! from `CARGO_MANIFEST_DIR`, which is how the crate's own tests and the
//! workspace binaries agree on where `tools/spore` and `SPORE/` live without a
//! second configuration mechanism.

use core::fmt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// Environment variable that overrides the workspace root.
pub const ROOT_ENV: &str = "OPENSPORE_ROOT";

/// A `.package` file found on disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocatedPackage {
    /// File name, e.g. `Spore_Content.package`.
    pub name: String,
    /// Absolute path to the file.
    pub path: PathBuf,
    /// Size in bytes, read from the filesystem rather than assumed.
    pub size_bytes: u64,
}

impl fmt::Display for LocatedPackage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} ({} bytes)", self.name, self.size_bytes)
    }
}

/// What a loose record file holds, so a comparison knows which decoder to run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordKind {
    /// An RW4 container.
    Rw4,
    /// A gmdl record.
    Gmdl,
}

/// A standalone record file, outside any package.
///
/// The real corpus has none of these -- every record lives in a package -- but
/// the committed fixtures do, and a comparison that can only run over a package
/// cannot be exercised hermetically.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocatedRecord {
    /// File name, e.g. `mini_rw4.rw4`.
    pub name: String,
    /// Absolute path to the file.
    pub path: PathBuf,
    /// Size in bytes.
    pub size_bytes: u64,
    /// Which decoder applies.
    pub kind: RecordKind,
}

impl fmt::Display for LocatedRecord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} ({:?}, {} bytes)",
            self.name, self.kind, self.size_bytes
        )
    }
}

/// The set of inputs a comparison may run over.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Corpus {
    packages: Vec<LocatedPackage>,
    records: Vec<LocatedRecord>,
    /// The staging directory this corpus copied its fixtures into, if any.
    ///
    /// Owned so it can be removed on drop. Per-call staging is what fixed the
    /// concurrency race, but it also meant every call created a directory that
    /// nothing ever deleted: a reviewer measured 12 accumulated directories on a
    /// machine that had run the suite a handful of times. Bounded and small, and
    /// still a leak -- a test helper that leaves files in `/tmp` on every run is
    /// the same defect class as one that leaves them on failure.
    staging: Option<PathBuf>,
}

impl Drop for Corpus {
    fn drop(&mut self) {
        if let Some(staging) = self.staging.take() {
            let _ = std::fs::remove_dir_all(&staging);
        }
    }
}

impl Corpus {
    /// An empty corpus. Every comparison over it reports `Skipped`.
    #[must_use]
    pub fn empty() -> Self {
        Self::default()
    }

    /// Locates the installed packages under the workspace root.
    ///
    /// Looks in `SPORE/Data/` and `SPORE/DataEP1/` for `*.package`. Returns an
    /// empty corpus -- never an error -- when neither directory exists, because
    /// a machine without the game is a normal machine.
    #[must_use]
    pub fn discover() -> Self {
        Self::from_root(&workspace_root())
    }

    /// Locates the installed packages under an explicit root.
    #[must_use]
    pub fn from_root(root: &Path) -> Self {
        let mut packages = Vec::new();
        for relative in ["SPORE/Data", "SPORE/DataEP1"] {
            let directory = root.join(relative);
            packages.extend(scan_packages(&directory));
        }
        packages.sort_by(|a, b| a.name.cmp(&b.name));
        Self {
            packages,
            records: Vec::new(),
            staging: None,
        }
    }

    /// Locates the installed packages directly under one directory.
    #[must_use]
    pub fn from_dir(directory: &Path) -> Self {
        Self {
            packages: scan_packages(directory),
            records: Vec::new(),
            staging: None,
        }
    }

    /// A corpus built from the committed fixtures, copied into a fresh
    /// temporary directory so nothing in the repository is written to.
    ///
    /// `tests/fixtures/mini_package.dbpf` becomes a package the index and
    /// byte comparisons can walk; `mini_rw4.rw4` and `mini.gmdl` become loose
    /// records, because neither is inside the package and neither is supposed
    /// to be. This is the corpus the hermetic tests run over: it exercises the
    /// same code paths as the real corpus without needing the real corpus.
    ///
    /// Returns an error only when the repository's own fixtures are missing,
    /// which is a broken checkout rather than an ordinary state.
    ///
    /// # The staging directory is per CALL, not per process
    ///
    /// This used to be keyed on the process id alone, which is a real race and
    /// produced a real intermittent failure: cargo runs a crate's tests as
    /// concurrent threads inside ONE process, so every test calling
    /// `synthetic()` shared a staging path. The next caller then executed the
    /// `remove_dir_all` below and deleted the fixtures out from under a sibling
    /// that was still comparing them. The symptom was
    /// `a_fixture_run_produces_no_divergence_except_the_one_the_oracle_gets_wrong`
    /// failing with "the committed fixtures must exist", which reads like a
    /// missing checkout and is nothing of the kind -- the checkout was fine and
    /// the test's own helper had eaten the files.
    ///
    /// A process-unique counter is what distinguishes the calls. It is also why
    /// the counter is an `AtomicU64` and not a plain `static mut`: the tests
    /// that hit this are exactly the ones running in parallel.
    pub fn synthetic() -> Result<Self, CorpusError> {
        static CALL: AtomicU64 = AtomicU64::new(0);
        let fixtures = workspace_root().join("tests").join("fixtures");
        let staging = std::env::temp_dir().join(format!(
            "spore-differential-synthetic-{}-{}",
            std::process::id(),
            CALL.fetch_add(1, Ordering::Relaxed),
        ));
        // A stale directory would silently serve old bytes. Safe now that the path
        // is unique to this call, so it cannot belong to a concurrent one.
        let _ = std::fs::remove_dir_all(&staging);
        std::fs::create_dir_all(&staging).map_err(|source| CorpusError::Io {
            action: format!("create {}", staging.display()),
            source,
        })?;

        let package_source = fixtures.join("mini_package.dbpf");
        let package_path = staging.join("mini_package.package");
        copy(&package_source, &package_path)?;

        let mut records = Vec::new();
        for (file, kind) in [
            ("mini_rw4.rw4", RecordKind::Rw4),
            ("mini.gmdl", RecordKind::Gmdl),
        ] {
            let source = fixtures.join(file);
            let target = staging.join(file);
            copy(&source, &target)?;
            records.push(LocatedRecord {
                name: file.to_owned(),
                size_bytes: size_of(&target)?,
                path: target,
                kind,
            });
        }

        Ok(Self {
            packages: vec![LocatedPackage {
                name: "mini_package.package".to_owned(),
                path: package_path,
                size_bytes: size_of(&staging.join("mini_package.package"))?,
            }],
            records,
            staging: Some(staging),
        })
    }

    /// `true` when at least one input was located.
    ///
    /// This is the single question every comparison asks before it spawns a
    /// subprocess.
    #[must_use]
    pub fn is_present(&self) -> bool {
        !self.packages.is_empty() || !self.records.is_empty()
    }

    /// The located packages, ascending by file name.
    #[must_use]
    pub fn packages(&self) -> &[LocatedPackage] {
        &self.packages
    }

    /// The located standalone records.
    #[must_use]
    pub fn records(&self) -> &[LocatedRecord] {
        &self.records
    }

    /// The first package whose name is exactly `name`.
    #[must_use]
    pub fn find_package(&self, name: &str) -> Option<&LocatedPackage> {
        self.packages.iter().find(|package| package.name == name)
    }

    /// The first standalone record of `kind`.
    #[must_use]
    pub fn first_record_of(&self, kind: RecordKind) -> Option<&LocatedRecord> {
        self.records.iter().find(|record| record.kind == kind)
    }

    /// Total bytes of the located packages.
    #[must_use]
    pub fn total_bytes(&self) -> u64 {
        self.packages
            .iter()
            .map(|package| package.size_bytes)
            .sum::<u64>()
    }
}

impl fmt::Display for Corpus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if !self.is_present() {
            return f.write_str("corpus: empty (no SPORE/ tree found)");
        }
        write!(f, "corpus: {} package(s), ", self.packages.len())?;
        for package in &self.packages {
            write!(f, "{} ", package.name)?;
        }
        write!(f, "/ {} record file(s)", self.records.len())
    }
}

/// Something went wrong locating or staging the corpus.
#[derive(Debug)]
pub enum CorpusError {
    /// A filesystem operation failed.
    Io {
        /// What was being attempted.
        action: String,
        /// The underlying error.
        source: std::io::Error,
    },
    /// A committed fixture this crate depends on is not in the checkout.
    FixtureMissing {
        /// The path expected.
        path: PathBuf,
    },
}

impl fmt::Display for CorpusError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { action, source } => write!(f, "corpus: {action}: {source}"),
            Self::FixtureMissing { path } => {
                write!(f, "corpus: committed fixture {} is missing", path.display())
            }
        }
    }
}

impl std::error::Error for CorpusError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::FixtureMissing { .. } => None,
        }
    }
}

/// The workspace root: `$OPENSPORE_ROOT`, else two levels above this crate.
#[must_use]
pub fn workspace_root() -> PathBuf {
    if let Some(root) = std::env::var_os(ROOT_ENV) {
        return PathBuf::from(root);
    }
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .to_path_buf()
}

/// Every `*.package` directly inside `directory`, ascending by file name.
fn scan_packages(directory: &Path) -> Vec<LocatedPackage> {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return Vec::new();
    };
    let mut packages: Vec<LocatedPackage> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_file()
                && path
                    .extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("package"))
        })
        .filter_map(|path| {
            let size_bytes = std::fs::metadata(&path).ok()?.len();
            Some(LocatedPackage {
                name: path.file_name()?.to_string_lossy().into_owned(),
                path,
                size_bytes,
            })
        })
        .collect();
    packages.sort_by(|a, b| a.name.cmp(&b.name));
    packages
}

fn copy(from: &Path, to: &Path) -> Result<(), CorpusError> {
    if !from.is_file() {
        return Err(CorpusError::FixtureMissing {
            path: from.to_path_buf(),
        });
    }
    std::fs::copy(from, to).map_err(|source| CorpusError::Io {
        action: format!("copy {} -> {}", from.display(), to.display()),
        source,
    })?;
    Ok(())
}

fn size_of(path: &Path) -> Result<u64, CorpusError> {
    std::fs::metadata(path)
        .map(|meta| meta.len())
        .map_err(|source| CorpusError::Io {
            action: format!("stat {}", path.display()),
            source,
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_root_without_a_spore_tree_yields_an_empty_corpus_and_not_an_error() {
        let empty =
            std::env::temp_dir().join(format!("spore-differential-absent-{}", std::process::id()));
        std::fs::create_dir_all(&empty).expect("temp dir");
        let corpus = Corpus::from_root(&empty);
        assert!(!corpus.is_present());
        assert!(corpus.packages().is_empty());
        assert!(corpus.records().is_empty());
        assert_eq!(corpus.total_bytes(), 0);
        assert_eq!(corpus.to_string(), "corpus: empty (no SPORE/ tree found)");
        let _ = std::fs::remove_dir_all(&empty);
    }

    #[test]
    fn a_root_with_packages_locates_them_with_their_real_sizes() {
        let root =
            std::env::temp_dir().join(format!("spore-differential-scan-{}", std::process::id()));
        let data = root.join("SPORE").join("Data");
        std::fs::create_dir_all(&data).expect("temp dir");
        std::fs::write(data.join("B.package"), b"12345").expect("write");
        std::fs::write(data.join("A.package"), b"123").expect("write");
        std::fs::write(data.join("notapackage.txt"), b"1").expect("write");

        let corpus = Corpus::from_root(&root);
        assert!(corpus.is_present());
        assert_eq!(corpus.packages().len(), 2);
        assert_eq!(corpus.packages()[0].name, "A.package");
        assert_eq!(corpus.packages()[0].size_bytes, 3);
        assert_eq!(corpus.packages()[1].name, "B.package");
        assert_eq!(corpus.total_bytes(), 8);
        assert!(corpus.find_package("A.package").is_some());
        assert!(corpus.find_package("C.package").is_none());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_synthetic_corpus_carries_the_package_and_both_loose_records() {
        let corpus = Corpus::synthetic().expect("committed fixtures must exist");
        assert!(corpus.is_present());
        assert_eq!(corpus.packages().len(), 1);
        assert_eq!(corpus.packages()[0].size_bytes, 685);
        assert_eq!(corpus.records().len(), 2);
        assert_eq!(
            corpus
                .first_record_of(RecordKind::Rw4)
                .map(|r| r.name.as_str()),
            Some("mini_rw4.rw4")
        );
        assert_eq!(
            corpus
                .first_record_of(RecordKind::Gmdl)
                .map(|r| r.name.as_str()),
            Some("mini.gmdl")
        );
    }
}
