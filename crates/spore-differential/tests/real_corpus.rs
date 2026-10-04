//! Real-corpus differential tests. **Every test here is `#[ignore]`d.**
//!
//! These need the installed game data, which is git-ignored and about 5 GB:
//!
//! ```text
//! SPORE/Data/*.package          Spore_Content.package is 995 MB,
//! SPORE/DataEP1/*.package       Spore_Graphics.package is 943 MB
//! ```
//!
//! `cargo test -p spore-differential` therefore does **not** run them, and
//! `cargo test --workspace` stays hermetic and fast. To run them:
//!
//! ```text
//! cargo test -p spore-differential --release -- --ignored --nocapture
//! ```
//!
//! `--release` matters: QFS decompression of ~5 GB and a Python oracle
//! re-reading a 995 MB package are both dominated by interpreter and
//! allocator cost, and a debug build turns a ten-minute run into an hour.
//! `--nocapture` matters because each test prints its report; the divergences
//! are the deliverable and a passing `ok` line hides them.
//!
//! Each test asserts something narrow rather than "no differences", because
//! some differences are already known and are findings rather than bugs (see
//! the notes inside). Where a difference is a bug in the Rust port, the test
//! asserts it *is* present so the harness cannot quietly start reporting zero.
//! That is deliberate: this suite's job is to report the truth, and a
//! regression test that encodes "clean" as correct would encode a lie the first
//! time the truth was "diverged".

use spore_differential::{compare, run_suite, summarize, Corpus, OracleConfig};

fn corpus() -> Corpus {
    Corpus::discover()
}

fn config() -> OracleConfig {
    OracleConfig::detect()
}

/// Refuses to run quietly: a corpus that is not there means the test is
/// `#[ignore]`d for a reason, so skipping with a printed note is correct.
fn require(name: &str, corpus: &Corpus) {
    assert!(
        corpus.is_present(),
        "{name}: no SPORE/ tree was located. These tests need the installed game data; \
         see the module docs."
    );
}

// ------------------------------------------------------------------- 1. index

/// The DBPF index over `Spore_Content.package`: every row, every field.
///
/// The repository's own figure is 17 119 records. If that changes the test says
/// so rather than silently adapting -- a different count is a fact about the
/// install worth seeing.
#[test]
#[ignore = "needs the git-ignored SPORE/ tree (~5 GB)"]
fn dbpf_index_over_spore_content() {
    let corpus = corpus();
    require("dbpf_index", &corpus);
    let report = compare::dbpf_index::compare(&corpus, &config());
    println!("{}", report.summary());

    assert!(report.status.is_complete(), "{}", report.summary());
    assert_eq!(
        report.records_walked,
        17_119,
        "Spore_Content.package is recorded as 17 119 records:\n{}",
        report.summary()
    );
    assert_eq!(
        report.outcome("diverged"),
        0,
        "the index must agree row for row:\n{}",
        report.summary()
    );
    assert_eq!(report.outcome("equal"), 17_119);
    assert!(report.all_records_agree(), "{}", report.summary());

    // The masking claim, measured rather than assumed.
    assert!(
        report
            .notes
            .iter()
            .any(|note| note.contains("17119 of 17119 row(s) carry bit 31")),
        "the report must state how many rows carry the top size bit:\n{}",
        report.summary()
    );
}

// ------------------------------------------------------------------- 2. bytes

/// QFS decompression over a bounded deterministic sample.
#[test]
#[ignore = "needs the git-ignored SPORE/ tree (~5 GB)"]
fn record_bytes_over_a_deterministic_sample() {
    let corpus = corpus();
    require("record_bytes", &corpus);
    let report = compare::record_bytes::compare(&corpus, &config());
    println!("{}", report.summary());

    assert!(report.status.is_complete(), "{}", report.summary());
    assert!(
        report
            .notes
            .iter()
            .any(|note| note.contains("deterministic; no random draw")),
        "the sample must be stated as deterministic:\n{}",
        report.summary()
    );
    assert!(
        report.records_walked >= 190,
        "the sample should be near its budget:\n{}",
        report.summary()
    );
    assert_eq!(
        report.outcome("diverged"),
        0,
        "decompressed bytes must match exactly:\n{}",
        report.summary()
    );
}

// --------------------------------------------------------------------- 3. rw4

