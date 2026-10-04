//! The gmdl reference table: **which byte order the keys are in**.
//!
//! # The fact
//!
//! Every word in a gmdl record is little-endian except the reference-count word
//! at `0x04`, which is **big-endian**. The 12-byte keys that follow it are the
//! open question this file exists to settle, and they are settled by the corpus
//! rather than by taste: the keys name real records in the package's own index,
//! so a reading that resolves is the right reading and a reading that does not
//! is the wrong one. There is no interpretation involved — a key either names a
//! row or it does not.
//!
//! Measured over `Spore_Content.package` (2026-10-04):
//!
//! | reading | declared keys | keys that resolve against the package index |
//! |---|---:|---:|
//! | big-endian | 7 989 | **7 989** |
//! | little-endian | 7 989 | **0** |
//!
//! A little-endian read of the *count* word is worse than a wrong answer: it
//! yields `N * 0x0100_0000`, which is larger than the record, so the walk stops
//! before reading a single key. That arm is measured too, so the two failures
//! stay distinguishable instead of being one "it doesn't work".
//!
//! # Why this is a test and not a comment
//!
//! Two crates in this workspace read this table and they once disagreed:
//! `spore-assets`'s graph read it big-endian and `spore-gmdl` read it
//! little-endian. It is now big-endian in both, and this file is what says so.
//! A byte-swapped key is invisible — it is still a `u32`, still
//! in range, still sorts — so nothing crashes and every downstream edge silently
//! points at a record that does not exist. The corpus count is the only thing
//! that catches it, and it has to be asserted somewhere or it will be re-broken.
//!
//! The hermetic half uses the committed fixture `tests/fixtures/mini.gmdl`, whose
//! one reference is `(instance 0x11111111, group 0x22222222, type 0x00e6bce5)`.
//! Two of the three words are byte-symmetric and one is not, which is exactly
//! what a byte-order test needs: the type word `0x00e6bce5` reversed is
//! `0xe5bce600`, and only the correct reading names the gmdl type id.

use std::path::PathBuf;

use spore_assets::{ContentStore, Package};
use spore_core::record::type_id;
use spore_core::ResourceKey;

/// The package every corpus number in this file is measured over.
const PACKAGE: &str = "SPORE/Data/Spore_Content.package";

/// The committed synthetic gmdl fixture. Two of its three reference words are
/// byte-symmetric; the type word is not.
const FIXTURE: &str = "tests/fixtures/mini.gmdl";

/// Bytes in one reference key: `{instance, group, type}`, in that on-disk order.
const KEY_BYTES: usize = 12;

/// Declared references across the gmdl records of `Spore_Content.package`.
const DECLARED: usize = 7_989;

/// Gmdl records in that package, measured 2026-10-04.
const GMDL_RECORDS: usize = 4_209;

fn le32(data: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(data.get(at..at + 4)?.try_into().ok()?))
}

fn be32(data: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(data.get(at..at + 4)?.try_into().ok()?))
}

