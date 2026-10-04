//! Real-corpus measurement of everything this crate claims about skeletons,
//! bind poses and animation.
//!
//! **Every test here is `#[ignore]`d and every one skips cleanly when the game is
//! not installed.** They read `SPORE/Data/Spore_Content.package` from the
//! operator's disk; they never copy, embed or commit a Spore byte. Without
//! `SPORE/` they print a skip line and return, so a hermetic checkout runs this
//! file and gets nothing rather than a failure.
//!
//! ```text
//! cargo test -p spore-gmdl --release --test corpus_animation -- --ignored --nocapture
//! ```
//!
//! # What is measured, and how each claim is settled
//!
//! | claim | settled by |
//! |---|---|
//! | the `refCount` count word is big-endian | the count only fits the record when read big-endian |
//! | the `refCount` **key** words are big-endian | index resolution: 7 989/7 989 keys resolve big-endian, 0/7 989 little-endian |
//! | the on-disk key word order | all six permutations tried; exactly one resolves |
//! | `this crate reads it the measured way` | the decoder's own output compared against an independent big-endian read of the raw bytes |
//! | what the references *are* | the targets resolve to `rw4` records whose section directories are read |
//! | whether the gmdl record itself carries skinning | `D3DDECLUSAGE` codes of every vertex descriptor of every gmdl record |
//! | whether the gmdl record itself carries bone ranges | the trailer walk over every record, read together with `TrailerWalk` |
//!
//! The resolver here is deliberately **not** `spore_gmdl`: it reads the count and
//! the three key words straight out of the record bytes. A test that compared
//! the decoder against itself would pass with the bug still in place, which is
//! exactly how this one survived the count-word fix.

use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use spore_core::ResourceKey;
use spore_dbpf::{decompress, DbpfEntry, PackageIndex, COMPRESSION_NONE};
use spore_gmdl::{parse_recovering, DeclUsage, TrailerWalk, GMDL_TYPE};

/// The package every number in this file is measured over.
const PACKAGE: &str = "SPORE/Data/Spore_Content.package";

/// `gmdl` records in that package.
const GMDL_RECORDS: usize = 4_209;

/// Records whose `refCount` word is non-zero.
const RECORDS_WITH_REFS: usize = 1_510;

/// Keys declared across those records.
const DECLARED_KEYS: usize = 7_989;

/// Distinct targets those keys name.
const DISTINCT_TARGETS: usize = 1_006;

/// Every declared key names this type: `rw4`, [`spore_rw4::RW4_TYPE`].
const TARGET_TYPE: u32 = 0x2F4E_681B;

/// The largest `refCount` any record in the corpus declares.
const MAX_REF_COUNT: u32 = 24;

// ---------------------------------------------------------------------------
// Package access
// ---------------------------------------------------------------------------

/// The corpus package's path, or `None` when the game is not installed.
///
/// "The corpus is absent" is a non-finding, not a test failure: this repository's
/// hermetic run must not require a 995 MB game install.
fn corpus_path() -> Option<PathBuf> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .ok()?;
    let path = root.join(PACKAGE);
    if !path.exists() {
        println!("SKIP: {PACKAGE} is not installed");
        return None;
    }
    Some(path)
}

/// A read-only handle over one package: its index, plus per-record extraction.
///
/// The whole 995 MB image is never held in memory. Only the header and the index
/// block are read up front — they are contiguous — and record payloads are read
/// one at a time by extent, so peak memory is the largest single gmdl record.
struct Corpus {
    file: File,
    index: PackageIndex,
}

impl Corpus {
    fn open(path: &Path) -> Self {
        let mut file = File::open(path).expect("package must open");
        let file_len = file.metadata().expect("metadata").len();
        let mut head = [0u8; 0x100];
        file.read_exact(&mut head).expect("package header");
        let word = |at: usize| -> usize {
            u32::from_le_bytes(head[at..at + 4].try_into().expect("four bytes")) as usize
        };
        // Header + index block in one contiguous buffer, which is what the index
        // parser needs: it addresses the index relative to the slice start. The
        // index length is not recorded anywhere, so it is bounded by the widest
        // possible row and then clamped to the file, because a slice that claims
        // more bytes than the package holds is a read error rather than a
        // shorter index.
        let index_offset = word(0x40);
        let wanted = index_offset
            .saturating_add(word(0x24).saturating_mul(28))
            .saturating_add(0x1000);
        let mut block = vec![0u8; wanted.min(file_len as usize)];
        file.seek(SeekFrom::Start(0)).expect("rewind");
        file.read_exact(&mut block).expect("header + index");
        let index = PackageIndex::parse(&block).expect("index must parse");
        Self { file, index }
    }

