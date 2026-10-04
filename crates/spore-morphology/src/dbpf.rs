//! A [`RecordSource`] over real DBPF packages.
//!
//! # Why the resolver takes a trait
//!
//! Correctness of the resolver is not "did it find the record" — that is the
//! package index's job — but "did it walk the chain correctly, and did it stop
//! where the data stops". So the record source is abstract, and the hermetic
//! tests drive the control flow with synthetic records.
//!
//! # Two packages, one part
//!
//! A part spans packages. The rigblock bounds records live in
//! `Spore_Game.package`; the `prop` pointers and the `0x40606000` pair live in
//! `Spore_Graphics.package`. A source that opened only one of them would report
//! every part unresolved at the prop stage and look like a resolver bug. This
//! type takes a set, and records the searches in the order given.
//!
//! **Absent stays absent.** A key no package holds returns `None` — never empty
//! bytes, never a zero record. That distinction is the whole reason the resolver
//! can tell "no such record" from "record that will not decode", and the two
//! become different [`crate::StopReason`]s.

use std::path::Path;

use spore_core::ResourceKey;
use spore_dbpf::{extract_record, DbpfEntry, PackageIndex};

use crate::source::RecordSource;

/// One opened package.
struct Loaded {
    image: Vec<u8>,
    index: PackageIndex,
}

impl Loaded {
    fn open(path: &Path) -> Option<Self> {
        let image = std::fs::read(path).ok()?;
        let index = PackageIndex::parse(&image).ok()?;
        Some(Self { image, index })
    }

    fn find(&self, key: &ResourceKey) -> Option<&DbpfEntry> {
        self.index.entries().iter().find(|entry| {
            entry.type_id == key.type_id
                && entry.group_id == key.group_id
                && entry.instance_id == key.instance_id
        })
    }
}

/// Reads records from a set of packages, in the order they were added.
///
/// Hand-written `Debug`: a `PackageIndex` over a gigabyte of records is not
/// something to print, and a caller wants the package count, not the contents.
pub struct PackageSource {
    packages: Vec<Loaded>,
    /// How many lookups found nothing, so a caller can prove the misses were
    /// misses rather than a source that was never wired up.
    lookups: std::cell::Cell<usize>,
    misses: std::cell::Cell<usize>,
}

impl Default for PackageSource {
    fn default() -> Self {
        Self {
            packages: Vec::new(),
            lookups: std::cell::Cell::new(0),
            misses: std::cell::Cell::new(0),
        }
    }
}

impl core::fmt::Debug for PackageSource {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let (lookups, misses) = self.counters();
        f.debug_struct("PackageSource")
            .field("packages", &self.packages.len())
            .field("lookups", &lookups)
            .field("misses", &misses)
            .finish()
    }
}

impl PackageSource {
    /// An empty source. Every lookup misses.
    pub fn new() -> Self {
        Self::default()
    }

    /// Opens one package, ignoring a package that is absent or unparseable.
    ///
    /// Ignoring rather than failing is right for a source assembled from an
    /// optional install: a caller that asks "how many parts resolve" gets a
    /// smaller number, not an error it cannot interpret.
    pub fn open(mut self, path: &Path) -> Self {
        if let Some(loaded) = Loaded::open(path) {
            self.packages.push(loaded);
        }
        self
    }

    /// Opens `SPORE/Data/<name>` under `root`, if it is there.
    pub fn open_in(mut self, root: &Path, name: &str) -> Self {
        if let Some(loaded) = Loaded::open(&root.join("SPORE/Data").join(name)) {
            self.packages.push(loaded);
        }
        self
    }

    /// How many packages were actually opened.
    pub fn package_count(&self) -> usize {
        self.packages.len()
    }

    /// Lookups served and lookups that missed.
    pub fn counters(&self) -> (usize, usize) {
        (self.lookups.get(), self.misses.get())
    }

    /// The record at `key` in any opened package.
    pub fn get(&self, key: &ResourceKey) -> Option<Vec<u8>> {
        for package in &self.packages {
            if let Some(entry) = package.find(key) {
                if let Ok(bytes) = extract_record(&package.image, entry) {
                    return Some(bytes);
                }
            }
        }
        None
    }
}

impl RecordSource for PackageSource {
    fn record(&self, key: &ResourceKey) -> Option<Vec<u8>> {
        self.lookups.set(self.lookups.get() + 1);
        let found = self.get(key);
        if found.is_none() {
            self.misses.set(self.misses.get() + 1);
        }
        found
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_source_misses_everything_and_says_so() {
        let source = PackageSource::new();
        let key = ResourceKey::new(1, 2, 3);
        assert!(source.get(&key).is_none());
        assert_eq!(source.package_count(), 0);
    }

    #[test]
    fn opening_an_absent_package_is_not_an_error_and_leaves_the_count_visible() {
        let source = PackageSource::new().open(Path::new("/nonexistent/package.package"));
        assert_eq!(source.package_count(), 0, "the miss must be visible");
        assert!(source.get(&ResourceKey::new(1, 2, 3)).is_none());
    }
}
