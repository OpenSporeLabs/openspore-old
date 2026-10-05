//! How often the real corpus reaches an input class on which
//! `spore_texture::framing::level_offset` would have to answer.
//!
//! **Every test here is `#[ignore]`d and every one skips cleanly when the game is
//! not installed.** They read `SPORE/Data/*.package` and `SPORE/DataEP1/*.package`
//! from the operator's disk; they never copy, embed or commit a Spore byte.
//! Without `SPORE/` they print a `SKIP:` line and return.
//!
//! ```text
//! cargo test -p spore-differential --release --test framing_refusals -- --ignored --nocapture
//! ```
//!
//! # Why this file lives in `spore-differential` and not in `spore-texture`
//!
//! A census needs the real package bytes, which means `spore-dbpf`, and
//! `spore-texture` depends only on `spore-core` and `thiserror`. Adding
//! `spore-dbpf` as a dev-dependency would edit `Cargo.lock`, which this
//! workstream is forbidden to touch, so the measurement lives in the crate that
//! already has both dependencies and already owns the corpus comparison.
//!
//! # What it measures
//!
//! `level_offset(frame, width, height, mip)` places a level from four numbers and
//! nothing else. Every input class on which those four numbers do **not**
//! determine an offset — a level the envelope does not declare, a chain longer
//! than the crate can place, a degenerate dimension — is a class where the
//! function had to answer anyway. This file counts how many real records reach
//! each class, so the answer is a measurement and not an intuition.
//!
//! The offset arithmetic itself is **not** re-derived here: it is checked against
//! the same independently written transcription `luminance_corpus.rs` uses
//! (`Frame`, `dim`, `ceil4`, `level_bytes`, `windows`), reached by walking the
//! crate's public `level_offset` and the independent `windows` side by side and
//! reporting every level where they differ. A class with a disagreement count of
//! zero is a class the crate gets right over real data.

use std::collections::BTreeMap;
use std::path::PathBuf;

use spore_assets::Package;
use spore_core::record::type_id;
use spore_differential::Corpus;
use spore_texture::framing::{frame_for, FrameFormat, MAX_PLACEABLE_LEVELS};

/// The packages this file measures, matching `luminance_corpus.rs`.
const PACKAGES: [&str; 2] = ["Spore_Content.package", "Spore_EP1_Content_01.package"];

// ------------------------------------------- the framing, transcribed again here
//
// Three transcriptions of this table exist (`spore_texture::framing`, this file,
// and the driver's `FRAMES`). Where they disagree one of them mistyped a
// constant, which is worth finding; where they agree it is the arithmetic of two
// readings of `docs/RASTER-FRAMING.md` §2, not the truth. Only the corpus is that.

/// Payload bytes of one level, written out from the measured table.
fn level_bytes(frame: FrameFormat, width: u32, height: u32, mip: u32) -> usize {
    let dim = |base: u32| -> usize {
        let d = if mip >= 32 { 1 } else { (base >> mip).max(1) };
        usize::try_from(d).unwrap_or(usize::MAX)
    };
    if frame.block_bytes == 0 {
        return dim(width).saturating_mul(dim(height)).saturating_mul(4);
    }
    let blocks = |n: usize| n / 4 + usize::from(n % 4 != 0);
    blocks(dim(width))
        .saturating_mul(blocks(dim(height)))
        .saturating_mul(frame.block_bytes)
}

/// `(offset, length)` of every level's **payload**, prefixed levels included.
fn windows(frame: FrameFormat, width: u32, height: u32, mip_count: u32) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut at = frame.header_size;
    for mip in 0..mip_count {
        if mip >= frame.first_prefixed_level {
            at += 4;
        }
        let len = level_bytes(frame, width, height, mip);
        out.push((at, len));
        at += len;
    }
    out
}

// ------------------------------------------------------------------- the census