    fn entries_of_type(&self, type_id: u32) -> Vec<DbpfEntry> {
        self.index.of_type(type_id).copied().collect()
    }

    /// Reads and decompresses one record's payload.
    fn record(&mut self, entry: &DbpfEntry) -> Vec<u8> {
        let mut raw = vec![0u8; entry.stored_size as usize];
        self.file
            .seek(SeekFrom::Start(u64::from(entry.offset)))
            .expect("seek to record");
        self.file.read_exact(&mut raw).expect("record bytes");
        if entry.compression == COMPRESSION_NONE {
            raw
        } else {
            decompress(&raw).expect("QFS payload")
        }
    }
}

/// True when a key names a row of the corpus index.
fn resolves(index: &PackageIndex, key: &ResourceKey) -> bool {
    index.find(key).is_some()
}

// ---------------------------------------------------------------------------
// An independent reader for the refCount table
// ---------------------------------------------------------------------------

/// One record that breaks the ceiling rule: its ranges, the count this test
/// compares against, and the highest slot its vertices actually name.
type Residual = (Vec<(u32, u32)>, u32, u32);

/// One reading of the `refCount` table: the count, and one key per entry.
struct RefTable {
    /// The count word as read.
    count: u32,
    /// One `(instance, group, type)` per declared entry, in record order.
    keys: Vec<(u32, u32, u32)>,
    /// `false` when the declared table does not fit the record, so the read
    /// cannot produce keys at all. This is a *non-finding*, not an empty table.
    fits: bool,
}

/// Reads the `refCount` table out of raw record bytes.
///
/// `big_endian` selects the byte order of **both** the count and the three words
/// of every key. The two orders are not separable in the file: there is no
/// marker, and a record whose count is zero reads as zero either way.
fn read_ref_table(record: &[u8], big_endian: bool) -> RefTable {
    let word = |at: usize| -> Option<u32> {
        let bytes = record.get(at..at + 4)?;
        let fixed: [u8; 4] = bytes.try_into().ok()?;
        Some(if big_endian {
            u32::from_be_bytes(fixed)
        } else {
            u32::from_le_bytes(fixed)
        })
    };
    let Some(count) = word(4) else {
        return RefTable {
            count: 0,
            keys: Vec::new(),
            fits: false,
        };
    };
    let needed = usize::try_from(count)
        .unwrap_or(usize::MAX)
        .saturating_mul(12);
    let fits = 8usize.saturating_add(needed) <= record.len();
    let mut keys = Vec::new();
    if fits {
        for entry in 0..usize::try_from(count).unwrap_or(0) {
            let base = 8 + entry * 12;
            let (Some(instance), Some(group), Some(type_id)) =
                (word(base), word(base + 4), word(base + 8))
            else {
                break;
            };
            keys.push((instance, group, type_id));
        }
    }
    RefTable { count, keys, fits }
}

// ---------------------------------------------------------------------------
// 1. The endianness, settled by index resolution
// ---------------------------------------------------------------------------

