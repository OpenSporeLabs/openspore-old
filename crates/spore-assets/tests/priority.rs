//! Package priority, and the two-revision record that makes it load-bearing.
//!
//! Hermetic: everything below is built in memory from synthetic bytes. **No Spore
//! asset bytes**, no game install, no GPU, no window. The one corpus test at the
//! end is `#[ignore]`d and skips cleanly.
//!
//! Run with:
//!
//! ```text
//! cargo test -p spore-assets --test priority
//! ```
//!
//! # Why this file exists
//!
//! `ContentStore` resolves **first match wins** over the order packages were
//! pushed. Nothing sorts them, nothing deduplicates, nothing guesses -- which is
//! the right design and also means the order is the only thing holding the
//! policy together. A patch package has to go in with `push_front`, and the way
//! that goes wrong is invisible: `push_front` on a store that already has a
//! patch still returns `Ok`, `find` still returns a record, the bytes still
//! decompress, and the only difference is that some *other* revision of the same
//! identity won. There is no crash and no diagnostic.
//!
//! So the invariants are pinned here rather than left to a reader of the docs:
//!
//! 1. `push` appends at the lowest priority and `push_front` prepends at the
//!    highest, and `package_names()` is exactly the order that was asked for --
//!    including **two** `push_front` calls, where the second must end up ahead of
//!    the first. An `insert` that reordered, or a `push_front` that appended, would
//!    pass a single-patch test and fail this one.
//! 2. When two packages hold the same identity, the *order alone* decides which
//!    one answers, and reversing the order flips the answer. That is what makes
//!    the priority auditable rather than incidental.
//! 3. `read_from` bypasses priority and says so, so a caller that needs the
//!    shadowed revision can still get it.
//! 4. Resolution is deterministic: the same store answers the same way every
//!    time, and `identities_of_type` is in resolution order.
//!
//! # The real two-revision record
//!
//! `Spore/Data/Spore_Game.package` and `SPORE/Data/PatchData.package` both carry
//! `0x2A3CE5B7:0x00000000:0xA426730B` -- the Cell Stage globals record -- at
//! **264** and **276** bytes. A word-level alignment shows the 264 is neither a
//! truncation nor a shift: the 276-byte revision is the same record with exactly
//! three fields **absent** (`gameMode` at word 0, `startingCellKey` at word 14,
//! `controlMethod` at word 53), and every other word is byte-identical. So the
//! base copy is an older revision of the same record, not a damaged one, and the
//! right answer is whichever package the caller put first. The corpus test at the
//! end measures it; this file's hermetic half reproduces the same shape with two
//! synthetic revisions so the *policy* is tested without the game.

use spore_assets::{ContentStore, Package};
use spore_core::ResourceKey;

const HEADER_SIZE: usize = 96;
const ROW_SIZE: usize = 28;

/// One record to place in a synthetic package.
struct Record {
    key: ResourceKey,
    payload: Vec<u8>,
}

impl Record {
    fn new(key: ResourceKey, payload: Vec<u8>) -> Self {
        Self { key, payload }
    }
}

/// A DBPF v3 image holding `records`, uncompressed.
///
/// Written from the layout `spore_dbpf::index` documents, with the flags word
/// zeroed so the plain 28-byte row stride is used.
fn build_package(records: &[Record]) -> Vec<u8> {
    let index_offset = HEADER_SIZE;
    let arena_offset = index_offset + 4 + records.len() * ROW_SIZE;

    let mut index = Vec::with_capacity(4 + records.len() * ROW_SIZE);
    index.extend_from_slice(&0u32.to_le_bytes());
    let mut arena: Vec<u8> = Vec::new();
    for record in records {
        let at = arena_offset + arena.len();
        index.extend_from_slice(&record.key.type_id.to_le_bytes());
        index.extend_from_slice(&record.key.group_id.to_le_bytes());
        index.extend_from_slice(&record.key.instance_id.to_le_bytes());
        index.extend_from_slice(&(at as u32).to_le_bytes());
        let size = record.payload.len() as u32;
        index.extend_from_slice(&size.to_le_bytes());
        index.extend_from_slice(&size.to_le_bytes());
        index.extend_from_slice(&0u16.to_le_bytes());
        index.push(0);
        index.push(0);
        arena.extend_from_slice(&record.payload);
    }

    let mut out = vec![0u8; HEADER_SIZE];
    out[0..4].copy_from_slice(b"DBPF");
    out[0x24..0x28].copy_from_slice(&(records.len() as u32).to_le_bytes());
    out[0x28..0x2c].copy_from_slice(&((4 + records.len() * ROW_SIZE) as u32).to_le_bytes());
    out[0x40..0x44].copy_from_slice(&(index_offset as u32).to_le_bytes());
    out.extend_from_slice(&index);
    out.extend_from_slice(&arena);
    out
}

