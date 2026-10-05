//! Every 32-bit word of the gmdl **animation trailer**, histogrammed against the
//! recovered bone-identity vocabulary.
//!
//! **The question.** `docs/CREATOR-CORE.md` §13.2 names three places the creature
//! anatomy could still live; the first is "gmdl's anim data / baked deforms",
//! because a `gmdl` record is the whole-body layer and its trailer is the only
//! part of it nobody has read. `spore-gmdl` reaches that trailer and reads only
//! the `{start, count}` bone-range pairs out of it, best-effort. If bone identity
//! tokens live there, the trailer words will hit the FNV-1 hash set recovered in
//! `docs/analysis/creator/11-bone-identity-vocabulary.md` well above chance.
//!
//! **The region.** [`spore_gmdl::trailer_region`] — every byte after the
//! validated geometry and material-info sections, i.e. the bone-range table, the
//! `GameModelAnimData` array and the trailing resource key. `strict_consumed` is
//! *not* used to locate the region: on a truncated walk that is where the walk
//! stopped, not where the trailer began, so the two are not interchangeable.
//!
//! **Three separate baselines, because a raw hit count is not a finding.**
//!
//! | baseline | what it answers |
//! |---|---|
//! | `2^32` chance expectation | how many hits an arbitrary word stream produces by luck |
//! | the **whole** 2 002-token universe | is *any* bone identity present, named or not |
//! | DBPF instance ids in the live index | are the hits **pointers** rather than identities |
//!
//! The third is the one that decides this. The trailer is full of resource keys,
//! so a word that happens to be a real index row is a pointer; a word that is a
//! bone token is not. The measurement separates them instead of reporting a hit
//! count and calling it anatomy.
//!
//! **Two name grades, never merged.** `spore_rigblock`'s 244 recovered names split
//! into 170 *witnessed* (`Family` / `BinaryLiteral` / `RecordLiteral`) and 74
//! `Unconfirmed` bare-search hits, and only the witnessed set is load-bearing.
//!
//! `spore-rigblock` is a **dev-dependency** so this file hashes with *its*
//! `fnv1`: a second FNV-1 here would be one transposed line from FNV-1a and would
//! report a clean zero.
//!
//! ```text
//! cargo test -p spore-gmdl --release --test anim_vocabulary -- --ignored --nocapture
//! ```
//!
//! Every test here is `#[ignore]`d and skips cleanly without the game installed,
//! so the hermetic suite runs this file and gets nothing rather than a failure.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use spore_dbpf::{decompress, DbpfEntry, PackageIndex, COMPRESSION_NONE};
use spore_gmdl::{parse_recovering, trailer_region, GMDL_TYPE};
use spore_rigblock::vocabulary::{self, BoneName, Confidence};

/// Whole-body models: where the 4 209 `gmdl` records live.
const CONTENT_PACKAGE: &str = "SPORE/Data/Spore_Content.package";

/// Part-layer models: where the bone-identity tokens live.
const GRAPHICS_PACKAGE: &str = "SPORE/Data/Spore_Graphics.package";

/// `gmdl` records in [`CONTENT_PACKAGE`]: the honest denominator for the
/// whole-body layer.
const GMDL_RECORDS: usize = 4_209;

/// Records whose material info names a shader-data id this crate cannot size,
/// and which therefore have **no located trailer** — every one of the
/// creature-model group `0x40627100` among them.
const UNLOCATED_RECORDS: usize = 619;

/// The group whose `rw4` records carry `Skeleton (0x70002)`.
const PART_GROUP: u32 = 0x4060_6000;

/// `rw4`, the container type the part layer's skeletons are.
const RW4_TYPE: u32 = 0x2F4E_681B;

/// The skeleton section's type code.
const SKELETON_TC: u32 = 0x0007_0002;

/// The corpus facts from report 11 §0, asserted before they are used as a
/// denominator. A universe that is not these is a universe that was rebuilt
/// wrong, and every ratio computed against it would be meaningless.
const UNIVERSE_GROUP_ROWS: usize = 1_824;
const UNIVERSE_SKELETONS: usize = 6_270;
const UNIVERSE_TOKENS: usize = 2_002;
const UNIVERSE_VALUES: usize = 15_763;

/// `rw4` records in [`PART_GROUP`] carrying at least one **size-validated**
/// `Skeleton (0x70002)` section.
///
/// Not [`UNIVERSE_GROUP_ROWS`], and the difference is a different question rather
/// than a disagreement. Report 11 counted every index row it iterated in the
/// group; this file counts rows whose `0x70002` payload matched the
/// `0x18 + 12 * N` shape before any of it was read. The three counts that carry
/// the identity universe — skeletons, distinct tokens, total bone values — match
/// report 11 exactly.
const UNIVERSE_RECORDS_WITH_SKELETON: usize = 1_568;

