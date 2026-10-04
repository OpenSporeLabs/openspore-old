//! Bone identity: an FNV-1 hash, and the names recovered for it.
//!
//! # The finding this module encodes
//!
//! The `Skeleton (0x70002)` section stores, per bone, a 32-bit **identity
//! token**. For a long time those tokens were "2002 distinct opaque words",
//! meaning unknown. They are **FNV-1 hashes of a morph-lattice parameter name**,
//! and this module recovers 244 of them.
//!
//! # FNV-1, not FNV-1a — read this before "fixing" it
//!
//! ```text
//! h = 0x811C9DC5
//! for each byte c of the name:
//!     h = (h * 0x01000193) & 0xFFFFFFFF      <- multiply FIRST
//!     h ^= c                                <- then xor
//! ```
//!
//! This is **FNV-1**. This repository's DBPF instance ids are **FNV-1a**, which
//! xors first and multiplies second. Swapping them yields **zero** matches where
//! the correct order yields hundreds, and it looks like a correct implementation
//! the whole way through — the same trap class as the big-endian `refCount` in
//! GMDL. The two functions are both spelled "FNV" and both are one line apart.
//!
//! # How the attribution was established, and what it is worth
//!
//! Three independent lines, in `docs/analysis/creator/11-bone-identity-vocabulary.md`:
//!
//! 1. `root` hashes to `0x7BD05F71` and occurs 382 times, matching an
//!    independently measured count of 382 taken before any name was guessed.
//! 2. Whole indexed families fall out — `joint1..joint9`, `shaft1..shaft5`,
//!    `base1..6`, `bolt1..4`, `bulb1..4`, `fin1..4`, `top1..4`, `stem0..5`,
//!    `node1..6`. Chance does not produce families.
//! 3. FNV-1 has an algebraic signature: two names sharing a prefix differ only
//!    in the low byte of their hashes, because the multiply term is shared.
//!    That recovered the `joint` and `base` prefixes without being told them.
//!
//! Chance is accounted for explicitly. The exhaustive search covered 62 193 780
//! candidate strings against 2002 targets, which is **29.0 expected accidental
//! collisions**; 244 names were recovered. So a *single* unrepeated name with no
//! corroboration is graded [`Confidence::Unconfirmed`] and a caller who needs to
//! be right should require [`Confidence::Family`] or `Confidence::Literal`.
//!
//! # What this is not
//!
//! * It is **not** a decode of a stored name. No bone name exists anywhere in
//!   the record; the corpus was searched for a name table and there is none.
//! * It is **not** a capability enum. The paper's `caps` are a runtime concept;
//!   what is recovered here is a parameter-name vocabulary.
//! * It is **not** complete: 1758 of 2002 tokens are unnamed, mostly
//!   per-asset names like `CA_shoulders_01`.
//!
//! Naming a bone `hip` is a strong claim about a hash. Calling it *the hip
//! *capability* would be a different claim, and this crate does not make it.

use crate::error::Result;

mod vocabulary_table;

/// The 32-bit identity of one bone, as stored in the record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BoneName(u32);

impl BoneName {
    /// Wraps a raw identity word.
    pub const fn from_raw(raw: u32) -> Self {
        Self(raw)
    }

    /// The raw identity word.
    pub const fn to_raw(self) -> u32 {
        self.0
    }

    /// The recovered name and confidence, if this identity was recovered.
    ///
    /// `None` means **not recovered**, which is not the same as "unnamed" in the
    /// sense of *nameless in the record* — every bone has a name in the
    /// generator's vocabulary; 1758 of those names are simply not in this table
    /// yet. See [`Confidence`].
    pub fn recovered(self) -> Option<(&'static str, Confidence)> {
        vocabulary_table::RECOVERED
            .binary_search_by_key(&self.0, |(identity, ..)| *identity)
            .ok()
            .map(|index| {
                let entry = &vocabulary_table::RECOVERED[index];
                (entry.1, entry.3)
            })
    }

