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

use crate::source::{Probe, RecordSource};

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

    /// The `compression` word of the index row at `key`, in any opened package.
    ///
    /// Used only to tell "there is no row" from "there is a row whose payload
    /// would not come out". A row whose extraction failed for a non-compression
    /// reason (a truncated extent, a decompressed size mismatch) reports its own
    /// word, because that is the row this source actually holds and a later
    /// reader should be told which one it was.
    fn compression_at(&self, key: &ResourceKey) -> Option<u16> {
        self.packages
            .iter()
            .find_map(|package| package.find(key).map(|entry| entry.compression))
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

    /// A source with no package open cannot look anything up, and every miss it
    /// reports is therefore a statement about the source rather than about the
    /// corpus. Zero packages is the state a caller reaches when it points at an
    /// install that is not there, which is the exact case where "absent" and
    /// "never looked" must not be confused.
    fn is_searchable(&self) -> bool {
        self.package_count() > 0
    }

    /// Separates "no index entry anywhere" from "an index entry whose bytes would
    /// not come out".
    ///
    /// Refines only the `None` case of [`RecordSource::record`], which is why the
    /// two can never disagree: `Present` is returned on exactly the path that
    /// already produced `Some`.
    fn probe(&self, key: &ResourceKey) -> Probe {
        match self.get(key) {
            Some(bytes) => Probe::Present(bytes),
            None => match self.compression_at(key) {
                Some(compression) => Probe::Unreadable { compression },
                None => Probe::NoSuchRecord,
            },
        }
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

    #[test]
    fn a_source_with_no_package_is_not_searchable_and_says_so() {
        // The distinction this module exists for: with no package open, the
        // record was never looked for, so reporting a miss would be a fabricated
        // negative about the corpus.
        let source = PackageSource::new();
        assert!(!source.is_searchable());
        assert!(matches!(
            source.probe(&ResourceKey::new(1, 2, 3)),
            Probe::NoSuchRecord
        ));
    }
}
