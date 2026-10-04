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

use spore_core::ResourceKey;

/// A read-only view of records by key.
pub trait RecordSource {
    /// The record at `key`, or `None` if no such record exists.
    fn record(&self, key: &ResourceKey) -> Option<Vec<u8>>;
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