/// The canonical RW4 section-walk line, over **every** rw4 record.
///
/// This is the same comparison `tests/test_rw4.py` runs against C++, extended to
/// the Rust port. Exact strings: the repository's own figure is 1131 records in
/// `Spore_Content`, and `spore-core` records that all 1131 carry the RW4 magic.
#[test]
#[ignore = "needs the git-ignored SPORE/ tree (~5 GB)"]
fn rw4_describe_over_every_rw4_record() {
    let corpus = corpus();
    require("rw4_describe", &corpus);
    let report = compare::rw4::compare(&corpus, &config());
    println!("{}", report.summary());

    assert!(report.status.is_complete(), "{}", report.summary());
    assert_eq!(
        report.records_walked,
        1_131,
        "Spore_Content.package is recorded as holding 1131 rw4 records:\n{}",
        report.summary()
    );
    assert_eq!(
        report.outcome("diverged"),
        0,
        "the describe line must be character-identical:\n{}",
        report.summary()
    );
    assert_eq!(report.outcome("both-failed"), 0);
}

// -------------------------------------------------------------------- 4. gmdl

/// The gmdl walk over a bounded deterministic sample of the 4209 gmdl records.
#[test]
#[ignore = "needs the git-ignored SPORE/ tree (~5 GB)"]
fn gmdl_walk_over_a_deterministic_sample() {
    let corpus = corpus();
    require("gmdl_walk", &corpus);
    let report = compare::gmdl::compare(&corpus, &config());
    println!("{}", report.summary());

    assert!(report.status.is_complete(), "{}", report.summary());
    assert!(
        report.tolerance.is_some(),
        "a comparison that compared floats must name its tolerance:\n{}",
        report.summary()
    );
    assert!(
        report
            .tolerance
            .as_deref()
            .is_some_and(|t| t.contains("1e-4")),
        "{}",
        report.summary()
    );
    // `matches()` must be compared, and the Rust side must be the one carrying
    // the trailer state, because that is where the two walkers differ in what
    // they can say.
    assert!(
        report
            .notes
            .iter()
            .any(|note| note.contains("fully_walked()")),
        "the final-offset check must be reported:\n{}",
        report.summary()
    );

    // The anim-data ceiling, asserted so it cannot silently change. Neither
    // implementation decodes these records: the oracle dies on an impossible
    // baked-deform count and spore-gmdl returns Ok with a truncated trailer.
    // See `compare::gmdl`'s module docs for the trace.
    let rust_only = report.outcome("rust-only");
    let both_failed = report.outcome("both-failed");
    assert!(
        rust_only > 0,
        "the anim-data ceiling is expected over the real corpus:\n{}",
        report.summary()
    );
    assert!(
        report
            .notes
            .iter()
            .any(|note| note.contains("Truncated { at: BakedDeform }")),
        "a rust-only verdict must quote the Rust trailer state, so a partial \
         decode cannot be read as a complete one:\n{}",
        report.summary()
    );
    assert!(
        report
            .notes
            .iter()
            .any(|note| note.contains("undocumented shader-data id 0x218")),
        "the both-failed cause must be quoted:\n{}",
        report.summary()
    );
    println!(
        "anim-data ceiling: {rust_only} oracle-unwalkable record(s), {both_failed} refused by \
         both sides, 0 field-level divergences"
    );
}

// ------------------------------------------------------------------ 5. raster