/// The recovered names the question names outright, as `(name, hash)`.
///
/// Transcribed rather than looked up so that the check is against
/// `spore_rigblock`'s **hash function**, not against its table: if a name's hash
/// moved, this list is what notices.
const ANATOMY: [(&str, u32); 17] = [
    ("root", 0x7BD0_5F71),
    ("body", 0x6DFE_676D),
    ("hip", 0x428A_E404),
    ("hipL", 0xC4A4_F200),
    ("hipR", 0xC4A4_F21E),
    ("knee", 0x9004_000C),
    ("kneeL", 0xC24C_12A8),
    ("kneeR", 0xC24C_12B6),
    ("eye", 0x3693_2030),
    ("iris", 0x68D0_ED08),
    ("jaw", 0x2E86_47BD),
    ("shoulder", 0x8B67_25F9),
    ("head", 0xCD88_7E07),
    ("wing", 0x5B9D_B0F0),
    ("wingL", 0x293D_899C),
    ("wingR", 0x293D_8982),
    ("joint1", 0xD0BE_09E0),
];

// ---------------------------------------------------------------------------
// Package access
// ---------------------------------------------------------------------------

/// A read-only handle over one package: its index, plus per-record extraction.
///
/// The whole ~1 GB image is never held in memory. Only the header and the index
/// block are read up front — they are contiguous — and payloads are read one
/// record at a time by extent, so peak memory is the largest single record.
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

/// A package's path, or `None` when the game is not installed.
///
/// "The corpus is absent" is a non-finding, not a test failure.
fn package_path(relative: &str) -> Option<PathBuf> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .ok()?;
    let path = root.join(relative);
    if !path.exists() {
        println!("SKIP: {relative} is not installed");
        return None;
    }
    Some(path)
}

// ---------------------------------------------------------------------------
// The bone-identity universe, rebuilt from the part layer
// ---------------------------------------------------------------------------

/// Every identity word the part layer's `Skeleton (0x70002)` sections carry.
///
/// This is report 11's 2 002-token universe, **rebuilt** rather than transcribed:
/// a 2 002-entry constant is a claim nobody could check, and the corpus it was
/// measured from is on disk.
struct Universe {
    tokens: BTreeSet<u32>,
    frequency: HashMap<u32, u32>,
    group_rows: usize,
    records_with_skeleton: usize,
    skeletons: usize,
    values: usize,
}

fn bone_universe(corpus: &mut Corpus) -> Universe {
    bone_universe_in(corpus, PART_GROUP)
}

/// [`bone_universe`] for an arbitrary group, so the whole-body layer can be
/// measured with the same reader that measured the part layer.
fn bone_universe_in(corpus: &mut Corpus, group: u32) -> Universe {
    let mut frequency: HashMap<u32, u32> = HashMap::new();
    let mut records = 0usize;
    let mut skeletons = 0usize;
    let mut values = 0usize;
    let entries: Vec<DbpfEntry> = corpus
        .entries_of_type(RW4_TYPE)
        .into_iter()
        .filter(|e| e.group_id == group)
        .collect();
    for entry in &entries {
        let raw = corpus.record(entry);
        let Ok(rw4) = spore_rw4::parse(&raw) else {
            continue;
        };
        let mut found = false;
        for section in &rw4.sections {
            if section.type_code != SKELETON_TC {
                continue;
            }
            let base = section.data_address as usize;
            let Some(bytes) = raw.get(base + 0x0C..base + 0x10) else {
                continue;
            };
            let count = u32::from_le_bytes(bytes.try_into().expect("four bytes"));
            // Report 11 §0 pins the payload size: 0x18 header bytes plus 12 per
            // bone. A section that does not match is some other structure, and
            // reading it as this one would invent identities out of geometry.
            if i64::from(section.size) != 0x18 + 12 * i64::from(count) {
                continue;
            }
            skeletons += 1;
            for bone in 0..count as usize {
                let at = base + 0x18 + 4 * bone;
                if let Some(word) = raw.get(at..at + 4) {
                    let token = u32::from_le_bytes(word.try_into().expect("four bytes"));
                    *frequency.entry(token).or_default() += 1;
                    values += 1;
                }
            }
            found = true;
        }
        if found {
            records += 1;
        }
    }
    Universe {
        tokens: frequency.keys().copied().collect(),
        frequency,
        group_rows: entries.len(),
        records_with_skeleton: records,
        skeletons,
        values,
    }
}

// ---------------------------------------------------------------------------
// The vocabulary split
// ---------------------------------------------------------------------------

/// The named split, keyed by identity word so a name and its grade stay together.
struct Named {
    strong: HashSet<u32>,
    weak: HashSet<u32>,
    names: BTreeMap<u32, &'static str>,
}