#[test]
#[ignore = "needs SPORE/; run with -- --ignored --nocapture"]
fn refcount_keys_resolve_big_endian_and_never_little_endian() {
    let Some(path) = corpus_path() else { return };
    let mut corpus = Corpus::open(&path);
    println!("corpus: {}", path.display());

    let gmdl = corpus.entries_of_type(GMDL_TYPE);
    assert_eq!(gmdl.len(), GMDL_RECORDS, "gmdl record count");

    let mut records_with_refs = 0usize;
    let mut be_keys = 0usize;
    let mut be_resolved = 0usize;
    let mut le_resolved = 0usize;
    let mut le_fits = 0usize;
    let mut le_keys = 0usize;
    let mut distinct: BTreeSet<ResourceKey> = BTreeSet::new();
    let mut targets_by_type: BTreeMap<u32, usize> = BTreeMap::new();
    let mut targets_by_group: BTreeMap<u32, usize> = BTreeMap::new();
    let mut max_count = 0u32;

    for entry in &gmdl {
        let raw = corpus.record(entry);
        let be = read_ref_table(&raw, true);
        let le = read_ref_table(&raw, false);
        max_count = max_count.max(be.count);
        if be.count > 0 {
            records_with_refs += 1;
        }
        // Every key of every record, under both readings.
        for (instance, group, type_id) in &be.keys {
            be_keys += 1;
            let key = ResourceKey::new(*type_id, *group, *instance);
            if resolves(&corpus.index, &key) {
                be_resolved += 1;
            }
            distinct.insert(key);
            *targets_by_type.entry(*type_id).or_default() += 1;
            *targets_by_group.entry(*group).or_default() += 1;
        }
        if le.fits {
            le_fits += 1;
        }
        for (instance, group, type_id) in &le.keys {
            le_keys += 1;
            if resolves(
                &corpus.index,
                &ResourceKey::new(*type_id, *group, *instance),
            ) {
                le_resolved += 1;
            }
        }
    }

    println!("records with at least one ref: {records_with_refs}");
    println!("declared keys (big-endian):     {be_keys}");
    println!("  resolving to an index row:    {be_resolved}");
    println!("distinct targets:               {}", distinct.len());
    println!("targets by type:                {targets_by_type:#x?}");
    println!("targets by group:               {targets_by_group:#x?}");
    println!("largest refCount:               {max_count}");
    println!(
        "little-endian reading: {le_keys} keys readable, {le_resolved} resolving \
         (the table only fits the record on {le_fits} of {} records)",
        gmdl.len()
    );

    assert_eq!(records_with_refs, RECORDS_WITH_REFS);
    assert_eq!(be_keys, DECLARED_KEYS);
    assert_eq!(
        be_resolved, DECLARED_KEYS,
        "every declared key must name a real index row when read big-endian"
    );
    assert_eq!(distinct.len(), DISTINCT_TARGETS);
    assert_eq!(targets_by_type.len(), 1, "exactly one target type");
    assert_eq!(
        targets_by_type.get(&TARGET_TYPE),
        Some(&DECLARED_KEYS),
        "every declared key names rw4"
    );
    assert_eq!(max_count, MAX_REF_COUNT);
    assert_eq!(
        le_resolved, 0,
        "no key may resolve under the little-endian reading; {le_resolved} did"
    );
}

#[test]
#[ignore = "needs SPORE/; run with -- --ignored --nocapture"]
fn the_on_disk_key_word_order_is_instance_group_type_and_nothing_else() {
    // Six orderings, one winner. Without this, "the order is
    // {instance, group, type}" is a convention; with it, it is the only
    // assignment under which the keys address anything.
    let Some(path) = corpus_path() else { return };
    let mut corpus = Corpus::open(&path);
    let gmdl = corpus.entries_of_type(GMDL_TYPE);
    const PERMUTATIONS: [[usize; 3]; 6] = [
        [0, 1, 2],
        [0, 2, 1],
        [1, 0, 2],
        [1, 2, 0],
        [2, 0, 1],
        [2, 1, 0],
    ];
    let mut hits = [0usize; 6];
    let mut entries = 0usize;
    for entry in &gmdl {
        let raw = corpus.record(entry);
        for (instance, group, type_id) in read_ref_table(&raw, true).keys {
            let words = [instance, group, type_id];
            entries += 1;
            for (slot, permutation) in PERMUTATIONS.iter().enumerate() {
                let key = ResourceKey::new(
                    words[permutation[0]],
                    words[permutation[1]],
                    words[permutation[2]],
                );
                if resolves(&corpus.index, &key) {
                    hits[slot] += 1;
                }
            }
        }
    }
    for (slot, permutation) in PERMUTATIONS.iter().enumerate() {
        println!("permutation {permutation:?}: {}/{}", hits[slot], entries);
    }
    assert_eq!(entries, DECLARED_KEYS);
    assert_eq!(
        hits,
        [0, 0, 0, 0, 0, DECLARED_KEYS],
        "exactly one word order resolves, and it is (word0, word1, word2) = \
         (instance, group, type)"
    );
}

#[test]
#[ignore = "needs SPORE/; run with -- --ignored --nocapture"]
fn this_crate_reads_the_table_the_way_the_corpus_says_it_is_written() {
    // The decoder is compared against an independent big-endian read of the raw
    // bytes. Had this compared the decoder against itself it would have passed
    // with the little-endian bug still in place.
    let Some(path) = corpus_path() else { return };
    let mut corpus = Corpus::open(&path);
    let gmdl = corpus.entries_of_type(GMDL_TYPE);
    let mut records = 0usize;
    let mut keys = 0usize;
    for entry in &gmdl {
        let raw = corpus.record(entry);
        let expected = read_ref_table(&raw, true);
        let parsed = parse_recovering(&raw).expect("every gmdl record walks its geometry");
        let got: Vec<ResourceKey> = parsed
            .model
            .referenced_files
            .iter()
            .map(|key| ResourceKey::new(key.type_id, key.group_id, key.instance_id))
            .collect();
        let want: Vec<ResourceKey> = expected
            .keys
            .iter()
            .map(|(instance, group, type_id)| ResourceKey::new(*type_id, *group, *instance))
            .collect();
        assert_eq!(got, want, "decoder disagrees with the raw big-endian read");
        records += 1;
        keys += got.len();
    }
    println!("{records} records, {keys} keys, decoder == raw big-endian read");
    assert_eq!(records, GMDL_RECORDS);
    assert_eq!(keys, DECLARED_KEYS);
}