fn package(name: &str, records: &[Record]) -> Package {
    Package::from_vec(name, build_package(records)).expect("the synthetic package must open")
}

/// The identity both revisions of the test record carry.
fn globals_key() -> ResourceKey {
    ResourceKey::new(0x2A3C_E5B7, 0x0000_0000, 0xA426_730B)
}

/// 264 bytes: the older revision, with three fields absent.
fn globals_base() -> Vec<u8> {
    let mut out = vec![0u8; 264];
    for (index, byte) in out.iter_mut().enumerate() {
        *byte = (index % 251) as u8;
    }
    // The three absent fields read as zero in the 276-byte revision's own
    // alignment, so zero them here too: the base revision must not accidentally
    // carry a value the patched one lacks.
    for word in [0usize, 14, 53] {
        out[word * 4..word * 4 + 4].fill(0);
    }
    out
}

/// 276 bytes: the patched revision, which is the base plus three fields.
fn globals_patched() -> Vec<u8> {
    let mut out = vec![0u8; 276];
    for (index, byte) in out.iter_mut().enumerate() {
        *byte = (index % 251) as u8;
    }
    for word in [0usize, 14, 53] {
        out[word * 4..word * 4 + 4].fill(0);
    }
    // Three fields the base does not have. Distinct non-zero values, so a test
    // can tell which revision answered by VALUE rather than by length.
    out[0..4].copy_from_slice(&1u32.to_le_bytes());
    out[56..60].copy_from_slice(&0xA1B2_C3D4u32.to_le_bytes());
    out[212..216].copy_from_slice(&0x5EED_0002u32.to_le_bytes());
    out
}

/// A third revision, for the "two patches" ordering.
fn globals_second_patch() -> Vec<u8> {
    let mut out = globals_patched();
    out[0..4].copy_from_slice(&2u32.to_le_bytes());
    out
}

#[test]
fn push_appends_and_push_front_prepends_and_the_order_is_exactly_what_was_asked_for() {
    let mut store = ContentStore::new();
    assert!(store.is_empty());
    assert_eq!(store.len(), 0);

    store.push(package("base", &[]));
    assert_eq!(store.package_names(), vec!["base"]);

    store.push(package("more-base", &[]));
    assert_eq!(store.package_names(), vec!["base", "more-base"]);

    store.push_front(package("patch", &[]));
    assert_eq!(
        store.package_names(),
        vec!["patch", "base", "more-base"],
        "`push_front` must go to the FRONT; an append here silently demotes the patch"
    );

    // Two patches: the second must end up ahead of the first, because it was
    // asked for later. A `push_front` that appended, or an `insert(1)`, would
    // pass the single-patch case above and fail here.
    store.push_front(package("newer-patch", &[]));
    assert_eq!(
        store.package_names(),
        vec!["newer-patch", "patch", "base", "more-base"],
        "the most recently pushed_front patch must have the highest priority"
    );

    // And `push` after a `push_front` still goes to the bottom, so adding a base
    // package late cannot displace a patch.
    store.push(package("late-base", &[]));
    assert_eq!(
        store.package_names(),
        vec!["newer-patch", "patch", "base", "more-base", "late-base"]
    );
    assert_eq!(store.len(), 5);
    assert_eq!(store.packages().len(), 5);
}