fn named_vocabulary() -> Named {
    let mut strong = HashSet::new();
    let mut weak = HashSet::new();
    let mut names = BTreeMap::new();
    for entry in vocabulary::entries() {
        let raw = entry.identity.to_raw();
        names.insert(raw, entry.name);
        if entry.confidence.is_witnessed() {
            strong.insert(raw);
        } else {
            assert_eq!(entry.confidence, Confidence::Unconfirmed);
            weak.insert(raw);
        }
    }
    Named {
        strong,
        weak,
        names,
    }
}

// ---------------------------------------------------------------------------
// The histogram
// ---------------------------------------------------------------------------

/// Counts of one measurement.
///
/// `*_distinct` is the count that decides the verdict, not `*_hits`: a token
/// repeated 400 times inside one anim record is one observation, and treating it
/// as 400 is how a hash coincidence becomes a "family".
#[derive(Debug, Default, Clone)]
struct Tally {
    /// 4-byte words examined, over all four alignment phases.
    words: u64,
    /// Words landing in the 2 002-token universe.
    universe_hits: u64,
    /// Distinct universe tokens.
    universe_distinct: BTreeSet<u32>,
    /// Words landing in a *witnessed* recovered name.
    strong_hits: u64,
    /// Distinct witnessed names.
    strong_distinct: BTreeSet<u32>,
    /// Words landing in an `Unconfirmed` recovered name.
    weak_hits: u64,
    /// Distinct `Unconfirmed` names.
    weak_distinct: BTreeSet<u32>,
    /// Words that are the instance id of a real row of this package's index.
    pointer_hits: u64,
    /// Distinct such instance ids.
    pointer_distinct: BTreeSet<u32>,
    /// Per-phase universe hits, so "which alignment" is a fact not an assumption.
    per_phase: [u64; 4],
}

impl Tally {
    fn absorb(&mut self, one: Tally) {
        self.words += one.words;
        self.universe_hits += one.universe_hits;
        self.universe_distinct.extend(one.universe_distinct);
        self.strong_hits += one.strong_hits;
        self.strong_distinct.extend(one.strong_distinct);
        self.weak_hits += one.weak_hits;
        self.weak_distinct.extend(one.weak_distinct);
        self.pointer_hits += one.pointer_hits;
        self.pointer_distinct.extend(one.pointer_distinct);
        for (slot, add) in self.per_phase.iter_mut().zip(one.per_phase) {
            *slot += add;
        }
    }

    fn every_distinct_token(&self) -> BTreeSet<u32> {
        self.universe_distinct
            .iter()
            .chain(self.strong_distinct.iter())
            .chain(self.weak_distinct.iter())
            .copied()
            .collect()
    }
}

/// Counts every 4-byte word of `region` against the vocabulary and the index.
///
/// **Alignment is swept, not assumed.** A table whose true boundary sits two
/// bytes earlier than this crate's walk believes is invisible to a phase-0-only
/// scan. Sweeping all four phases turns "I assumed an alignment" into a
/// measurement, and the per-phase counts are printed so a reader can see that
/// the swept baseline is not doing hidden work.
fn scan(region: &[u8], universe: &Universe, named: &Named, pointers: &HashSet<u32>) -> Tally {
    let mut tally = Tally::default();
    for (phase, slot) in tally.per_phase.iter_mut().enumerate() {
        let mut at = phase;
        while at + 4 <= region.len() {
            let word = u32::from_le_bytes(region[at..at + 4].try_into().expect("four"));
            if universe.tokens.contains(&word) {
                tally.universe_hits += 1;
                tally.universe_distinct.insert(word);
                *slot += 1;
            }
            if named.strong.contains(&word) {
                tally.strong_hits += 1;
                tally.strong_distinct.insert(word);
            }
            if named.weak.contains(&word) {
                tally.weak_hits += 1;
                tally.weak_distinct.insert(word);
            }
            if pointers.contains(&word) {
                tally.pointer_hits += 1;
                tally.pointer_distinct.insert(word);
            }
            tally.words += 1;
            at += 4;
        }
    }
    tally
}

/// Whether the word at `at` is the **instance** word of a resource key.
///
/// A gmdl trailer holds keys as three consecutive words: instance, type, group.
/// So a word is a pointer rather than an identity when the next twelve bytes
/// carry *both* the type and the group of a real index row that this word is the
/// instance of. Requiring all three is what stops "a group id that happens to be
/// a bone token" from being called a pointer, and vice versa.
fn is_key_shaped(region: &[u8], at: usize, rows: &[DbpfEntry]) -> bool {
    let word_at = |offset: usize| -> Option<u32> {
        region
            .get(offset..offset + 4)
            .map(|b| u32::from_le_bytes(b.try_into().expect("four bytes")))
    };
    let (Some(instance), Some(type_word), Some(group_word)) =
        (word_at(at), word_at(at + 4), word_at(at + 8))
    else {
        return false;
    };
    rows.iter().any(|row| {
        row.instance_id == instance && row.type_id == type_word && row.group_id == group_word
    })
}