/// Every raster record's envelope in both packages.
fn census() -> Vec<(String, usize, spore_texture::RasterEnvelope)> {
    let corpus = Corpus::discover();
    let located: Vec<(String, PathBuf)> = PACKAGES
        .iter()
        .filter_map(|name| {
            corpus
                .packages()
                .iter()
                .find(|p| p.name == *name)
                .map(|p| (p.name.clone(), p.path.clone()))
        })
        .collect();

    let mut out = Vec::new();
    for (name, path) in located {
        let package = Package::open(name.clone(), path.clone()).expect("package must open");
        let image = package.bytes();
        for (row, entry) in package.index().entries().iter().enumerate() {
            if entry.type_id != type_id::RASTER {
                continue;
            }
            let bytes = spore_dbpf::extract_record(image, entry)
                .unwrap_or_else(|error| panic!("{name} row {row}: extract failed: {error}"));
            let envelope = spore_texture::parse_envelope(&bytes)
                .unwrap_or_else(|error| panic!("{name} row {row}: envelope refused: {error}"));
            out.push((name.clone(), row, envelope));
        }
    }
    out
}

/// One input class, and the count of real records that reach it.
#[derive(Debug, Default, Clone)]
struct Tally {
    /// Records reaching the class.
    records: usize,
    /// Distinct `(fourcc, width, height, mip_count)` shapes reaching it.
    shapes: BTreeMap<(u32, u32, u32, u32), usize>,
}

impl Tally {
    fn add(&mut self, env: &spore_texture::RasterEnvelope) {
        self.records += 1;
        *self
            .shapes
            .entry((env.fourcc, env.width, env.height, env.mip_count))
            .or_default() += 1;
    }

    fn report(&self, name: &str) {
        println!(
            "class {name}: {} record(s), {} distinct shape(s){}",
            self.records,
            self.shapes.len(),
            if self.records == 0 {
                String::new()
            } else {
                let mut shapes: Vec<String> = self
                    .shapes
                    .iter()
                    .map(|((f, w, h, m), n)| format!("0x{f:08x} {w}x{h} mips={m} x{n}"))
                    .collect();
                shapes.sort();
                format!(": {}", shapes.join(", "))
            }
        );
    }
}

/// `(fourcc, [(value, count)])` from a `(fourcc, value) -> count` table, in
/// fourcc order.
fn by_fourcc(table: &BTreeMap<(u32, u32), usize>) -> Vec<(u32, Vec<(u32, usize)>)> {
    let mut out: BTreeMap<u32, Vec<(u32, usize)>> = BTreeMap::new();
    for ((fourcc, value), n) in table {
        out.entry(*fourcc).or_default().push((*value, *n));
    }
    out.into_iter().collect()
}

/// `width`'s bit width: the number of halvings before `base >> mip` reaches zero.
///
/// For a power-of-two `d` this is exactly the mip count a chain ends at, because
/// level `bits - 1` is `d >> (bits - 1) == 1`. `512 -> 10`, `256 -> 9`,
/// `64 -> 7`, `32 -> 6`: every figure `luminance_corpus.rs` measures.
fn bits(d: u32) -> u32 {
    32 - d.leading_zeros()
}

/// The mip count the dimensions imply, by `level_dimension`'s own rule.
fn natural_mip_count(width: u32, height: u32) -> u32 {
    bits(width.max(height))
}