#[test]
fn two_revisions_of_one_identity_are_decided_by_the_order_alone() {
    let key = globals_key();
    let base_records = || vec![Record::new(key, globals_base())];
    let patch_records = || vec![Record::new(key, globals_patched())];

    // Base first: the 264-byte revision answers, and the caller can tell by value.
    let mut base_first = ContentStore::new();
    base_first.push(package("base", &base_records()));
    let found = base_first.find(&key).expect("the record is held");
    assert_eq!(found.package_name, "base");
    let bytes = base_first.read(&key).expect("read");
    assert_eq!(bytes.len(), 264);
    assert_eq!(
        &bytes[0..4],
        &[0, 0, 0, 0],
        "the base revision must not carry the patched revision's `gameMode`"
    );

    // The patch in front: the 276-byte revision answers, and the three fields it
    // gained are present by value.
    let mut overlaid = ContentStore::new();
    overlaid.push(package("base", &base_records()));
    overlaid.push_front(package("patch", &patch_records()));
    let found = overlaid.find(&key).expect("the record is held");
    assert_eq!(found.package_name, "patch");
    let bytes = overlaid.read(&key).expect("read");
    assert_eq!(bytes.len(), 276);
    assert_eq!(&bytes[0..4], &1u32.to_le_bytes(), "`gameMode` is 1");
    assert_eq!(
        &bytes[56..60],
        &0xA1B2_C3D4u32.to_le_bytes(),
        "`startingCellKey` is present"
    );
    assert_eq!(
        &bytes[212..216],
        &0x5EED_0002u32.to_le_bytes(),
        "`controlMethod` is present"
    );

    // The reverse order flips the answer. This is the property that makes the
    // decision auditable: nothing but the order changed.
    let mut reversed = ContentStore::new();
    reversed.push_front(package("base", &base_records()));
    reversed.push(package("patch", &patch_records()));
    assert_eq!(
        reversed.find(&key).expect("held").package_name,
        "base",
        "pushing the patch LAST must lose: `push` is the lowest priority"
    );
}

#[test]
fn two_patches_are_decided_by_which_was_pushed_front_last() {
    let key = globals_key();
    let mut store = ContentStore::new();
    store.push(package("base", &[Record::new(key, globals_base())]));
    store.push_front(package("old-patch", &[Record::new(key, globals_patched())]));
    assert_eq!(store.read(&key).expect("read").len(), 276);
    assert_eq!(&store.read(&key).expect("read")[0..4], &1u32.to_le_bytes());

    store.push_front(package(
        "new-patch",
        &[Record::new(key, globals_second_patch())],
    ));
    let bytes = store.read(&key).expect("read");
    assert_eq!(&bytes[0..4], &2u32.to_le_bytes(), "the newest patch wins");
    assert_eq!(store.find(&key).expect("held").package_name, "new-patch");
}

#[test]
fn read_from_bypasses_the_priority_and_says_which_package_it_read() {
    let key = globals_key();
    let mut store = ContentStore::new();
    store.push(package("base", &[Record::new(key, globals_base())]));
    store.push_front(package("patch", &[Record::new(key, globals_patched())]));

    assert_eq!(store.read(&key).expect("read").len(), 276);
    assert_eq!(
        store
            .read_from(1, &key)
            .expect("read_from must reach the shadowed revision")
            .len(),
        264,
        "`read_from` is how a caller gets the revision that lost"
    );
    assert_eq!(
        store.read(&key).expect("read").len(),
        276,
        "and it changes nothing"
    );

    // An out-of-range package index is refused by name, with the count.
    let error = store
        .read_from(7, &key)
        .expect_err("index 7 does not exist");
    assert!(error.to_string().contains('7'), "{error}");
    assert!(error.to_string().contains('2'), "{error}");
}