/// Expected accidental hits for `words` words against `targets` of 2³².
fn chance(words: u64, targets: usize) -> f64 {
    words as f64 * targets as f64 / 4_294_967_296.0
}

// ---------------------------------------------------------------------------
// The measurement
// ---------------------------------------------------------------------------

#[test]
#[ignore = "reads the installed game; run with --release"]
fn the_anatomy_vocabulary_does_not_occur_in_the_gmdl_anim_trailer() {
    let (Some(content), Some(graphics)) = (
        package_path(CONTENT_PACKAGE),
        package_path(GRAPHICS_PACKAGE),
    ) else {
        return;
    };

    // ---- the vocabulary, rebuilt and checked against report 11's own numbers
    let mut graphics_corpus = Corpus::open(&graphics);
    let universe = bone_universe(&mut graphics_corpus);
    println!(
        "universe: {} group rows, {} records with a validated skeleton, {} skeletons, \
         {} distinct tokens, {} bone values",
        universe.group_rows,
        universe.records_with_skeleton,
        universe.skeletons,
        universe.tokens.len(),
        universe.values
    );
    assert_eq!(universe.group_rows, UNIVERSE_GROUP_ROWS);
    assert_eq!(
        (
            universe.records_with_skeleton,
            universe.skeletons,
            universe.tokens.len(),
            universe.values
        ),
        (
            UNIVERSE_RECORDS_WITH_SKELETON,
            UNIVERSE_SKELETONS,
            UNIVERSE_TOKENS,
            UNIVERSE_VALUES
        ),
        "the rebuilt token universe is not report 11's; every ratio below would be meaningless"
    );

    let named = named_vocabulary();
    println!(
        "named: {} = {} witnessed + {} unconfirmed",
        named.strong.len() + named.weak.len(),
        named.strong.len(),
        named.weak.len()
    );
    assert_eq!(
        named.strong.len() + named.weak.len(),
        244,
        "the table changed"
    );

    // The two identity words this file's whole method rests on, checked through
    // `spore_rigblock`'s own hash rather than against its table: if `fnv1` were
    // FNV-1a here, the histogram would report a confident zero about nothing.
    for (name, hash) in ANATOMY {
        assert_eq!(vocabulary::fnv1(name), hash, "{name} hash moved");
    }
    assert!(universe.tokens.contains(&0x7BD0_5F71));
    assert!(named.strong.contains(&0x7BD0_5F71));
    assert_eq!(
        BoneName::from_raw(0x7BD0_5F71).name_if_certain(),
        Some("root")
    );

    // ---- the whole-body layer's trailers
    let mut corpus = Corpus::open(&content);
    let pointers: HashSet<u32> = corpus
        .index
        .entries()
        .iter()
        .map(|e| e.instance_id)
        .filter(|id| *id != 0)
        .collect();
    let entries = corpus.entries_of_type(GMDL_TYPE);
    assert_eq!(entries.len(), GMDL_RECORDS, "the gmdl corpus moved");

    let mut located_tally = Tally::default();
    let mut located = 0usize;
    let mut regions: Vec<usize> = Vec::new();
    let mut anim_bearing = 0usize;
    let mut per_record_hits: BTreeMap<u32, PerRecord> = BTreeMap::new();
    let mut unlocated_groups: BTreeMap<u32, usize> = BTreeMap::new();
    let mut unlocated_stops: BTreeMap<String, usize> = BTreeMap::new();
    let mut key_shaped: BTreeMap<u32, bool> = BTreeMap::new();
    let mut wide_tally = Tally::default();
    let mut wide_anatomy: BTreeSet<u32> = BTreeSet::new();
    let mut wide_regions = 0usize;
    let mut wide_bytes = 0usize;

    let anatomy_set: HashSet<u32> = ANATOMY.iter().map(|(_, hash)| *hash).collect();
    let mut anatomy_found: BTreeSet<u32> = BTreeSet::new();
    for entry in &entries {
        let raw = corpus.record(entry);
        // `parse_recovering` is asked the same question for the same record: it
        // is the crate's own statement about how far the record was validated,
        // and a reader comparing the two should see them agree.
        let _ = parse_recovering(&raw);
        match trailer_region(&raw) {
            Some(region) => {
                located += 1;
                let len = region.len();
                regions.push(len);
                // 20 bytes is the floor this framing permits: a zero bone-range
                // count, a zero anim count, and the 12-byte trailing key. Records
                // at that size carry no anim data at all, and are counted apart,
                // because "8.9 M words scanned" means something very different if
                // most of them are the bytes of an absent table.
                if len > 20 {
                    anim_bearing += 1;
                }
                let one = scan(region, &universe, &named, &pointers);
                for token in one.every_distinct_token() {
                    key_shaped.insert(token, false);
                    for phase in 0..4 {
                        let mut at = phase;
                        while at + 4 <= region.len() {
                            let word =
                                u32::from_le_bytes(region[at..at + 4].try_into().expect("four"));
                            if word == token && is_key_shaped(region, at, corpus.index.entries()) {
                                key_shaped.insert(token, true);
                            }
                            at += 4;
                        }
                    }
                }
                for token in one.every_distinct_token() {
                    let slot = per_record_hits.entry(token).or_default();
                    slot.total += 1;
                    let mark = slot
                        .by_record
                        .entry((entry.group_id, entry.instance_id))
                        .or_default();
                    *mark += 1;
                }
                located_tally.absorb(one);
                for phase in 0..4 {
                    let mut at = phase;
                    while at + 4 <= region.len() {
                        let word = u32::from_le_bytes(region[at..at + 4].try_into().expect("four"));
                        if anatomy_set.contains(&word) {
                            anatomy_found.insert(word);
                        }
                        at += 4;
                    }
                }
            }
            None => {
                *unlocated_groups.entry(entry.group_id).or_default() += 1;
                let reason = parse_recovering(&raw)
                    .ok()
                    .and_then(|p| p.stop)
                    .map(|s| format!("{}:{}", s.stage.as_str(), s.error))
                    .unwrap_or_else(|| "unknown".to_string());
                *unlocated_stops.entry(reason).or_default() += 1;
                // Region B: no located trailer exists for these records, so the
                // wider span from the last validated byte to the record end is
                // measured instead and reported as a *different, weaker* claim.
                // It over-includes whatever material info this crate could not
                // size, which can only add false positives — never hide a hit.
                if let Ok(partial) = parse_recovering(&raw) {
                    let from = partial.model.strict_consumed.min(raw.len());
                    let region = &raw[from..];
                    wide_regions += 1;
                    wide_bytes += region.len();
                    wide_tally.absorb(scan(region, &universe, &named, &pointers));
                    for phase in 0..4 {
                        let mut at = phase;
                        while at + 4 <= region.len() {
                            let word =
                                u32::from_le_bytes(region[at..at + 4].try_into().expect("four"));
                            if anatomy_set.contains(&word) {
                                wide_anatomy.insert(word);
                            }
                            at += 4;
                        }
                    }
                }
            }
        }
    }

    regions.sort_unstable();
    let total: usize = regions.iter().sum();
    let median = regions.get(regions.len() / 2).copied().unwrap_or(0);
    let biggest = regions.last().copied().unwrap_or(0);
    let trivial = regions.iter().filter(|n| **n <= 20).count();

    println!("\n=== region A: the located trailer ===");
    println!(
        "located for {located} of {} records; {} of those carry anim data (>20 B), \
         {trivial} are the bare 20-byte floor",
        entries.len(),
        anim_bearing
    );
    println!("trailer bytes: total {total}, median {median}, biggest {biggest}");
    println!(
        "words scanned (4 alignment phases): {}",
        located_tally.words
    );
    println!(
        "universe hits {} over {} distinct tokens; chance {:.3} words; \
         per-phase {:?}",
        located_tally.universe_hits,
        located_tally.universe_distinct.len(),
        chance(located_tally.words, UNIVERSE_TOKENS),
        located_tally.per_phase
    );
    println!(
        "named: {} witnessed hits over {} distinct (chance {:.3}); {} unconfirmed hits \
         over {} distinct (chance {:.3})",
        located_tally.strong_hits,
        located_tally.strong_distinct.len(),
        chance(located_tally.words, named.strong.len()),
        located_tally.weak_hits,
        located_tally.weak_distinct.len(),
        chance(located_tally.words, named.weak.len())
    );
    println!(
        "pointer control: {} words are a live index instance id, {} distinct; \
         chance {:.3}",
        located_tally.pointer_hits,
        located_tally.pointer_distinct.len(),
        chance(located_tally.words, pointers.len())
    );

    println!("\n=== every distinct universe token, and what it turns out to be ===");
    let mut unexplained = 0usize;
    for token in located_tally.every_distinct_token() {
        let label = named.names.get(&token).copied().unwrap_or("<unnamed>");
        let is_pointer = pointers.contains(&token);
        // "Is an instance id somewhere in the package" is weak evidence on its
        // own, because one instance id is shared by a record's sibling rows.
        // The strong form is the key triple, and it is asserted below.
        let shaped = key_shaped.get(&token).copied().unwrap_or(false);
        if !is_pointer {
            unexplained += 1;
        }
        let rows: Vec<String> = corpus
            .index
            .entries()
            .iter()
            .filter(|e| e.instance_id == token)
            .map(|e| format!("t=0x{:08x} g=0x{:08x}", e.type_id, e.group_id))
            .collect();
        println!(
            "  0x{token:08x} {label:<14} witnessed={} unconfirmed={} lives_in_index={} \
             key_shaped={shaped} rows=[{}] part_layer_freq={}",
            named.strong.contains(&token),
            named.weak.contains(&token),
            is_pointer,
            rows.join(" "),
            universe.frequency.get(&token).copied().unwrap_or(0),
        );
        assert!(
            shaped,
            "bone-identity token 0x{token:08x} ({label}) occurs in the trailer but is not \
             the instance word of any resource key; it would be an unexplained identity"
        );
    }
    println!("tokens that are neither a pointer nor explained: {unexplained}");

    println!("\n=== region B: the 17 anatomical names, by hand ===");
    let anatomy_names = anatomy_found.len();
    for (name, hash) in ANATOMY {
        let state = if anatomy_found.contains(&hash) {
            "PRESENT"
        } else {
            "absent"
        };
        println!("  {name:<10} 0x{hash:08x} {state}");
    }
    println!("anatomical names present in the gmdl anim trailer: {anatomy_names}");
    println!("\nper-record distribution of every distinct token:");
    for (token, spread) in &per_record_hits {
        let label = named.names.get(token).copied().unwrap_or("<unnamed>");
        println!(
            "  0x{token:08x} {label:<14} in {} record(s)",
            spread.by_record.len()
        );
    }

    println!("\n=== the records with no located trailer ===");
    let unlocated: usize = unlocated_groups.values().sum();
    println!("{unlocated} records, by group:");
    for (group, n) in &unlocated_groups {
        println!("  group 0x{group:08x}: {n}");
    }
    println!("and why the walk could not size their material info:");
    for (reason, n) in &unlocated_stops {
        println!("  {n:5} x {reason}");
    }

    println!("\n=== region B: the {unlocated} unlocated records, measured wider ===");
    println!(
        "measured {wide_regions} records, {wide_bytes} bytes from the last validated byte \
         to the record end (this span over-includes unsized material info)"
    );
    println!("words scanned (4 alignment phases): {}", wide_tally.words);
    println!(
        "universe hits {} over {} distinct; chance {:.3}; per-phase {:?}",
        wide_tally.universe_hits,
        wide_tally.universe_distinct.len(),
        chance(wide_tally.words, UNIVERSE_TOKENS),
        wide_tally.per_phase
    );
    println!(
        "witnessed-name hits: {} over {} distinct (chance {:.3})",
        wide_tally.strong_hits,
        wide_tally.strong_distinct.len(),
        chance(wide_tally.words, named.strong.len())
    );
    println!(
        "pointer control: {} words over {} distinct instance ids",
        wide_tally.pointer_hits,
        wide_tally.pointer_distinct.len()
    );
    println!(
        "anatomical names present in region B: {}",
        wide_anatomy.len()
    );
    for token in &wide_anatomy {
        println!("  0x{token:08x} present");
    }

    println!("\n=== what the trailer actually points at (region A pointer words) ===");
    let mut targets: BTreeMap<(u32, u32), usize> = BTreeMap::new();
    for token in &located_tally.pointer_distinct {
        for row in corpus
            .index
            .entries()
            .iter()
            .filter(|e| e.instance_id == *token)
        {
            *targets.entry((row.type_id, row.group_id)).or_default() += 1;
        }
    }
    let mut ranked: Vec<((u32, u32), usize)> = targets.into_iter().collect();
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    for ((type_id, group), n) in ranked.iter().take(12) {
        println!("  {n:5} instance ids resolve to t=0x{type_id:08x} g=0x{group:08x}");
    }
    let anim_targets = ranked
        .iter()
        .filter(|((type_id, _), _)| *type_id == RW4_TYPE)
        .count();
    println!(
        "distinct (type, group) targets behind the trailer's instance ids: {}",
        ranked.len()
    );
    println!("of those, pointing at an rw4 container: {anim_targets}");
    let rw4_groups: Vec<(u32, usize)> = ranked
        .iter()
        .filter(|((t, _), _)| *t == RW4_TYPE)
        .map(|((_, group), n)| (*group, *n))
        .collect();
    for (group, n) in rw4_groups {
        println!("  rw4 group 0x{group:08x}: {n} distinct instance ids");
        // Does the container the whole-body anim data names carry a skeleton at
        // all? If it does not, the anatomy is not reachable from the anim data
        // by pointer either, and the next probe has to start somewhere else.
        let mut skeleton_sections = 0usize;
        let mut probed = 0usize;
        let mut codes: BTreeMap<u32, usize> = BTreeMap::new();
        let probe_rows: Vec<DbpfEntry> = corpus
            .index
            .entries()
            .iter()
            .filter(|e| e.type_id == RW4_TYPE && e.group_id == group)
            .take(4)
            .copied()
            .collect();
        for row in probe_rows {
            let raw = corpus.record(&row);
            probed += 1;
            let Ok(rw4) = spore_rw4::parse(&raw) else {
                continue;
            };
            for section in &rw4.sections {
                *codes.entry(section.type_code).or_default() += 1;
                if section.type_code == SKELETON_TC {
                    skeleton_sections += 1;
                }
            }
        }
        println!(
            "    probed {probed} record(s): 0x70002 Skeleton sections = {skeleton_sections}; \
             section codes {codes:?}"
        );
    }

    // ---- the verdict, as assertions
    //
    // 1. No witnessed name occurs at all, anywhere. This is the whole claim, and
    //    it needs no baseline to be strong: 0 hits where 0.35 are expected.
    // 2. None of the 17 anatomical names occurs.
    // 3. Every universe token that does occur is explained. A token in the
    //    2 002-universe that is *not* also a live index row would be a candidate
    //    anatomy token, and the count of those is the number that decides it.
    assert_eq!(unlocated, UNLOCATED_RECORDS);
    assert_eq!(
        located_tally.strong_hits, 0,
        "a witnessed bone name occurred in the gmdl anim trailer: {:?}",
        located_tally.strong_distinct
    );
    assert_eq!(
        anatomy_names, 0,
        "an anatomical bone name occurred in the gmdl anim trailer"
    );
    assert_eq!(
        key_shaped.values().filter(|ok| **ok).count(),
        located_tally.every_distinct_token().len(),
        "every universe token that occurs must be a resource key's instance word"
    );
    assert_eq!(
        unexplained,
        0,
        "a bone-identity token occurred that no resource key explains: {:?}",
        located_tally.every_distinct_token()
    );
    assert_eq!(
        wide_tally.strong_hits, 0,
        "a witnessed bone name occurred in the wider unlocated-record region: {:?}",
        wide_tally.strong_distinct
    );
    assert_eq!(
        wide_anatomy.len(),
        0,
        "an anatomical bone name occurred in the wider unlocated-record region"
    );
    assert_eq!(unlocated_groups.get(&0x4062_7100).copied(), Some(97));
    // Distinct tokens, not words: the honest denominator. 4.14 tokens are
    // expected by chance and 3 were found.
    assert!(
        located_tally.universe_distinct.len() <= 8,
        "{} distinct bone-identity tokens in the gmdl anim trailer is above the \
         chance expectation of {:.2} and needs explaining",
        located_tally.universe_distinct.len(),
        chance(located_tally.words, UNIVERSE_TOKENS)
    );
}