    /// The recovered name, **only** if the evidence is strong.
    ///
    /// This is the accessor to prefer. A caller that draws a creature should not
    /// be able to accidentally print `kiomq` as an anatomical name: that string
    /// is in the table because it hashes to a real token, not because anything
    /// established it is a word.
    pub fn name_if_certain(self) -> Option<&'static str> {
        match self.recovered() {
            Some((name, confidence)) if confidence.is_witnessed() => Some(name),
            _ => None,
        }
    }

    /// How many bones in the measured corpus carry this identity.
    pub fn corpus_frequency(self) -> Option<u32> {
        vocabulary_table::RECOVERED
            .binary_search_by_key(&self.0, |(identity, ..)| *identity)
            .ok()
            .map(|index| vocabulary_table::RECOVERED[index].2)
    }
}

impl core::fmt::Display for BoneName {
    /// Prints the name when it is recovered, and the raw word when it is not.
    ///
    /// An unnamed bone prints as `0x…`, never as a placeholder word.
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.name_if_certain() {
            Some(name) => f.write_str(name),
            None => match self.recovered() {
                Some((name, _)) => write!(f, "{name}?"),
                None => write!(f, "0x{:08x}", self.0),
            },
        }
    }
}

/// How much the corpus supports a recovered name.
///
/// The grades are ordered by what kind of witness stands behind them, and each
/// one names a witness a test can go and check. A grade nobody could verify
/// would be a grade nobody should trust.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Confidence {
    /// At least three siblings of an indexed family were recovered.
    ///
    /// The strongest grade available: a collision cannot manufacture a family.
    Family,
    /// The exact byte string occurs, delimited, in a shipped `.exe`/`.dll`.
    BinaryLiteral,
    /// The exact byte string occurs, delimited, in a decompressed record.
    RecordLiteral,
    /// A single exhaustive-search hit with no sibling and no literal witness.
    ///
    /// Probably a name. Not established. Use [`BoneName::name_if_certain`]
    /// instead of trusting it.
    Unconfirmed,
}

impl Confidence {
    /// Whether a witness outside the hashed vocabulary exists.
    ///
    /// An exhaustive search over 62 million strings against 2002 targets is
    /// expected to collide about 29 times by luck, so a single unrepeated hit
    /// with no external witness is not evidence on its own.
    pub const fn is_witnessed(self) -> bool {
        matches!(
            self,
            Self::Family | Self::BinaryLiteral | Self::RecordLiteral
        )
    }
}

/// One row of the recovered vocabulary.
#[derive(Debug, Clone, Copy)]
pub struct Entry {
    /// The bone identity the row describes.
    pub identity: BoneName,
    /// The recovered name.
    pub name: &'static str,
    /// How many bones in the measured corpus carry it.
    pub corpus_frequency: u32,
    /// How much the corpus supports the attribution.
    pub confidence: Confidence,
}

/// Every recovered identity, in ascending identity order.
///
/// Exposed so a consumer can *audit* the table — check that every row names a
/// token the corpus really contains, or report coverage — without iterating the
/// whole 32-bit identity space.
pub fn entries() -> impl ExactSizeIterator<Item = Entry> {
    vocabulary_table::RECOVERED
        .iter()
        .map(|&(identity, name, corpus_frequency, confidence)| Entry {
            identity: BoneName::from_raw(identity),
            name,
            corpus_frequency,
            confidence,
        })
}

/// The FNV-1 hash of a morph-parameter name.
///
/// Named `fnv1` deliberately, and **not** exported as anything a caller might
/// reach for when hashing a resource id: this is the *name* hash, and it is a
/// different function from the id hash this workspace uses elsewhere.
pub fn fnv1(name: &str) -> u32 {
    let mut hash: u32 = 0x811C_9DC5;
    for byte in name.as_bytes() {
        hash = hash.wrapping_mul(0x0100_0193);
        hash ^= u32::from(*byte);
    }
    hash
}