/// Every input class `level_offset` cannot answer from its four arguments, counted
/// over the real corpus, plus the level-by-level disagreement between the crate's
/// walk and an independent transcription of the same framing.
#[test]
#[ignore = "needs the git-ignored SPORE/ tree (~1.5 GB)"]
fn the_corpus_counts_every_input_class_level_offset_cannot_answer() {
    let census = census();
    if census.is_empty() {
        eprintln!("SKIP: no Spore content package is installed; nothing to measure");
        return;
    }
    println!("raster records censused: {}", census.len());

    let mut unknown_fourcc = Tally::default();
    let mut zero_dimension = Tally::default();
    let mut zero_mip_count = Tally::default();
    let mut non_power_of_two = Tally::default();
    let mut not_block_aligned = Tally::default();
    let mut non_square = Tally::default();
    let mut mips_beyond_measured = Tally::default();
    let mut mips_beyond_placeable = Tally::default();
    let mut chain_shorter_than_geometry = Tally::default();
    let mut chain_longer_than_geometry = Tally::default();
    let mut chain_matches_geometry = Tally::default();
    let mut field_10_not_eight = Tally::default();
    let mut field_10_zero = Tally::default();
    let mut field_18_disagrees = Tally::default();
    let mut field_1c_zero = Tally::default();
    let mut all_shapes: BTreeMap<(u32, u32, u32, u32), usize> = BTreeMap::new();
    let mut field_1c_values: BTreeMap<(u32, u32), usize> = BTreeMap::new();
    let mut field_10_values: BTreeMap<u32, usize> = BTreeMap::new();

    // `level_offset` levels at or beyond `MIP_FLOOR` (32) and levels the envelope
    // never declared: does the crate place them the way the independent walk does?
    let mut placed_disagreements = 0usize;
    let mut declared_disagreements = 0usize;
    let mut declared_in_record = 0usize;
    let mut declared_probe_examples: Vec<String> = Vec::new();

    for (name, row, env) in &census {
        let tally = |t: &mut Tally| t.add(env);
        let Some(frame) = frame_for(env.fourcc) else {
            tally(&mut unknown_fourcc);
            continue;
        };

        *all_shapes
            .entry((env.fourcc, env.width, env.height, env.mip_count))
            .or_default() += 1;

        if env.width == 0 || env.height == 0 {
            tally(&mut zero_dimension);
        }
        if env.mip_count == 0 {
            tally(&mut zero_mip_count);
        }
        if !env.width.is_power_of_two() || !env.height.is_power_of_two() {
            tally(&mut non_power_of_two);
        }
        if env.width % 4 != 0 || env.height % 4 != 0 {
            tally(&mut not_block_aligned);
        }
        if env.width != env.height {
            tally(&mut non_square);
        }
        if env.mip_count > 10 {
            tally(&mut mips_beyond_measured);
        }
        if env.mip_count > MAX_PLACEABLE_LEVELS {
            tally(&mut mips_beyond_placeable);
        }
        match env.mip_count.cmp(&natural_mip_count(env.width, env.height)) {
            std::cmp::Ordering::Less => tally(&mut chain_shorter_than_geometry),
            std::cmp::Ordering::Greater => tally(&mut chain_longer_than_geometry),
            std::cmp::Ordering::Equal => tally(&mut chain_matches_geometry),
        }
        if env.field_10 != 8 {
            tally(&mut field_10_not_eight);
        }
        if env.field_10 == 0 {
            tally(&mut field_10_zero);
        }
        if env.field_18 as usize != level_bytes(frame, env.width, env.height, 0) {
            tally(&mut field_18_disagrees);
        }
        if env.field_1c == 0 {
            tally(&mut field_1c_zero);
        }
        *field_1c_values
            .entry((env.fourcc, env.field_1c))
            .or_default() += 1;
        *field_10_values.entry(env.field_10).or_default() += 1;

        // --- the walk itself, level by level, against the independent windows.
        let independent = windows(frame, env.width, env.height, env.mip_count);
        for (mip, (payload_at, _)) in independent.iter().enumerate() {
            let mut at = spore_texture::framing::level_offset(
                frame,
                env.width,
                env.height,
                mip as u32,
                env.mip_count,
            )
            .expect("a level the envelope declares is placeable");
            if mip as u32 >= frame.first_prefixed_level {
                at += spore_texture::framing::LEVEL_PREFIX_SIZE;
            }
            if at != *payload_at {
                placed_disagreements += 1;
            }
        }
        // --- and the levels the envelope cannot answer for at all. Four indices:
        //     one past the declared chain, two inside the placeable range and two
        //     past it. `at <= own` is the right test and not a strict `<`: asking
        //     for level `mip_count` of a `mip_count`-level chain lands exactly on
        //     the record end, which is one-past-the-last-byte and still a number
        //     a caller would slice with.
        let own = independent.last().map_or(frame.header_size, |(a, l)| a + l);
        for probe in [
            env.mip_count,
            env.mip_count + 1,
            spore_texture::MIP_FLOOR + 1,
            u32::MAX,
        ] {
            // Pre-fix, every one of these four probes returned a `usize`, and the
            // first returned the record's own length -- a plausible in-record
            // offset for a level the envelope does not declare. All 4386 records
            // did, because each of them is asked the same question. Post-fix, all
            // four are refused by name and no offset exists to compare.
            declared_disagreements += 1;
            if let Ok(at) = spore_texture::framing::level_offset(
                frame,
                env.width,
                env.height,
                probe,
                env.mip_count,
            ) {
                declared_in_record += 1;
                if declared_probe_examples.len() < 10 {
                    declared_probe_examples.push(format!(
                        "{name} row {row}: 0x{:08x} {}x{} mips={} declares levels 0..{}, asked for \
                         level {probe}, and got offset {at} -- record end is {own}",
                        env.fourcc, env.width, env.height, env.mip_count, env.mip_count - 1
                    ));
                }
            }
        }
    }

    println!();
    unknown_fourcc.report("fourcc with no frame");
    zero_dimension.report("width or height is 0");
    zero_mip_count.report("mip_count is 0");
    non_power_of_two.report("width or height is not a power of two");
    not_block_aligned.report("width or height is not a multiple of 4");
    non_square.report("width != height");
    mips_beyond_measured.report("mip_count > 10 (the largest chain the claim records)");
    mips_beyond_placeable.report("mip_count > MAX_PLACEABLE_LEVELS (what the walk refuses)");
    chain_shorter_than_geometry.report("mip_count < the dimensions imply (last level not 1x1)");
    chain_longer_than_geometry.report("mip_count > the dimensions imply (levels past 1x1)");
    chain_matches_geometry.report("mip_count == the dimensions imply (last level is 1x1)");
    field_10_not_eight.report("field_10 != 8");
    field_10_zero.report("field_10 == 0");
    field_18_disagrees.report("field_18 != the base level's own byte count");
    field_1c_zero.report("field_1c == 0");

    println!();
    // `field_10` and `field_1c` are printed as distributions rather than as counts,
    // because the question each raises is "is this word a constant?", and a single
    // "0 disagreements" line would not answer it. Neither word is read by
    // `level_offset` -- spore-texture pins that negatively -- so this is evidence
    // about the envelope, reported here because it was measured here.
    println!("field_10 distribution: {field_10_values:?}");
    for (fourcc, values) in by_fourcc(&field_1c_values) {
        let total: usize = values.iter().map(|(_, n)| *n).sum();
        let mut by_count: Vec<(u32, usize)> = values.clone();
        by_count.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        let top = by_count
            .iter()
            .take(3)
            .map(|(value, n)| format!("0x{value:08x} x{n}"))
            .collect::<Vec<String>>()
            .join(", ");
        println!(
            "field_1c in 0x{fourcc:08x} records: {} distinct value(s) over {total} record(s); \
             most common: {top}",
            values.len()
        );
    }

    println!();
    println!(
        "distinct (fourcc, width, height, mip_count) shapes: {}",
        all_shapes.len()
    );
    for ((f, w, h, m), n) in &all_shapes {
        println!("  0x{f:08x} {w}x{h} mips={m}: {n} record(s)");
    }

    println!();
    println!(
        "levels the envelope DECLARES where level_offset disagrees with the independent \
         walk: {placed_disagreements}"
    );
    println!(
        "levels the envelope does NOT declare (mip_count, MIP_FLOOR+1) probed per record: \
         {declared_disagreements} probe(s); {} returned an offset at \
         or before the record end",
        declared_probe_examples.len()
    );
    for example in &declared_probe_examples {
        println!("  {example}");
    }

    // The measurement, as assertions. Every count above is printed; these are the
    // ones this repository's documents state, so a corpus change shows up.
    assert_eq!(
        placed_disagreements, 0,
        "the crate places every level the envelope declares exactly where an independent \
         transcription places it"
    );
    assert_eq!(
        zero_dimension.records, 0,
        "no degenerate dimension in the corpus"
    );
    assert_eq!(zero_mip_count.records, 0, "no zero mip count in the corpus");
    assert_eq!(
        non_power_of_two.records, 0,
        "the halving rule is measured only over power-of-two dimensions, and every corpus \
         dimension is one"
    );
    assert_eq!(
        field_18_disagrees.records, 0,
        "field_18 is the base level's byte count"
    );
    assert_eq!(
        mips_beyond_placeable.records, 0,
        "no corpus record declares a chain longer than the walk places"
    );
    assert_eq!(
        chain_longer_than_geometry.records, 0,
        "no corpus record declares levels past the point where its geometry reaches 1x1"
    );
    assert_eq!(
        declared_in_record, 0,
        "a level the envelope does not declare must not come back with an offset at or inside \
         the record: that is the fabrication this census exists to find"
    );
}