/// Where one identity word was seen: how many records carried it, and which.
///
/// Distinguishes "one table, repeated" from "a hundred scattered coincidences",
/// which are different evidence and must not be summed.
#[derive(Debug, Default, Clone)]
struct PerRecord {
    total: usize,
    by_record: BTreeMap<(u32, u32), usize>,
}
// ---------------------------------------------------------------------------
// Where the pointer leads
// ---------------------------------------------------------------------------

/// The gmdl anim data's own resource key names a *different* skeleton group.
///
/// ## Why this is in the same file
///
/// The primary measurement refutes the anim-data hypothesis, and a refutation
/// that stops at "not there" is half a result. The same histogram already
/// produced the pointer map, and that map has exactly four `rw4` groups in it.
/// Three of them carry nothing but `0x80003`. The fourth, `0x41606100`, carries
/// `Skeleton (0x70002)` sections — and it is **not** the part layer's
/// `0x40606000`. So the whole-body anim data does reach a skeleton; it just does
/// not inline the identities. That makes this the cheapest next probe by a wide
/// margin, and measuring it here is worth more than naming it in prose.
///
/// ## What is asserted
///
/// Only two things, both of which survive a negative result:
///
/// 1. group `0x41606100` really does carry `Skeleton (0x70002)` sections;
/// 2. the identities it carries are reported with their overlap against the part
///    layer's 2 002-token universe — a real number either way, not a vibe.
#[test]
#[ignore = "reads the installed game; run with --release"]
fn the_whole_body_anim_data_points_at_a_skeleton_group_the_part_layer_does_not_use() {
    let (Some(content), Some(graphics)) = (
        package_path(CONTENT_PACKAGE),
        package_path(GRAPHICS_PACKAGE),
    ) else {
        return;
    };

    // The part layer's vocabulary, for the overlap question.
    let mut graphics_corpus = Corpus::open(&graphics);
    let part_layer = bone_universe_in(&mut graphics_corpus, PART_GROUP);
    assert_eq!(part_layer.tokens.len(), UNIVERSE_TOKENS, "part layer moved");

    // The whole-body skeleton group.
    let mut corpus = Corpus::open(&content);
    let body_layer = bone_universe_in(&mut corpus, WHOLE_BODY_GROUP);
    println!(
        "group 0x{WHOLE_BODY_GROUP:08x}: {} rw4 rows, {} records with a validated \\
         skeleton, {} skeletons, {} distinct tokens, {} bone values",
        body_layer.group_rows,
        body_layer.records_with_skeleton,
        body_layer.skeletons,
        body_layer.tokens.len(),
        body_layer.values
    );

    assert!(
        body_layer.skeletons > 0,
        "group 0x{WHOLE_BODY_GROUP:08x} carries no Skeleton (0x70002); the pointer lead \
         would be a dead end and this measurement has to say so"
    );
    let shared: Vec<u32> = body_layer
        .tokens
        .intersection(&part_layer.tokens)
        .copied()
        .collect();
    println!(
        "token overlap with the part layer: {} of {} whole-body tokens are also \\
         part-layer tokens ({:.1}%)",
        shared.len(),
        body_layer.tokens.len(),
        100.0 * shared.len() as f64 / body_layer.tokens.len() as f64
    );

    // And the whole-body names, if any were recovered.
    let named = named_vocabulary();
    let named_hits: Vec<(&str, u32)> = body_layer
        .tokens
        .iter()
        .filter_map(|t| named.names.get(t).map(|name| (*name, *t)))
        .collect();
    println!(
        "whole-body tokens with a recovered name: {}",
        named_hits.len()
    );
    for (name, token) in named_hits.iter() {
        println!(
            "  0x{token:08x} {name} (freq {}, witnessed={})",
            body_layer.frequency.get(token).copied().unwrap_or(0),
            named.strong.contains(token)
        );
    }

    // The 17 anatomical names, asked of the whole-body skeleton group this time.
    let mut present: Vec<&str> = Vec::new();
    for (name, hash) in ANATOMY {
        if body_layer.tokens.contains(&hash) {
            present.push(name);
            println!(
                "  anatomy: {name:<10} 0x{hash:08x} PRESENT (freq {})",
                body_layer.frequency.get(&hash).copied().unwrap_or(0)
            );
        }
    }
    println!(
        "anatomical names present in group 0x{WHOLE_BODY_GROUP:08x}: {}",
        present.len()
    );

    // The measurement the primary question asked, run against this group: do the
    // whole-body skeleton tokens appear inline in the gmdl trailer?
    let pointers: HashSet<u32> = corpus
        .index
        .entries()
        .iter()
        .map(|e| e.instance_id)
        .filter(|id| *id != 0)
        .collect();
    let entries = corpus.entries_of_type(GMDL_TYPE);
    let mut tally = Tally::default();
    for entry in &entries {
        let raw = corpus.record(entry);
        let Some(region) = trailer_region(&raw) else {
            continue;
        };
        tally.absorb(scan(region, &body_layer, &named, &pointers));
    }
    println!(
        "\ngmdl trailers against the 0x{WHOLE_BODY_GROUP:08x} vocabulary: {} words scanned, \
         {} hits over {} distinct (chance {:.3}), {} witnessed hits",
        tally.words,
        tally.universe_hits,
        tally.universe_distinct.len(),
        chance(tally.words, body_layer.tokens.len()),
        tally.strong_hits
    );

    // ---- the two assertions this measurement is worth
    //
    // The first is the positive one: the anatomy vocabulary DOES occur in the
    // whole-body skeleton layer, by pointer, and that is the answer the gmdl
    // anim-data hypothesis was groping towards. It is asserted rather than
    // printed because it is the finding, and a finding that only appears in
    // --nocapture output is a finding that will be believed once and lost.
    //
    // The second is the negative that makes the first mean something: those same
    // identities do **not** appear inline in the gmdl trailer that points at
    // them. Anatomy is reached through the pointer, not stored in the animation
    // record, and the two facts have to be reported together or the first one
    // reads as "found in the anim data".
    assert!(
        present.len() >= 8,
        "only {} of the 17 anatomical names occur in group 0x{WHOLE_BODY_GROUP:08x}: {present:?}; \
         the pointer lead would no longer support the anatomy claim",
        present.len()
    );
    assert!(
        tally.strong_hits == 0,
        "a witnessed bone name occurred inline in the gmdl trailer: {:?}",
        tally.strong_distinct
    );
}

/// The whole-body skeleton group the gmdl anim data's resource keys name.
const WHOLE_BODY_GROUP: u32 = 0x4160_6100;
