//! Where records come from, so the resolver can be tested without a game install.
//!
//! The resolver's correctness is not "does it find records" — that is the
//! database's job — but "does it walk the chain correctly and stop where the
//! data stops". So the record source is a trait with a hermetic in-memory
//! implementation, and the real one reads DBPF packages.
//!
//! Returning `Option` is the point: **absent is `None`, never empty bytes and
//! never a default record.** An empty `Vec` and "no such record" are different
//! facts, and a resolver that cannot tell them apart will eventually invent a
//! part.
//!
//! # Why `Option` is not enough on its own
//!
//! `record` folds three different outcomes into one:
//!
//! * every searched package was asked and holds nothing at that key — a
//!   **miss**, which is a finding about the corpus;
//! * an index entry exists and its bytes would not come out — an **error**;
//! * the source holds no package at all, so nothing was ever asked — not a
//!   finding about anything.
//!
//! A catalogue over the corpus has to tell those apart, so
//! [`RecordSource::probe`] returns [`Probe`] and
//! [`RecordSource::is_searchable`] says whether the source could look at all.
//! Both are **defaulted**, so an implementation written against the one-method
//! trait keeps compiling and keeps its previous meaning: the default `probe`
//! cannot distinguish an error from a miss, and says so by reporting the miss.

use spore_core::ResourceKey;

/// A read-only view of records by key.
pub trait RecordSource {
    /// The record at `key`, or `None` if no such record exists.
    fn record(&self, key: &ResourceKey) -> Option<Vec<u8>>;

    /// Whether this source can look anything up at all.
    ///
    /// `false` means every [`Probe::NoSuchRecord`] from this source is a
    /// statement about the source, not about the corpus. Defaulted to `true` so
    /// that adding the method cannot silently reclassify an existing source as
    /// non-searchable.
    fn is_searchable(&self) -> bool {
        true
    }

    /// The full outcome of a lookup.
    ///
    /// **Consistency requirement.** An implementation must agree with its own
    /// [`RecordSource::record`]: `Probe::Present` exactly when `record` returns
    /// `Some`. `PackageSource::probe` refines only the `None` case, so a caller
    /// that reads through either method sees the same corpus.
    ///
    /// The default cannot tell a miss from an error, so it reports the miss. A
    /// source that can tell them apart overrides this; see [`Probe`].
    fn probe(&self, key: &ResourceKey) -> Probe {
        match self.record(key) {
            Some(bytes) => Probe::Present(bytes),
            None => Probe::NoSuchRecord,
        }
    }
}

/// What a lookup found, with the misses separated from the failures.
///
/// Three outcomes, and the middle one is the one `Option` threw away:
/// **a miss is "we looked and it was not there"**, while
/// [`Probe::Unreadable`] is "it is there and we could not read it". Collapsing
/// them is how "we did not manage to look" turns into "we looked and found
/// nothing".
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Probe {
    /// The record is here and its bytes came out.
    Present(Vec<u8>),
    /// Every searched source was asked and holds no record at this key.
    NoSuchRecord,
    /// An index entry exists for this key, and its payload would not come out.
    ///
    /// Carries the row's raw `compression` word, which is what
    /// `spore_dbpf::extract_record` refuses on for an unsupported value, so a
    /// reader can tell "truncated extent" from "compression this build does not
    /// implement" without re-reading the index.
    Unreadable {
        /// The row's `compression` word, verbatim.
        compression: u16,
    },
}

impl Probe {
    /// The bytes, when the record was read.
    pub fn bytes(&self) -> Option<&[u8]> {
        match self {
            Self::Present(bytes) => Some(bytes),
            _ => None,
        }
    }

    /// Whether the record was read.
    pub fn is_present(&self) -> bool {
        matches!(self, Self::Present(_))
    }

    /// Whether the lookup could not be completed because the record would not
    /// come out. **Not** a miss.
    pub fn is_unreadable(&self) -> bool {
        matches!(self, Self::Unreadable { .. })
    }
}

/// An in-memory source, for tests and for a caller that has already loaded
/// what it needs.
#[derive(Debug, Default, Clone)]
pub struct MemorySource {
    records: Vec<(ResourceKey, Vec<u8>)>,
}

impl MemorySource {
    /// An empty source. Every resolution stops at the first hop.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a record, replacing any record already at the key.
    pub fn insert(&mut self, key: ResourceKey, bytes: Vec<u8>) -> &mut Self {
        match self.records.iter_mut().find(|(have, _)| *have == key) {
            Some(slot) => slot.1 = bytes,
            None => self.records.push((key, bytes)),
        }
        self
    }

    /// Whether a record exists at `key`, without reading it.
    pub fn contains(&self, key: &ResourceKey) -> bool {
        self.records.iter().any(|(have, _)| have == key)
    }

    /// How many records are held.
    pub fn len(&self) -> usize {
        self.records.len()
    }

    /// Whether the source holds nothing.
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }
}

impl RecordSource for MemorySource {
    fn record(&self, key: &ResourceKey) -> Option<Vec<u8>> {
        self.records
            .iter()
            .find(|(have, _)| have == key)
            .map(|(_, bytes)| bytes.clone())
    }
}