/// The reference table under one byte-order hypothesis.
///
/// `count_order` and `key_order` are separate on purpose: they are separate
/// words in the format, and the format gets one of them right and the other
/// wrong often enough that conflating them hides which.
fn reference_table(data: &[u8], count_order: Order, key_order: Order) -> Option<Vec<ResourceKey>> {
    let version = le32(data, 0)?;
    if version != spore_gmdl::SUPPORTED_VERSION {
        return None;
    }
    let count = match count_order {
        Order::BigEndian => be32(data, 4)?,
        Order::LittleEndian => le32(data, 4)?,
    };
    let needed = (count as usize).checked_mul(KEY_BYTES)?;
    if data.len() < 8 + needed {
        return None;
    }
    let mut out = Vec::with_capacity(count as usize);
    for index in 0..count as usize {
        let at = 8 + index * KEY_BYTES;
        let word = |offset: usize| match key_order {
            Order::BigEndian => be32(data, at + offset),
            Order::LittleEndian => le32(data, at + offset),
        };
        // On-disk order is instance, group, type; `ResourceKey` is type, group,
        // instance. The reorder is the whole reason this is three words and not
        // a slice copy.
        out.push(ResourceKey::new(word(8)?, word(4)?, word(0)?));
    }
    Some(out)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Order {
    BigEndian,
    LittleEndian,
}

/// A tally of one byte-order hypothesis over the corpus.
#[derive(Debug, Default, Clone, Copy)]
struct Tally {
    /// Records whose table could be read at all under this hypothesis.
    readable: usize,
    /// Records the hypothesis refused because the count did not fit.
    unreadable: usize,
    /// Keys the records declared in total.
    declared: usize,
    /// Keys that name a row in the package's own index.
    resolved: usize,
}

impl Tally {
    fn absorb(&mut self, keys: Option<Vec<ResourceKey>>, store: &ContentStore) {
        match keys {
            None => self.unreadable += 1,
            Some(keys) => {
                self.readable += 1;
                self.declared += keys.len();
                self.resolved += keys.iter().filter(|key| store.contains(key)).count();
            }
        }
    }
}

fn content_store() -> Option<ContentStore> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .ok()?;
    let path = root.join(PACKAGE);
    if !path.exists() {
        println!("SKIP: {PACKAGE} is not installed");
        return None;
    }
    println!(
        "corpus: {} ({} bytes)",
        path.display(),
        path.metadata().ok()?.len()
    );
    let mut store = ContentStore::new();
    store.push(Package::open("Spore_Content", path).expect("package must open"));
    Some(store)
}

/// The corpus measurement: one hypothesis per byte order, over every gmdl record.
#[test]
#[ignore = "needs the git-ignored SPORE/ tree (~1 GB)"]
fn big_endian_keys_resolve_and_little_endian_keys_do_not() {
    let Some(store) = content_store() else {
        return;
    };
    let package = &store.packages()[0];
    let image = package.bytes();
    let rows: Vec<spore_dbpf::DbpfEntry> = package
        .index()
        .entries()
        .iter()
        .copied()
        .filter(|entry| entry.type_id == type_id::GMDL)
        .collect();
    assert_eq!(
        rows.len(),
        GMDL_RECORDS,
        "the gmdl population of this package is a fact worth seeing change"
    );

    let mut big = Tally::default();
    let mut little = Tally::default();
    let mut little_count = Tally::default();
    for entry in &rows {
        let bytes = spore_dbpf::extract_record(image, entry).expect("every gmdl record extracts");
        big.absorb(
            reference_table(&bytes, Order::BigEndian, Order::BigEndian),
            &store,
        );
        little.absorb(
            reference_table(&bytes, Order::BigEndian, Order::LittleEndian),
            &store,
        );
        // The third arm: a little-endian COUNT, which is the bug that was found
        // first and which produces `N * 0x0100_0000`.
        little_count.absorb(
            reference_table(&bytes, Order::LittleEndian, Order::BigEndian),
            &store,
        );
    }

    println!("big-endian count, big-endian keys:    {big:?}");
    println!("big-endian count, little-endian keys: {little:?}");
    println!("little-endian count, big-endian keys: {little_count:?}");

    assert_eq!(big.declared, DECLARED, "the declared reference total");
    assert_eq!(
        big.resolved, DECLARED,
        "every big-endian key names a row in the package's own index, which is what makes \
         big-endian the right reading and not merely a plausible one"
    );
    assert_eq!(big.unreadable, 0, "every gmdl record's table is readable");

    assert_eq!(
        little.declared, DECLARED,
        "the same records declare the same number of keys either way"
    );
    assert_eq!(
        little.resolved, 0,
        "not one little-endian key names a real record; a byte-swapped key is a valid u32, so \
         this is the failure that never crashes"
    );

    // A little-endian count is a different failure and is kept separate.
    assert_eq!(
        little_count.resolved, 0,
        "a little-endian count also resolves nothing"
    );
    assert!(
        little_count.unreadable > 0,
        "a little-endian count must overrun the record rather than read a plausible number of \
         keys: {little_count:?}"
    );
}