// ---------------------------------------------------------------------------
// 2. What the gmdl record itself says about skinning
// ---------------------------------------------------------------------------

/// Every `(usage_code, decl_type)` pair seen in a gmdl vertex descriptor, and how
/// often.
type UsageCensus = BTreeMap<(u8, u8), usize>;

#[test]
#[ignore = "needs SPORE/; run with -- --ignored --nocapture"]
fn skinning_usage_census() {
    // `D3DDECLUSAGE` 1 is BLENDWEIGHT and 2 is BLENDINDICES — the Direct3D 9
    // skinning pair, transcribed into `spore_gmdl::DeclUsage`. This is the census
    // that says a gmdl record carries the *bind side* of a skeleton: which joints
    // each vertex names, and with what weight. It does not say the joints are
    // posed; nothing in the record poses them.
    let Some(path) = corpus_path() else { return };
    let mut corpus = Corpus::open(&path);
    let gmdl = corpus.entries_of_type(GMDL_TYPE);
    let mut census: UsageCensus = BTreeMap::new();
    let mut usage_totals: BTreeMap<u8, usize> = BTreeMap::new();
    let mut type_code_by_usage: BTreeMap<u8, BTreeMap<u32, usize>> = BTreeMap::new();
    let mut descriptors = 0usize;
    let mut records_with_indices = 0usize;
    let mut both = 0usize;
    let mut indices_only = 0usize;
    let mut weights_only = 0usize;
    let mut records_indices_only = 0usize;
    let mut nonzero_method = 0usize;
    let mut nonzero_stream = 0usize;
    let mut usage_indices: BTreeSet<u8> = BTreeSet::new();
    let mut shapes: BTreeMap<String, usize> = BTreeMap::new();

    for entry in &gmdl {
        let raw = corpus.record(entry);
        let Ok(parsed) = parse_recovering(&raw) else {
            continue;
        };
        let mut record_indices = false;
        let mut record_weights = false;
        for descriptor in &parsed.model.descriptors {
            descriptors += 1;
            let mut shape: Vec<String> = Vec::new();
            let mut has_weights = false;
            let mut has_indices = false;
            for element in descriptor {
                *census
                    .entry((element.decl_usage, element.decl_type))
                    .or_default() += 1;
                *usage_totals.entry(element.decl_usage).or_default() += 1;
                *type_code_by_usage
                    .entry(element.decl_usage)
                    .or_default()
                    .entry(element.type_code)
                    .or_default() += 1;
                if element.decl_method != 0 {
                    nonzero_method += 1;
                }
                if element.stream != 0 {
                    nonzero_stream += 1;
                }
                usage_indices.insert(element.usage_index);
                match DeclUsage::from_code(element.decl_usage) {
                    Some(DeclUsage::BlendWeight) => {
                        has_weights = true;
                        record_weights = true;
                    }
                    Some(DeclUsage::BlendIndices) => {
                        has_indices = true;
                        record_indices = true;
                    }
                    _ => {}
                }
                let name = DeclUsage::from_code(element.decl_usage)
                    .map_or("undocumented", DeclUsage::as_str);
                shape.push(format!("{name}@{}:{}", element.offset, element.decl_type));
            }
            match (has_weights, has_indices) {
                (true, true) => both += 1,
                (false, true) => indices_only += 1,
                (true, false) => weights_only += 1,
                (false, false) => {}
            }
            *shapes.entry(shape.join(" ")).or_default() += 1;
        }
        if record_indices {
            records_with_indices += 1;
        }
        if record_indices && !record_weights {
            records_indices_only += 1;
        }
    }

    println!("vertex descriptors: {descriptors}");
    println!("usage totals (code -> count): {usage_totals:#x?}");
    println!("(usage, declType) pairs:");
    for ((usage, decl), count) in &census {
        let name = DeclUsage::from_code(*usage).map_or("undocumented", DeclUsage::as_str);
        println!("   usage {usage} ({name}) declType {decl} -> {count}");
    }
    println!("type_code per usage: {type_code_by_usage:#x?}");
    println!("descriptors: both={both} indices-only={indices_only} weights-only={weights_only}");
    println!("records declaring BLENDINDICES: {records_with_indices} (of which without any weight: {records_indices_only})");
    println!("usage_index values seen: {usage_indices:?}; decl_method!=0: {nonzero_method}; stream!=0: {nonzero_stream}");
    println!("distinct descriptor shapes: {}", shapes.len());
    for (shape, count) in &shapes {
        println!("   {count:>5}  {shape}");
    }

    assert_eq!(descriptors, 5_945);
    assert_eq!(both, 1_330);
    assert_eq!(indices_only, 829);
    assert_eq!(
        weights_only, 0,
        "no descriptor declares a weight without indices"
    );
    assert_eq!(records_with_indices, 2_109);
    assert_eq!(records_indices_only, 812);
    assert_eq!(
        shapes.len(),
        8,
        "the corpus has exactly eight descriptor shapes"
    );
    assert_eq!(
        nonzero_method, 0,
        "a non-default declMethod would change how an element is transformed"
    );
    assert_eq!(nonzero_stream, 0, "only stream 0 is read by this crate");
    assert_eq!(
        usage_indices,
        BTreeSet::from([0u8]),
        "one skinning set per vertex"
    );
    // BLENDWEIGHT is always UBYTE4N and BLENDINDICES always UBYTE4: four joints
    // per vertex, four bytes each. Asserted as an equality, not a range, so a new
    // record with a fifth influence has to be re-examined rather than absorbed.
    assert_eq!(census.get(&(1, 8)), Some(&1_330), "BLENDWEIGHT is UBYTE4N");
    assert_eq!(census.get(&(2, 5)), Some(&2_159), "BLENDINDICES is UBYTE4");
    assert_eq!(census.get(&(1, 5)), None, "no other weight encoding");
    assert_eq!(census.get(&(2, 8)), None, "no other index encoding");
    // One type_code per usage across the whole corpus.
    for (usage, codes) in &type_code_by_usage {
        assert_eq!(
            codes.len(),
            1,
            "usage {usage} has more than one type_code: {codes:?}"
        );
    }
    assert_eq!(type_code_by_usage[&0][&0], 5_945, "POSITION -> 0");
    assert_eq!(type_code_by_usage[&5][&6], 5_945, "TEXCOORD -> 6");
    assert_eq!(type_code_by_usage[&3][&2], 5_945, "NORMAL -> 2");
    assert_eq!(type_code_by_usage[&2][&14], 2_159, "BLENDINDICES -> 14");
    assert_eq!(type_code_by_usage[&1][&15], 1_330, "BLENDWEIGHT -> 15");
    assert_eq!(type_code_by_usage[&6][&19], 1_841, "TANGENT -> 19");
}