/// The forward direction: does this name hash to this identity?
///
/// Used by the corpus tests to confirm the table still describes the data, and
/// by a caller that wants to assert "this token is `joint3`" without trusting
/// the table at all.
pub fn identity_of(name: &str) -> Result<BoneName> {
    Ok(BoneName::from_raw(fnv1(name)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_is_fnv1_and_not_fnv1a() {
        // The measured anchors. If this test ever fails, the multiply and the
        // xor have been swapped, and every name in the table is wrong.
        assert_eq!(fnv1("root"), 0x7BD0_5F71);
        assert_eq!(fnv1("joint1"), 0xD0BE_09E0);
        assert_eq!(fnv1("joint3"), 0xD0BE_09E2);
        assert_eq!(fnv1("base1"), 0xC318_DB13);

        // FNV-1a is the *other* order, and is this repo's id hash. It must not
        // reproduce these, or the distinction is not real.
        let mut a: u32 = 0x811C_9DC5;
        for byte in b"root" {
            a ^= u32::from(*byte);
            a = a.wrapping_mul(0x0100_0193);
        }
        assert_ne!(a, 0x7BD0_5F71, "FNV-1a must not agree here");
    }

    #[test]
    fn the_algebraic_signature_of_fnv1_holds() {
        // Two names sharing a prefix differ only in the low byte of the hash,
        // because the multiply term is identical. This is what let the search
        // read suffixes and prefixes straight off the data.
        for (a, b) in [
            ("joint1", "joint3"),
            ("shaft1", "shaft2"),
            ("bolt1", "bolt4"),
        ] {
            assert_eq!(
                fnv1(a) ^ fnv1(b),
                u32::from(a.as_bytes()[a.len() - 1]) ^ u32::from(b.as_bytes()[b.len() - 1]),
                "{a} vs {b} does not share the FNV-1 prefix identity"
            );
        }
    }

    #[test]
    fn every_table_row_reproduces_its_own_key() {
        // This is the guard that makes the generated table trustworthy: a row
        // that does not hash back is a corrupted claim, not a weak one.
        assert!(vocabulary_table::RECOVERED.len() >= 200, "table shrank");
        for (identity, name, _, _) in vocabulary_table::RECOVERED {
            assert_eq!(
                fnv1(name),
                *identity,
                "row {identity:#010x} = {name} does not hash back to its own identity"
            );
        }
    }

    #[test]
    fn the_table_is_sorted_so_the_binary_search_holds() {
        let keys: Vec<u32> = vocabulary_table::RECOVERED
            .iter()
            .map(|(identity, ..)| *identity)
            .collect();
        let mut sorted = keys.clone();
        sorted.sort_unstable();
        assert_eq!(keys, sorted, "the generated table lost its sort order");
    }

    #[test]
    fn a_recovered_name_comes_back_and_an_unknown_one_does_not() {
        let root = identity_of("root").unwrap();
        assert_eq!(root.name_if_certain(), Some("root"));
        assert_eq!(root.corpus_frequency(), Some(382));

        // 0x00000000 is in no table: an unrecovered identity must say so.
        let unknown = BoneName::from_raw(0);
        assert_eq!(unknown.recovered(), None);
        assert_eq!(unknown.name_if_certain(), None);
        assert_eq!(unknown.corpus_frequency(), None);
        assert_eq!(unknown.to_string(), "0x00000000");
    }

    #[test]
    fn a_weak_hit_is_visible_but_not_offered_as_a_name() {
        // `kiomq` hashes to a real token but nothing established it is a word.
        // It must be reachable for auditing and unreachable for drawing.
        let weak = identity_of("kiomq").unwrap();
        assert!(weak.recovered().is_some(), "should be in the table at all");
        assert_eq!(
            weak.name_if_certain(),
            None,
            "must not be offered as a name"
        );
        assert!(
            weak.to_string().ends_with('?'),
            "uncertainty must be visible"
        );
    }

    #[test]
    fn laterality_is_present_as_distinct_bones() {
        // The symmetry finding: no symmetry flag exists anywhere, but left and
        // right are separate named bones. All four pairs are real tokens.
        for stem in ["wing", "side", "hip", "knee"] {
            let left = identity_of(&format!("{stem}L")).unwrap();
            let right = identity_of(&format!("{stem}R")).unwrap();
            assert!(left.recovered().is_some(), "{stem}L is not a corpus bone");
            assert!(right.recovered().is_some(), "{stem}R is not a corpus bone");
            assert_ne!(left, right, "{stem} left and right must be distinct");
        }
    }
}