/// The pin on `spore_gmdl` itself: over the real records, the walk must produce
/// the big-endian table.
///
/// `parse_recovering` rather than `parse` on purpose. 619 records stop at the
/// undocumented shader-data id `0x218`, which is a *later* stage than the
/// reference table, so `parse_recovering` still returns the table for every
/// record and the comparison is over the whole population rather than the
/// subset that walks to the end.
///
/// Measured on this checkout (2026-10-04): 7 989 of 7 989 keys resolve and 0
/// records disagree, so `spore-gmdl` reads the table big-endian. This test is
/// the pin that keeps it there: reverting the three `read_u32_be` calls in
/// `Walk::read_referenced_files` makes it fail with a count, not a crash.
///
/// A byte-swapped key is the quietest failure in this format: it is still a
/// `u32`, still in range, and still sorts, so nothing panics and every
/// `gmdl->reference` edge simply dangles on a record that does not exist. That
/// is why the assertion is a resolution count against the package's own index
/// and not a comparison of two readings of the same bytes.
#[test]
#[ignore = "needs the git-ignored SPORE/ tree (~1 GB)"]
fn spore_gmdl_reads_the_reference_table_big_endian_over_the_real_corpus() {
    let Some(store) = content_store() else {
        return;
    };
    let package = &store.packages()[0];
    let image = package.bytes();
    let rows: Vec<spore_dbpf::DbpfEntry> = package
        .index()
        .entries()
        .iter()
        .copied()
        .filter(|entry| entry.type_id == type_id::GMDL)
        .collect();

    let mut mismatched = 0usize;
    let mut resolved_by_spore_gmdl = 0usize;
    let mut declared = 0usize;
    let mut first_mismatch: Option<String> = None;
    for entry in &rows {
        let bytes = spore_dbpf::extract_record(image, entry).expect("every gmdl record extracts");
        let expected = reference_table(&bytes, Order::BigEndian, Order::BigEndian)
            .expect("the big-endian table is readable for every record");
        let walked = spore_gmdl::parse_recovering(&bytes)
            .expect("the header walk itself does not fail")
            .model
            .referenced_files;
        declared += expected.len();
        resolved_by_spore_gmdl += walked.iter().filter(|key| store.contains(key)).count();
        if walked != expected {
            mismatched += 1;
            if first_mismatch.is_none() {
                first_mismatch = Some(format!(
                    "{}: big-endian says {:?}, spore_gmdl says {:?}",
                    entry.key, expected, walked
                ));
            }
        }
    }
    println!("declared references: {declared}");
    println!("spore_gmdl keys that resolve against the index: {resolved_by_spore_gmdl}");
    println!("records whose table disagrees: {mismatched}");
    if let Some(first) = &first_mismatch {
        println!("first disagreement: {first}");
    }

    assert_eq!(declared, DECLARED);
    assert_eq!(
        resolved_by_spore_gmdl, DECLARED,
        "spore_gmdl's referenced_files must name real records in the package's own index"
    );
    assert_eq!(
        mismatched, 0,
        "spore_gmdl must read the reference table big-endian"
    );
}