#[test]
#[ignore = "needs SPORE/; run with -- --ignored --nocapture"]
fn every_blend_index_slot_is_a_multiple_of_three() {
    // The load-bearing measurement for the joint table's stride. Over all 2109
    // records that declare BLENDINDICES, not one slot value is anything but a
    // multiple of three, and the gap between consecutive distinct values is
    // three in 48 623 cases against 1163 for every larger gap combined. That is
    // what makes `slot / 3` a reading rather than a guess.
    let Some(path) = corpus_path() else { return };
    let mut corpus = Corpus::open(&path);
    let gmdl = corpus.entries_of_type(GMDL_TYPE);
    let mut records = 0usize;
    let mut offending: Vec<u32> = Vec::new();
    let mut gaps: BTreeMap<u32, usize> = BTreeMap::new();
    let mut max_slot = 0u32;

    for entry in &gmdl {
        let raw = corpus.record(entry);
        let Ok(parsed) = parse_recovering(&raw) else {
            continue;
        };
        let model = parsed.model;
        let mut used: BTreeSet<u32> = BTreeSet::new();
        for buffer in &model.vertex_buffers {
            let Some(descriptor) = model.descriptors.get(buffer.desc_index as usize) else {
                continue;
            };
            let Some(element) = descriptor.iter().find(|element| {
                DeclUsage::from_code(element.decl_usage) == Some(DeclUsage::BlendIndices)
            }) else {
                continue;
            };
            let Ok(stride) = spore_gmdl::vertex_stride(descriptor) else {
                continue;
            };
            let stride = stride as usize;
            if stride == 0 {
                continue;
            }
            for vertex in 0..buffer.bytes.len() / stride {
                let at = vertex * stride + usize::from(element.offset);
                let Some(slots) = buffer.bytes.get(at..at + 4) else {
                    break;
                };
                for slot in slots {
                    used.insert(u32::from(*slot));
                }
            }
        }
        if used.is_empty() {
            continue;
        }
        records += 1;
        offending.extend(used.iter().copied().filter(|slot| slot % 3 != 0));
        max_slot = max_slot.max(used.iter().copied().max().unwrap_or(0));
        let ordered: Vec<u32> = used.iter().copied().collect();
        for pair in ordered.windows(2) {
            let gap = pair[1] - pair[0];
            *gaps.entry(gap).or_default() += 1;
        }
    }

    println!("records with BLENDINDICES values: {records}");
    println!("highest slot value in the corpus: {max_slot}");
    println!("gap between consecutive distinct values: {gaps:?}");
    assert_eq!(records, 2_109);
    assert!(
        offending.is_empty(),
        "{} slot values are not multiples of three, first {offending:?}",
        offending.len()
    );
    let three = gaps.get(&3).copied().unwrap_or(0);
    let other: usize = gaps
        .iter()
        .filter(|(gap, _)| **gap != 3)
        .map(|(_, n)| *n)
        .sum();
    assert_eq!(three, 48_623);
    assert_eq!(other, 1_186);
    assert!(
        three > 40 * other,
        "the three-gap must dominate every other gap combined: {three} vs {other}"
    );
}