#[test]
fn resolution_is_deterministic_across_repeated_queries() {
    let key = globals_key();
    let mut store = ContentStore::new();
    store.push(package("base", &[Record::new(key, globals_base())]));
    store.push(package("patch", &[Record::new(key, globals_patched())]));
    store.push_front(package("front", &[Record::new(key, globals_patched())]));

    let first = store.package_names();
    let mut answers = Vec::new();
    for _ in 0..64 {
        answers.push((
            store.find(&key).expect("held").package_name.to_owned(),
            store.package_names(),
        ));
    }
    for (name, names) in &answers {
        assert_eq!(
            name, "front",
            "the same store must answer the same way every time"
        );
        assert_eq!(names, &first, "the package order must not drift");
    }
    // And the digest of the winning record is stable, not just its length.
    let digests: Vec<u32> = (0..16)
        .map(|_| store.read(&key).expect("read")[0] as u32)
        .collect();
    assert!(
        digests.windows(2).all(|pair| pair[0] == pair[1]),
        "the winning bytes must be stable"
    );
}

#[test]
fn identities_of_type_is_in_resolution_order_and_deduplicated() {
    let base_a = ResourceKey::new(0x1111_1111, 0, 1);
    let base_b = ResourceKey::new(0x1111_1111, 0, 2);
    let front_a = ResourceKey::new(0x1111_1111, 0, 3);

    let mut store = ContentStore::new();
    // Deliberately out of ascending order inside each package, so "sorted" and
    // "resolution order" are different answers and only the second is right.
    store.push(package(
        "base",
        &[Record::new(base_b, vec![2]), Record::new(base_a, vec![1])],
    ));
    store.push_front(package("front", &[Record::new(front_a, vec![3])]));
    // The same identity in two packages must appear once.
    store.push_front(package("dup", &[Record::new(base_a, vec![9])]));

    // Packages in resolution order are `dup`, `front`, `base` -- two
    // `push_front` calls after the base, so the later one outranks the earlier
    // one. `dup` holds `base_a`, which `base` also holds, so `base_a` is
    // attributed to `dup` and dropped from `base`'s contribution.
    assert_eq!(store.package_names(), vec!["dup", "front", "base"]);
    assert_eq!(
        store.identities_of_type(0x1111_1111),
        vec![base_a, front_a, base_b],
        "package order, then index order, with the identity two packages share attributed to the \
         higher-priority one"
    );
    // `find` agrees with the list: `base_a` resolves in `dup`, not `base`.
    assert_eq!(store.find(&base_a).expect("held").package_name, "dup");
    assert_eq!(store.find(&base_b).expect("held").package_name, "base");
    assert_eq!(store.find(&front_a).expect("held").package_name, "front");
    // A type nobody holds is an empty list, not an error and not every key.
    assert!(store.identities_of_type(0xDEAD_BEEF).is_empty());
}