/// What `level_offset` answers for indices the envelope does not contain, on a real
/// shape: one refusal per question, and never a number.
///
/// Pre-fix this test read as a demonstration of the defect and it FAILED, which is
/// how the fabrication was found: all six probes past a ten-level record came back
/// as `usize`, and the one for `level 10` came back as **349 616** — the record's
/// own length, i.e. a real, in-record offset for a level that does not exist.
///
/// The census above counts the same thing over all 4386 records. This test takes
/// one real shape and shows each index individually, because the count alone would
/// not say *which* answer was the plausible one.
#[test]
#[ignore = "needs the git-ignored SPORE/ tree (~1.5 GB)"]
fn an_undeclared_or_unplaceable_level_is_refused_on_a_real_shape() {
    let census = census();
    if census.is_empty() {
        eprintln!("SKIP: no Spore content package is installed; nothing to measure");
        return;
    }
    let env = census
        .iter()
        .map(|(_, _, env)| *env)
        .find(|env| env.mip_count == 10 && env.width == 512)
        .expect("the corpus holds a 512x512 10-level record");
    let frame = frame_for(env.fourcc).expect("a frame for every corpus fourcc");
    println!(
        "probe shape: 0x{:08x} {}x{} mips={}",
        env.fourcc, env.width, env.height, env.mip_count
    );

    let record_end =
        spore_texture::framing::record_size(frame, env.width, env.height, env.mip_count);
    println!("record size the crate reports: {record_end}");

    // The level the envelope does not declare, and every index past the ceiling.
    // `mip_count` is deliberately absent: no record in the corpus has a chain this
    // build cannot place, so this is a probe of the function, not of the data.
    for (probe, mip_count, what) in [
        (env.mip_count, env.mip_count, "one past the declared chain"),
        (
            env.mip_count + 1,
            env.mip_count + 1,
            "two past the declared chain",
        ),
        (
            spore_texture::MIP_FLOOR + 1,
            u32::MAX,
            "the first index past the ceiling",
        ),
        (64, u32::MAX, "far past the ceiling"),
        (u32::MAX, u32::MAX, "the largest index there is"),
    ] {
        let answer =
            spore_texture::framing::level_offset(frame, env.width, env.height, probe, mip_count);
        let verdict = match &answer {
            Ok(offset) => format!(
                "offset {offset:>8}  ({} the record end)",
                if *offset <= record_end {
                    "INSIDE"
                } else {
                    "past"
                }
            ),
            Err(error) => format!("REFUSED: {error}"),
        };
        println!("  level {probe:>10} ({what}): {verdict}");
        assert!(
            answer.is_err(),
            "level {probe} ({what}) returned a number where there is no level to place"
        );
    }

    // And the two indices that ARE placeable, so the refusals above are not the
    // whole story: the whole chain of a real record is still placed exactly.
    for mip in 0..env.mip_count {
        let offset =
            spore_texture::framing::level_offset(frame, env.width, env.height, mip, env.mip_count)
                .unwrap_or_else(|error| {
                    panic!("level {mip} of a real shape must be placeable: {error}")
                });
        assert!(
            offset < record_end,
            "level {mip} of a {record_end}-byte record is placed at {offset}"
        );
    }
    println!(
        "all {} declared levels of the shape are still placed inside the {record_end}-byte record",
        env.mip_count
    );

    // The boundary itself, from the other side: level 32 is the LAST placeable
    // index, so a chain of 33 gets it and a chain of 32 does not. No corpus record
    // is 33 levels long -- that is the measurement above -- so this is a property
    // of the ceiling rather than of the data, and it is pinned so the ceiling
    // cannot drift one level in either direction.
    let level_32 = spore_texture::MIP_FLOOR;
    assert_eq!(
        spore_texture::framing::level_offset(frame, env.width, env.height, level_32, level_32),
        Err(spore_texture::TextureError::LevelNotInChain {
            mip: level_32,
            mip_count: level_32
        }),
        "a chain of 32 levels declares indices 0..31, so level 32 does not exist"
    );
    let at_33 =
        spore_texture::framing::level_offset(frame, env.width, env.height, level_32, level_32 + 1);
    println!(
        "  level {level_32} of a {}-level chain: {}",
        level_32 + 1,
        match &at_33 {
            Ok(offset) => format!("placed at {offset}"),
            Err(error) => format!("REFUSED: {error}"),
        }
    );
    assert!(
        at_33.is_ok(),
        "level 32 is the last placeable index, and a chain of 33 declares it"
    );
}