#[test]
#[ignore = "needs SPORE/; run with -- --ignored --nocapture"]
fn the_bone_range_span_bounds_the_joint_ids_the_vertices_name() {
    // `slot / 3` is the joint id (the previous test). This asks the only question
    // the record can answer about the joint table's *size*: does the trailer's
    // first bone-range count bound those ids? In 313 of the 315 records that
    // declare ranges it does; in 2 it does not, both by four joints. Every number
    // below is asserted, including the three records whose highest joint sits
    // below the ceiling, because a summary that reported only the tidier figure
    // would be the kind of claim this repository exists to refuse.
    let Some(path) = corpus_path() else { return };
    let mut corpus = Corpus::open(&path);
    let gmdl = corpus.entries_of_type(GMDL_TYPE);
    let mut with_ranges = 0usize;
    let mut within = 0usize;
    let mut outside = 0usize;
    let mut exceptions: Vec<(u32, u32)> = Vec::new();
    let mut at_ceiling = 0usize;
    let mut one_triple_below = 0usize;
    // `(the record's bone ranges, first range count, highest slot)`
    let mut residuals: Vec<Residual> = Vec::new();

    for entry in &gmdl {
        let raw = corpus.record(entry);
        let Ok(parsed) = parse_recovering(&raw) else {
            continue;
        };
        let model = parsed.model;
        if !model.declares_bone_ranges() {
            continue;
        }
        with_ranges += 1;
        let mut used: BTreeSet<u32> = BTreeSet::new();
        for buffer in &model.vertex_buffers {
            let Some(descriptor) = model.descriptors.get(buffer.desc_index as usize) else {
                continue;
            };
            let Some(element) = descriptor.iter().find(|element| {
                DeclUsage::from_code(element.decl_usage) == Some(DeclUsage::BlendIndices)
            }) else {
                continue;
            };
            let Ok(stride) = spore_gmdl::vertex_stride(descriptor) else {
                continue;
            };
            let stride = stride as usize;
            if stride == 0 {
                continue;
            }
            for vertex in 0..buffer.bytes.len() / stride {
                let at = vertex * stride + usize::from(element.offset);
                let Some(slots) = buffer.bytes.get(at..at + 4) else {
                    break;
                };
                for slot in slots {
                    used.insert(u32::from(*slot));
                }
            }
        }
        // The first range's count, not the sum: a record with ranges
        // [(0, 62), (62, 22)] bounds its joint ids at 3*62-3, not 3*84-3.
        let first_count = model.bone_ranges.first().map_or(0, |(_, count)| *count);
        let span = model.bone_range_span();
        let _ = span;
        let Some(highest) = used.iter().copied().max() else {
            continue;
        };
        // The ceiling is 3 * (count - 1): the last slot the joint table can
        // address when it is strided by three.
        match highest {
            slot if slot == 3u32.saturating_mul(first_count.saturating_sub(1)) => at_ceiling += 1,
            slot if slot == 3u32.saturating_mul(first_count.saturating_sub(2)) => {
                one_triple_below += 1
            }
            _ => residuals.push((model.bone_ranges.clone(), first_count, highest)),
        }
        if used.iter().all(|slot| slot / 3 < first_count) {
            within += 1;
        } else {
            outside += 1;
            exceptions.push((first_count, highest / 3));
        }
    }

    println!("records declaring bone ranges: {with_ranges}");
    println!(
        "every joint id below the first range's count: {within}; not: {outside} {exceptions:?}"
    );
    println!(
        "highest slot sits at 3*(count-1) in {at_ceiling} records, at 3*(count-2) in \
         {one_triple_below}"
    );
    assert_eq!(with_ranges, 315);
    assert_eq!(within, 313);
    assert_eq!(outside, 2);
    println!("records whose highest slot sits at neither bound: {residuals:?}");
    assert_eq!(at_ceiling, 285);
    assert_eq!(one_triple_below, 27);
    assert_eq!(residuals.len(), 3);
}