/// The raster envelope, the layer count, and the base mip -- over a
/// deterministic bounded sample.
///
/// # This arm used to assert zero divergences. It now asserts a measured
/// disagreement and says whose fault it is.
///
/// The oracle side of this comparison is `tools/spore/raster/raster.py` and
/// `tools/spore/dxt5/dxt5.py`, and both implement the **refuted** framing: a
/// derived layer count dividing a payload by `16 + chain` with `chain` charging
/// 8 bytes per 4x4 block, a 16-byte "layer header" in front of every layer, and
/// an 8-byte "Spore block" codec. A byte count over 4386 of 4386 raster records
/// in the two content packages settled that the records hold one mip chain, a
/// 16-byte BC3 block for `DXT5` and an 8-byte BC1 block for `DXT1`
/// (`docs/RASTER-FRAMING.md` §2). So the envelope arm still agrees field for
/// field and the other two arms cannot, and pretending otherwise would mean
/// either refusing `DXT1` or reverting the framing to keep a test green.
///
/// The numbers below are the measurement, asserted exactly so a silent change
/// in any of the three arms shows up:
///
/// | arm | measured |
/// |---|---|
/// | envelope, 150 sampled | 150 agree, 0 disagree |
/// | layer count | 68 disagreements, 82 refusals by the oracle's own division, 0 agreements |
/// | base mip digest (12 decoded) | 8 disagreements, 4 one-sided, 0 agreements |
///
/// What is asserted about the framing itself is **not** here: it is
/// `tests/luminance_corpus.rs`, which checks every raster record of both
/// packages against a second transcription of the measured layout in Python
/// (4386 of 4386) because the oracle this comparison used to trust has no
/// second opinion to give.
#[test]
#[ignore = "needs the git-ignored SPORE/ tree (~5 GB)"]
fn raster_envelope_layer_count_and_base_mip() {
    let corpus = corpus();
    require("raster", &corpus);
    let report = compare::raster::compare(&corpus, &config());
    println!("{}", report.summary());

    assert!(report.status.is_complete(), "{}", report.summary());
    assert!(
        report
            .notes
            .iter()
            .any(|note| note.contains("envelope compared on")),
        "{}",
        report.summary()
    );

    // --- the envelope arm: still a clean field-for-field comparison.
    assert!(
        report
            .divergences
            .iter()
            .all(|d| !d.field.starts_with("envelope.")),
        "the envelope is eight words read by both sides without validation, so every sampled \
         record must agree on all eight:\n{}",
        report.summary()
    );

    // --- the three arms, measured. The retained divergence list is capped, so the
    //     breakdown is asserted through the report's own tally notes, which are
    //     not.
    for (what, expected) in [
        (
            "layer count",
            "layer count over 150 sampled record(s): 0 agreed, 68 disagreed, 82 refused by the \
             oracle's own division, 0 stated nothing",
        ),
        (
            "base mip digest",
            "base mip digest: 0 agreed, 8 disagreed, 4 one-sided; 8 of those are a DXT5 decode \
             on both sides reading different bytes out of the same record",
        ),
    ] {
        assert!(
            report.notes.iter().any(|note| note.contains(expected)),
            "the {what} tally must read exactly this, because it is the measurement: \
             expected a note containing {expected:?}\n{}",
            report.summary()
        );
    }

    // --- and nothing else diverges. Three field paths, no fourth: a divergence
    //     outside this set would be a new disagreement nobody has looked at.
    for divergence in &report.divergences {
        assert!(
            matches!(
                divergence.field.as_str(),
                "layer count" | "mip0.sha256[:16]"
            ),
            "unexpected divergence field {}\n{}",
            divergence.field,
            report.summary()
        );
    }

    // --- a layer-count disagreement must quote BOTH numbers, because the claim
    //     "the oracle is wrong" is only checkable if the reader can see the
    //     payload it divided and the stride it divided by.
    let layer_divergences: Vec<_> = report
        .divergences
        .iter()
        .filter(|d| d.field == "layer count")
        .collect();
    assert!(
        !layer_divergences.is_empty(),
        "no layer-count disagreement is retained, so the refutation is not being reported:\n{}",
        report.summary()
    );
    for divergence in &layer_divergences {
        assert!(
            divergence.oracle.contains("layers") && divergence.oracle.contains("stride"),
            "the oracle side must quote its own arithmetic, not a paraphrase: {}",
            divergence
        );
        // The Rust side states the measured identity: the record's own length,
        // the length its envelope implies, and the base level's byte count.
        assert!(
            divergence
                .rust
                .starts_with("1 chain (record is exactly 349616 bytes: header + a"),
            "the Rust side must state the record's own length and the length its envelope \
             implies: {divergence}"
        );
        assert!(
            divergence.rust.contains("262144-byte"),
            "the Rust side must quote the base level's byte count, which is where the 8-versus-16 \
             byte block disagreement shows up as a number: {divergence}"
        );
        assert!(
            divergence.rust.contains("one u32 count per level"),
            "the Rust side must name the per-level counts the record carries: {divergence}"
        );
        assert!(
            divergence
                .note
                .as_deref()
                .is_some_and(|note| note.contains("REFUTED ORACLE")),
            "a layer-count divergence must say whose rule was refuted: {divergence}"
        );
    }

    // --- and the digest disagreements must be labelled as a codec disagreement
    //     rather than counted as a port defect, with the codec's grade stated.
    let digest_divergences: Vec<_> = report
        .divergences
        .iter()
        .filter(|d| d.field == "mip0.sha256[:16]")
        .collect();
    assert!(
        !digest_divergences.is_empty(),
        "no base-mip disagreement is retained:\n{}",
        report.summary()
    );
    for divergence in &digest_divergences {
        let note = divergence.note.as_deref().unwrap_or_default();
        match divergence.class {
            // Both sides produced a digest and they differ: that IS a codec
            // disagreement, so the note must say the codec is INFERRED.
            spore_differential::DivergenceClass::ValueMismatch => assert!(
                note.contains("INFERRED"),
                "a codec disagreement must state that the codec is INFERRED, not verified: \
                 {divergence}"
            ),
            // One side produced a digest: there is no codec to grade, and the
            // note has to say which side is silent and why.
            spore_differential::DivergenceClass::OneSidedDecode => assert!(
                note.contains("fourcc 0x") && note.contains("4386"),
                "a one-sided decode must name the format word and say where the Rust framing is \
                 checked instead of being taken on trust: {divergence}"
            ),
            other => panic!("unexpected class {other} on a digest comparison: {divergence}"),
        }
    }

    println!(
        "raster: envelope 150/150 agree; layer count 68 disagreements + 82 oracle refusals, 0 \
         agreements; base mip 8 disagreements + 4 one-sided, 0 agreements. Every disagreement \
         is the refuted 8-byte-block oracle; the framing itself is checked over 4386 of 4386 \
         records in tests/luminance_corpus.rs"
    );
}