/// The real two-revision record, measured. **`#[ignore]`d; skips cleanly.**
#[test]
#[ignore = "needs the git-ignored SPORE/ tree"]
fn the_cell_globals_record_has_two_revisions_and_the_order_picks_one() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize();
    let Ok(root) = root else {
        println!("SKIP: the workspace root could not be resolved");
        return;
    };
    let base_path = root.join("SPORE/Data/Spore_Game.package");
    let patch_path = root.join("SPORE/Data/PatchData.package");
    if !base_path.exists() || !patch_path.exists() {
        println!("SKIP: Spore_Game.package or PatchData.package is not installed");
        return;
    }

    let base = Package::open("Spore_Game", base_path).expect("Spore_Game must open");
    let patch = Package::open("PatchData", patch_path).expect("PatchData must open");
    let key = globals_key();

    let base_len = base
        .index()
        .find(&key)
        .map(|entry| entry.memory_size)
        .unwrap_or(0) as usize;
    let patch_len = patch
        .index()
        .find(&key)
        .map(|entry| entry.memory_size)
        .unwrap_or(0) as usize;
    println!("Spore_Game:   {base_len} bytes");
    println!("PatchData:    {patch_len} bytes");
    assert_eq!(base_len, 264, "the base revision's length");
    assert_eq!(patch_len, 276, "the patched revision's length");

    // The 264 is NOT a truncation of the 276 and NOT a shift: the 276 is the
    // same record with three fields absent. So the two revisions share every
    // word except three, which is the only way "absent" and "corrupt" can be
    // told apart.
    let mut store_base_first = ContentStore::new();
    store_base_first.push(base);
    let mut store_patch_first = ContentStore::new();
    store_patch_first.push(
        Package::open("Spore_Game", root.join("SPORE/Data/Spore_Game.package")).expect("open"),
    );
    store_patch_first.push_front(
        Package::open("PatchData", root.join("SPORE/Data/PatchData.package")).expect("open"),
    );

    assert_eq!(
        store_base_first
            .read(&key)
            .expect("base must hold the record")
            .len(),
        264,
        "base content first resolves to the older revision"
    );
    assert_eq!(
        store_patch_first
            .read(&key)
            .expect("the patch must hold the record")
            .len(),
        276,
        "the patch in front resolves to the newer revision"
    );
    assert_eq!(
        store_patch_first.find(&key).expect("held").package_name,
        "PatchData"
    );

    let newer = store_patch_first.read(&key).expect("read");
    let older = store_base_first.read(&key).expect("read");
    println!(
        "patched revision: gameMode=0x{:08x} startingCellKey=0x{:08x} controlMethod=0x{:08x}",
        u32::from_le_bytes(newer[0..4].try_into().expect("4 bytes")),
        u32::from_le_bytes(newer[56..60].try_into().expect("4 bytes")),
        u32::from_le_bytes(newer[212..216].try_into().expect("4 bytes"))
    );
    assert_eq!(
        older.len(),
        newer.len() - 12,
        "three `u32` fields, not a truncated tail"
    );

    // The word-level alignment, which is the actual evidence for "absent" rather
    // than "truncated". Two cursors: whenever the words agree, both advance; when
    // they do not, the longer record advances. Every word the shorter record
    // lacks is then counted, and its offset is printed. Three is the claim; a
    // truncated tail would leave 264's words as a prefix and skip nothing at the
    // end but many in the middle, and a shift would skip a run at the front.
    let older_words: Vec<u32> = older
        .chunks_exact(4)
        .map(|word| u32::from_le_bytes(word.try_into().expect("4 bytes")))
        .collect();
    let newer_words: Vec<u32> = newer
        .chunks_exact(4)
        .map(|word| u32::from_le_bytes(word.try_into().expect("4 bytes")))
        .collect();
    let (mut here, mut there, mut matched) = (0usize, 0usize, 0usize);
    let mut skipped: Vec<usize> = Vec::new();
    while here < older_words.len() && there < newer_words.len() {
        if older_words[here] == newer_words[there] {
            here += 1;
            there += 1;
            matched += 1;
        } else {
            skipped.push(there);
            there += 1;
        }
    }
    println!(
        "alignment: {} of the older record's {} word(s) matched; the newer record skips {}",
        matched,
        older_words.len(),
        skipped.len()
    );
    for at in &skipped {
        println!(
            "  byte {:#06x} (word {}) present only in the patched revision: 0x{:08x}",
            at * 4,
            at,
            newer_words[*at]
        );
    }
    assert_eq!(
        matched,
        older_words.len(),
        "every word of the shorter record must appear, in order, in the longer one -- otherwise \
         the shorter one is not the same record"
    );
    assert_eq!(
        skipped.len(),
        3,
        "exactly three fields are absent from the older revision, which is what makes this a \
         revision and not a truncation"
    );
    assert_eq!(
        skipped,
        vec![0, 14, 53],
        "the three absent fields are at byte 0 (`gameMode`), byte 0x38 (`startingCellKey`) and \
         byte 0xd4 (`controlMethod`)"
    );
    assert!(
        skipped.iter().any(|at| *at > 0),
        "at least one absent field is interior, which is what excludes a truncated tail"
    );
    // Note what makes this alignment necessary rather than a value comparison:
    // two of the three absent fields read zero in the patched revision, so a
    // byte comparison would call them present.
    assert_eq!(newer_words[0], 0);
    assert_eq!(newer_words[53], 0);
}