/// The hermetic half: the committed fixture, whose one reference is
/// byte-order-distinguishable, and the decoder that reads it.
#[test]
fn the_committed_fixture_names_the_gmdl_type_only_under_the_big_endian_reading() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("the workspace root must exist");
    let path = root.join(FIXTURE);
    let bytes = std::fs::read(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));

    let big = reference_table(&bytes, Order::BigEndian, Order::BigEndian)
        .expect("the fixture's table is readable big-endian");
    let little = reference_table(&bytes, Order::BigEndian, Order::LittleEndian)
        .expect("the fixture's table is readable little-endian");
    assert_eq!(big.len(), 1, "the fixture declares one reference");
    assert_eq!(little.len(), 1);
    assert_ne!(
        big, little,
        "the fixture must distinguish the two readings, or this file proves nothing"
    );

    // --- THE FIXTURE CANNOT SETTLE THE BYTE ORDER, and that is worth pinning.
    //
    // `tests/fixtures/mini.gmdl` stores its one reference key with the type word
    // little-endian, so the reading that names the gmdl type id here is the
    // LITTLE-endian one -- the opposite of what the corpus says. The fixture was
    // generated from an assumption (it mirrors what the Python oracle reads), not
    // from a real record, so it carries no evidence about Spore's bytes.
    //
    // A test that quietly used this fixture to "confirm" a byte order would
    // confirm the wrong one. So the fixture is asserted to be what it is, and
    // the real question is left to the corpus test below.
    assert_eq!(
        little[0],
        ResourceKey::new(type_id::GMDL, 0x2222_2222, 0x1111_1111),
        "the committed fixture's type word is stored little-endian, so only the little-endian \
         reading names the gmdl type id -- which is a property of the FIXTURE GENERATOR, not \
         evidence about the format"
    );
    assert_eq!(
        big[0].type_id,
        u32::from_le_bytes(type_id::GMDL.to_be_bytes()),
        "and the big-endian reading of the same bytes names 0x{:08x}",
        big[0].type_id
    );
    assert_eq!(big[0].type_id, 0xE5BC_E600);
    assert_ne!(big[0].type_id, type_id::GMDL);
}

/// A truncated table is refused by name in both readings, and an implausible
/// count is refused rather than walked.
#[test]
fn a_truncated_or_implausible_reference_table_is_refused() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("the workspace root must exist");
    let path = root.join(FIXTURE);
    let bytes = std::fs::read(&path).expect("the committed fixture must exist");

    // One byte short of the declared key.
    let mut short = bytes.clone();
    short.truncate(8 + KEY_BYTES - 1);
    assert_eq!(
        reference_table(&short, Order::BigEndian, Order::BigEndian),
        None,
        "a table that does not fit must not be read partially"
    );
    // A count that cannot fit, written as raw bytes so the two readings are
    // genuinely different words rather than one value read two ways.
    let mut absurd_be = bytes.clone();
    absurd_be[4..8].copy_from_slice(&[0x01, 0x00, 0x00, 0x00]); // 16 777 216 big-endian
    assert_eq!(
        reference_table(&absurd_be, Order::BigEndian, Order::BigEndian),
        None
    );
    let mut absurd_le = bytes.clone();
    absurd_le[4..8].copy_from_slice(&[0x00, 0x00, 0x00, 0x01]); // 16 777 216 little-endian
    assert_eq!(
        reference_table(&absurd_le, Order::LittleEndian, Order::BigEndian),
        None
    );
    // A word that is implausible under BOTH readings, so the decoder cannot
    // accidentally succeed by reading the count the other way.
    let mut absurd = bytes.clone();
    absurd[4..8].copy_from_slice(&[0x7F, 0xFF, 0xFF, 0xFF]);
    assert_eq!(
        reference_table(&absurd, Order::BigEndian, Order::BigEndian),
        None
    );
    assert_eq!(
        reference_table(&absurd, Order::LittleEndian, Order::BigEndian),
        None
    );
    // The decoder refuses a count the record cannot hold too.
    assert!(
        spore_gmdl::parse(&absurd).is_err(),
        "a reference count the record cannot hold must be refused"
    );
    assert!(
        spore_gmdl::parse(&short).is_err(),
        "a record whose reference table is cut short must be refused"
    );

    // Zero references is an answer for the TABLE reader -- an empty table, not a
    // failure. It is deliberately not asserted through `spore_gmdl::parse`,
    // because the count word is embedded in the layout: zeroing it on a real
    // record shifts every following field by twelve bytes, so the record stops
    // being the record. Asserting a whole-record decode here would be asserting
    // that a corrupt header is a valid model.
    let mut none = bytes.clone();
    none[4..8].copy_from_slice(&0u32.to_be_bytes());
    assert_eq!(
        reference_table(&none, Order::BigEndian, Order::BigEndian),
        Some(Vec::new())
    );
    assert!(
        spore_gmdl::parse(&none).is_err(),
        "zeroing the count on a record with a reference shifts the rest of its layout, so it \
         must not decode as the same model"
    );
}