#[test]
#[ignore = "needs SPORE/; run with -- --ignored --nocapture"]
fn the_trailer_walk_over_the_whole_corpus() {
    // The trailer's `boneRangeCount` is a count of `{start, count}` pairs. This
    // records what the corpus actually holds, read together with `TrailerWalk`
    // so "no bone ranges" and "the walk never got there" stay distinguishable.
    let Some(path) = corpus_path() else { return };
    let mut corpus = Corpus::open(&path);
    let gmdl = corpus.entries_of_type(GMDL_TYPE);
    let mut with_ranges = 0usize;
    let mut total_ranges = 0usize;
    let mut complete = 0usize;
    let mut truncated = 0usize;
    let mut max_ranges = 0usize;
    let mut starts: BTreeSet<u32> = BTreeSet::new();
    let mut with_refs = 0usize;
    let mut stopped: BTreeMap<&'static str, usize> = BTreeMap::new();

    for entry in &gmdl {
        let raw = corpus.record(entry);
        let Ok(parsed) = parse_recovering(&raw) else {
            continue;
        };
        let model = parsed.model;
        match model.trailer {
            TrailerWalk::Complete => complete += 1,
            TrailerWalk::Truncated { at } => {
                truncated += 1;
                *stopped.entry(stage_name(at)).or_default() += 1;
            }
        }
        if model.declares_bone_ranges() {
            with_ranges += 1;
            total_ranges += model.bone_ranges.len();
            max_ranges = max_ranges.max(model.bone_ranges.len());
            for (start, _) in &model.bone_ranges {
                starts.insert(*start);
            }
            if !model.referenced_files.is_empty() {
                with_refs += 1;
            }
        }
    }

    println!("trailer complete: {complete}, truncated: {truncated} {stopped:?}");
    println!(
        "records with bone ranges: {with_ranges}, ranges total: {total_ranges}, \
         max per record: {max_ranges}, distinct starts: {}",
        starts.len()
    );
    println!("of those, records that declare any referenced file: {with_refs}");
    assert_eq!(with_ranges, 315);
    assert_eq!(total_ranges, 333);
    assert_eq!(max_ranges, 3);
    assert_eq!(starts.len(), 9);
    assert_eq!(
        with_refs, 44,
        "the bone table is not a pointer into the reference table: most records \
         that declare one reference nothing"
    );
    assert_eq!(complete + truncated, GMDL_RECORDS);
    assert_eq!(complete, 2_438);
    assert_eq!(truncated, 1_771);
    assert_eq!(stopped.get("baked-deform"), Some(&1_719));
    assert_eq!(stopped.get("anim-data"), Some(&52));
}

/// The canonical spelling of a trailer stop, for the census above.
fn stage_name(stage: spore_gmdl::TrailerStage) -> &'static str {
    use spore_gmdl::TrailerStage::{
        AnimData, AnimDataCount, BakedDeform, BoneRangeCount, BoneRanges, UnknownKey,
    };
    match stage {
        BoneRangeCount => "bone-range-count",
        BoneRanges => "bone-ranges",
        AnimDataCount => "anim-data-count",
        AnimData => "anim-data",
        BakedDeform => "baked-deform",
        UnknownKey => "unknown-key",
    }
}

// ---------------------------------------------------------------------------
// 3. What the references point at
// ---------------------------------------------------------------------------