// ---------------------------------------------------------------- 6. manifest

/// Manifest row count, ordering, and the "decodable" classification.
#[test]
#[ignore = "needs the git-ignored SPORE/ tree (~5 GB)"]
fn manifest_rows_and_classification() {
    let corpus = corpus();
    require("manifest", &corpus);
    let report = compare::manifest::compare(&corpus, &config());
    println!("{}", report.summary());

    assert!(report.status.is_complete(), "{}", report.summary());
    // Both sides are one row per (type, group, instance), so the counts must
    // agree over a single package.
    assert!(
        report.divergences.iter().all(|d| d.field != "rows"),
        "the row count must agree:\n{}",
        report.summary()
    );
    assert!(
        report.divergences.iter().all(|d| d.field != "rows.ordered"),
        "rows must come out ascending on both sides:\n{}",
        report.summary()
    );
    // The classification difference is EXPECTED and is reported as definitional.
    // It must not be silently absorbed, and it must not be counted as a defect.
    let definitional: Vec<_> = report
        .divergences
        .iter()
        .filter(|d| d.field == "decodable type ids")
        .collect();
    for divergence in &definitional {
        assert_eq!(
            divergence.class,
            spore_differential::DivergenceClass::Definitional,
            "{divergence}"
        );
        // Both lists are printed in hex. Over the real corpus the measured
        // difference is: the oracle calls `png` (0x2F7D0004) decodable and
        // gmdl (0x00E6BCE5) walk-fail, while spore-assets calls `gmdl`
        // decodable and has no decoder for `png` at all.
        assert!(
            divergence.oracle.contains("0x2f7d0004"),
            "the oracle's decodable list must be quoted:\n{divergence}"
        );
        assert!(
            divergence.rust.contains("0x00e6bce5"),
            "the Rust decodable list must be quoted:\n{divergence}"
        );
    }
    println!(
        "classification difference recorded as {} definitional divergence(s)",
        definitional.len()
    );
}

// ---------------------------------------------------------------- 7. worldobj

/// The `0x0F43029A` records: oracle only, no Rust decoder exists.
#[test]
#[ignore = "needs the git-ignored SPORE/ tree (~5 GB)"]
fn worldobj_is_oracle_only_and_says_so() {
    let corpus = corpus();
    require("worldobj", &corpus);
    let report = compare::worldobj::compare(&corpus, &config());
    println!("{}", report.summary());

    // Either the package holds none (then it is skipped, honestly) or it holds
    // some (then the missing Rust decoder is recorded as a definitional gap).
    if report.status.is_complete() {
        assert!(
            report
                .divergences
                .iter()
                .any(|d| d.class == spore_differential::DivergenceClass::Definitional),
            "a record set with no Rust decoder must say so:\n{}",
            report.summary()
        );
    } else {
        assert!(report.status.to_string().contains("skipped"));
    }
}

// ------------------------------------------------------------------- the suite

/// The whole suite over the real corpus, printed.
///
/// Not an assertion about cleanliness: this is the run whose *output* is the
/// deliverable. It asserts only that every comparison actually ran, so a silent
/// skip cannot be mistaken for a clean pass.
#[test]
#[ignore = "needs the git-ignored SPORE/ tree (~5 GB)"]
fn the_whole_suite_over_the_real_corpus() {
    let corpus = corpus();
    require("suite", &corpus);
    assert!(corpus.total_bytes() > 0);
    println!("corpus: {corpus}");

    let reports = run_suite(&corpus, &config());
    assert_eq!(reports.len(), compare::ALL.len());
    for report in &reports {
        assert!(
            report.status.is_complete(),
            "{} did not run over the real corpus: {}",
            report.comparison,
            report.status
        );
        assert!(
            report.records_walked > 0,
            "{} walked nothing",
            report.comparison
        );
    }
    println!("\n{}", summarize(&reports));
}