#[test]
#[ignore = "needs SPORE/; run with -- --ignored --nocapture"]
fn every_reference_target_is_an_rw4_container_in_the_same_package() {
    // The type id is settled by resolution, not by a naming table: all 7 989
    // keys name a row of type 0x2F4E681B, which is `rw4`.
    let Some(path) = corpus_path() else { return };
    let mut corpus = Corpus::open(&path);
    let gmdl = corpus.entries_of_type(GMDL_TYPE);
    let mut targets: BTreeSet<(u32, u32, u32)> = BTreeSet::new();
    for entry in &gmdl {
        let raw = corpus.record(entry);
        for (instance, group, type_id) in read_ref_table(&raw, true).keys {
            let key = ResourceKey::new(type_id, group, instance);
            let found = corpus
                .index
                .find(&key)
                .copied()
                .unwrap_or_else(|| panic!("{key} must resolve"));
            targets.insert((found.type_id, found.group_id, found.instance_id));
        }
    }
    assert_eq!(targets.len(), DISTINCT_TARGETS);
    let by_group: BTreeMap<u32, usize> =
        targets
            .iter()
            .fold(BTreeMap::new(), |mut acc, (_, group, _)| {
                *acc.entry(*group).or_default() += 1;
                acc
            });
    println!("distinct rw4 targets by group: {by_group:#x?}");
    for (type_id, _, _) in &targets {
        assert_eq!(*type_id, TARGET_TYPE);
    }
}

#[test]
#[ignore = "needs SPORE/; run with -- --ignored --nocapture"]
fn the_referenced_containers_carry_skeleton_and_animations_sections() {
    // The section type codes are read out of the RW4 directory. The *names* the
    // reference tables attach to them are a different claim, and the repository
    // holds two tables that disagree (spore-rw4's `cpp_type_name` says
    // 0x70001 = KeyframeAnim, 0x70002 = Skeleton, 0x70003 = AnimationSkin,
    // 0xFF0001 = Animations; its `python_type_name` spells the same codes
    // differently). This test asserts the CODES and prints the names, so the
    // naming disagreement stays visible instead of being resolved by fiat.
    let Some(path) = corpus_path() else { return };
    let mut corpus = Corpus::open(&path);
    let gmdl = corpus.entries_of_type(GMDL_TYPE);
    let mut targets: BTreeSet<(u32, u32, u32)> = BTreeSet::new();
    for entry in &gmdl {
        let raw = corpus.record(entry);
        for (instance, group, type_id) in read_ref_table(&raw, true).keys {
            targets.insert((type_id, group, instance));
        }
    }
    let mut by_group: BTreeMap<u32, BTreeMap<u32, usize>> = BTreeMap::new();
    let mut per_record: BTreeMap<u32, usize> = BTreeMap::new();
    let mut unparsed = 0usize;
    for (type_id, group, instance) in &targets {
        let key = ResourceKey::new(*type_id, *group, *instance);
        let Some(row) = corpus.index.find(&key).copied() else {
            continue;
        };
        let raw = corpus.record(&row);
        let Ok(rw4) = spore_rw4::parse(&raw) else {
            unparsed += 1;
            continue;
        };
        let group_map = by_group.entry(*group).or_default();
        let mut codes: Vec<u32> = rw4.sections.iter().map(|s| s.type_code).collect();
        codes.sort_unstable();
        codes.dedup();
        *per_record.entry(rw4.file_type).or_default() += 1;
        for code in codes {
            *group_map.entry(code).or_default() += 1;
        }
    }
    println!("rw4 records whose directory did not parse: {unparsed}");
    println!("rw4 file types (0x1C): {per_record:#x?}");
    for (group, codes) in &by_group {
        println!("group 0x{group:08X}:");
        for (code, count) in codes {
            println!(
                "   0x{code:05X}  cpp={:?}  python={:?}  records={count}",
                spore_rw4::cpp_type_name(*code),
                spore_rw4::python_type_name(*code),
            );
        }
    }
    assert_eq!(
        unparsed, 0,
        "every referenced container's directory must parse"
    );
    // The three model-specific groups carry exactly one section code and it is
    // not a skeleton; the shared group carries the animation-bearing codes.
    let shared = by_group
        .get(&0x4160_6100)
        .expect("the shared reference group must be present");
    for code in [0x0007_0001u32, 0x0007_0002, 0x0007_0003, 0x00FF_0001] {
        assert!(
            shared.contains_key(&code),
            "shared group must carry section code 0x{code:05X}"
        );
    }
    for (group, codes) in &by_group {
        if *group == 0x4160_6100 {
            continue;
        }
        assert_eq!(
            codes.len(),
            1,
            "group 0x{group:08X} carries exactly one section code"
        );
        assert!(
            codes.contains_key(&0x0008_0003),
            "group 0x{group:08X} carries 0x80003"
        );
    }
}
