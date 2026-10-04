//! Hermetic tests: no game install, no network, no corpus.
//!
//! These are the tests that run under a plain `cargo test -p spore-differential`
//! and under `cargo test --workspace`. They must be cheap and they must not need
//! `SPORE/`. The only external thing they use is `python3` with the repository's
//! own `tools/spore/` oracles, because a differential harness that did not
//! actually talk to the oracle would not be testing anything.
//!
//! Each test here maps to one of the five required properties:
//!
//! 1. an absent `SPORE/` yields an empty corpus and `Skipped`, never a failure
//!    -- [`an_absent_game_install_skips_every_comparison`];
//! 2. the committed fixtures drive the harness to `Equal` --
//!    [`the_synthetic_corpus_reaches_equality_on_the_index_and_the_rw4_walk`];
//! 3. a `Divergence` quotes both sides --
//!    [`a_divergence_always_quotes_both_sides`];
//! 4. the float tolerance is applied *and reported* --
//!    [`the_float_tolerance_is_applied_and_reported`];
//! 5. a record both sides refuse is `BothFailed` --
//!    [`a_record_both_sides_refuse_is_agreement_not_divergence`].

use spore_differential::{
    compare, run_suite, summarize, Agreement, Corpus, DifferentialReport, Divergence,
    DivergenceClass, OracleConfig, RecordIdentity, Status, FLOAT_TOLERANCE,
};

/// The oracle configuration these tests run against.
fn config() -> OracleConfig {
    OracleConfig::detect()
}

/// A root that provably has no `SPORE/` tree, so `Corpus::discover` finds
/// nothing. Unique per process so a parallel test run cannot collide.
fn empty_root(tag: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!(
        "spore-differential-absent-{tag}-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&path).expect("temp dir");
    path
}

fn skipped_reason(report: &DifferentialReport) -> &str {
    match &report.status {
        Status::Skipped(reason) => reason,
        Status::Complete => panic!("{} was expected to be skipped", report.comparison),
    }
}

// ---------------------------------------------------------------- 1. absence

#[test]
fn an_absent_game_install_skips_every_comparison() {
    let root = empty_root("all");
    let corpus = Corpus::from_root(&root);

    assert!(
        !corpus.is_present(),
        "a root with no SPORE/ must yield an empty corpus, not an error"
    );
    assert!(corpus.packages().is_empty());
    assert_eq!(corpus.total_bytes(), 0);

    let reports = run_suite(&corpus, &config());
    assert_eq!(reports.len(), compare::ALL.len());
    for report in &reports {
        assert!(
            report.status.is_skipped(),
            "{} did not skip on an empty corpus: {}",
            report.comparison,
            report.status
        );
        assert!(
            !report.is_clean(),
            "{} must never look clean when skipped",
            report.comparison
        );
        assert!(!report.all_records_agree());
        assert_eq!(report.records_walked, 0, "{}", report.comparison);
        assert!(report.divergences.is_empty(), "{}", report.comparison);
        assert!(!skipped_reason(report).is_empty(), "{}", report.comparison);
    }
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn discover_itself_yields_an_empty_corpus_when_the_environment_points_nowhere() {
    let root = empty_root("discover");
    // `Corpus::discover()` reads OPENSPORE_ROOT, but this test must not SET it:
    // the variable is process-global, and `cargo test` runs the tests in a file
    // concurrently, so writing it here would race every other test in this
    // binary. `from_root` takes the same code path with the root as an argument,
    // which is what makes this assertable at all; the env var is only read, and
    // on a machine with the game installed this assertion would not hold, so it
    // is scoped to the explicit-root form.
    let corpus = Corpus::from_root(&root);
    assert!(!corpus.is_present());
    assert!(corpus.to_string().contains("empty"));

    // And `discover()` itself is total: it must return a corpus rather than
    // panic whether or not the tree is there. Which of the two it returns
    // depends on the machine, so only totality is asserted.
    let discovered = Corpus::discover();
    assert_eq!(
        discovered.packages().is_empty(),
        !discovered.is_present(),
        "an empty package list must mean an absent corpus and vice versa"
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn the_summary_of_a_skipped_suite_says_so_for_every_comparison() {
    let root = empty_root("summary");
    let text = summarize(&run_suite(&Corpus::from_root(&root), &config()));
    for name in compare::ALL {
        assert!(text.contains(name), "{name} missing from the summary");
        assert!(text.contains("skipped"), "{name}");
    }
    let _ = std::fs::remove_dir_all(&root);
}

// ------------------------------------------------------------- 2. fixtures

#[test]
fn the_synthetic_corpus_reaches_equality_on_the_index_and_the_rw4_walk() {
    let corpus = Corpus::synthetic().expect("the committed fixtures must exist");
    assert!(corpus.is_present());
    let config = config();

    // --- the DBPF index over the committed mini package.
    let Some(package) = corpus.packages().first() else {
        panic!("the synthetic corpus must hold the committed mini package");
    };
    let index = compare::dbpf_index::compare_package(package, &config)
        .expect("the oracle must answer for a committed fixture");
    assert_eq!(
        index.status,
        Status::Complete,
        "index comparison did not run: {}",
        index.summary()
    );
    assert_eq!(
        index.records_walked, 3,
        "mini_package.dbpf has three records"
    );
    assert!(
        index.all_records_agree(),
        "the committed mini package must agree row for row:\n{}",
        index.summary()
    );
    assert!(index.is_clean(), "{}", index.summary());

    // --- the QFS byte comparison over the same package.
    let bytes = compare::record_bytes::compare_package(package, &config, 16)
        .expect("the oracle must answer for a committed fixture");
    assert_eq!(bytes.status, Status::Complete);
    assert_eq!(bytes.records_walked, 3);
    assert!(bytes.all_records_agree(), "{}", bytes.summary());

    // --- the RW4 describe over the committed standalone rw4 record.
    let rw4_record = corpus
        .first_record_of(spore_differential::RecordKind::Rw4)
        .expect("the synthetic corpus must hold mini_rw4.rw4");
    assert!(compare::rw4::is_rw4_record(rw4_record));
    let rw4 = compare::rw4::compare_record_file(rw4_record, &config);
    assert_eq!(rw4.status, Status::Complete);
    assert_eq!(rw4.records_walked, 1, "one standalone record, one verdict");
    assert!(
        rw4.all_records_agree() && rw4.divergences.is_empty(),
        "the committed mini_rw4.rw4 must produce an identical describe line:\n{}",
        rw4.summary()
    );
    assert_eq!(rw4.outcome("equal"), 1);
    assert_eq!(rw4.outcome("both-failed"), 0);

    // --- and the gmdl walk over the committed standalone gmdl record, which
    //     proves the sampler and the field comparison run over real bytes.
    let gmdl_record = corpus
        .first_record_of(spore_differential::RecordKind::Gmdl)
        .expect("the synthetic corpus must hold mini.gmdl");
    assert!(compare::gmdl::is_gmdl_record(gmdl_record));
    let gmdl = compare::gmdl::compare_record_file(gmdl_record, &config);
    assert_eq!(gmdl.status, Status::Complete);
    assert!(
        gmdl.tolerance.is_some(),
        "a comparison that compared floats must name its tolerance:\n{}",
        gmdl.summary()
    );
    assert!(
        gmdl.tolerance
            .as_deref()
            .is_some_and(|t| t.contains("1e-4")),
        "{}",
        gmdl.summary()
    );
}

#[test]
fn a_fixture_run_produces_no_divergence_except_the_one_the_oracle_gets_wrong() {
    // Over the committed fixtures the Rust parsers and the Python oracles agree
    // completely -- with one exception, and it is a real defect in the oracle.
    //
    // # FINDING: `gmdl.py` reads the reference-table keys little-endian
    //
    // The oracle's own docstring states the rule it then breaks:
    //
    // ```text
    // refCount         u32  (BIG-endian; every other word is little-endian)
    //   refCount *     { instanceID, groupID, typeID } u32 x3
    // ```
    //
    // `u32be` is applied to the count and plain little-endian `u` to the three
    // words of every key behind it. The corpus settles which side is right:
    // over the 4 209 gmdl records of `Spore_Content.package`, the big-endian
    // reading of those keys resolves 7 989 of 7 989 against the package's own
    // index and the little-endian reading resolves 0 of 7 989 -- see
    // `tests/gmdl_reference_table.rs`. `spore-gmdl` reads them big-endian, which
    // is correct; the oracle does not.
    //
    // The committed fixture cannot show this on its own, because it was
    // *generated* from the oracle's assumption, so its key bytes are
    // little-endian too. What the fixture does show, and what this test pins, is
    // that the two sides now disagree about one word and one word only.
    //
    // This is asserted rather than tolerated: a test that quietly dropped the
    // comparison would report "clean" the moment either side was "fixed" in the
    // wrong direction, and the harness's whole value is that it cannot.
    let corpus = Corpus::synthetic().expect("the committed fixtures must exist");
    let config = config();
    let package = corpus.packages().first().expect("the mini package");
    let rw4_record = corpus
        .first_record_of(spore_differential::RecordKind::Rw4)
        .expect("mini_rw4.rw4");
    let gmdl_record = corpus
        .first_record_of(spore_differential::RecordKind::Gmdl)
        .expect("mini.gmdl");

    // The two file-level entry points fold the oracle error into a `Skipped`
    // report, so they are already reports; the two package-level ones return it.
    let agreeing = vec![
        compare::dbpf_index::compare_package(package, &config)
            .expect("the oracle must answer for a committed fixture"),
        compare::record_bytes::compare_package(package, &config, 16)
            .expect("the oracle must answer for a committed fixture"),
        compare::rw4::compare_record_file(rw4_record, &config),
    ];
    for report in &agreeing {
        assert_eq!(
            report.status,
            Status::Complete,
            "{} did not run: {}",
            report.comparison,
            report.summary()
        );
        assert!(
            report.divergences.is_empty(),
            "{} produced divergences:\n{}",
            report.comparison,
            report.summary()
        );
    }

    let gmdl = compare::gmdl::compare_record_file(gmdl_record, &config);
    assert_eq!(gmdl.status, Status::Complete, "{}", gmdl.summary());
    assert_eq!(
        gmdl.divergences.len(),
        1,
        "exactly one field disagrees, and it is the reference table's type word:\n{}",
        gmdl.summary()
    );
    let divergence = &gmdl.divergences[0];
    assert_eq!(
        divergence.field,
        "refs",
        "the divergence is the reference table, nothing else:\n{}",
        gmdl.summary()
    );
    // Both sides print the triple instance-first, so the values are directly
    // comparable and the difference is exactly one byte-reversal.
    assert!(
        divergence.oracle.contains("type=0x00e6bce5"),
        "the oracle read the type word little-endian: {divergence}"
    );
    assert!(
        divergence.rust.contains("type=0xe5bce600"),
        "spore_gmdl read it big-endian, and the fixture's bytes are little-endian: {divergence}"
    );
    // The instance and group words are byte-symmetric in this fixture, so they
    // cannot contribute to the disagreement -- which is why the fixture can show
    // the shape of the bug but not settle it.
    assert!(
        divergence.oracle.contains("inst=0x11111111"),
        "{divergence}"
    );
    assert!(divergence.rust.contains("inst=0x11111111"), "{divergence}");
    assert!(divergence.oracle.contains("grp=0x22222222"), "{divergence}");
    assert!(divergence.rust.contains("grp=0x22222222"), "{divergence}");
}

// ------------------------------------------------------- 3. quoting both sides

#[test]
fn a_divergence_always_quotes_both_sides() {
    let identity = RecordIdentity::new(
        "Spore_Content.package",
        41,
        0x00E6_BCE5,
        0x1234_5678,
        0x9ABC_DEF0,
    );
    let oracle = "0x1 obj=1 sec=3 buf=80 | 10030 d=0x138 s=12";
    let rust = "0x1 obj=1 sec=3 buf=80 | 10030 d=0x138 s=13";
    let divergence = Divergence::value(identity, "describe", oracle, rust);

    let text = divergence.to_string();

    // Both values, verbatim and quoted, so neither can be read as a paraphrase.
    assert!(text.contains(&format!("{oracle:?}")), "{text}");
    assert!(text.contains(&format!("{rust:?}")), "{text}");
    // The field path, so a reader knows what to go and look at.
    assert!(text.contains("describe"), "{text}");
    // The record identity, in full: package, index and the whole triple.
    assert!(text.contains("Spore_Content.package"), "{text}");
    assert!(text.contains("[41]"), "{text}");
    assert!(text.contains("t=0x00e6bce5"), "{text}");
    assert!(text.contains("g=0x12345678"), "{text}");
    assert!(text.contains("i=0x9abcdef0"), "{text}");
    // And the class, so a reader knows how much to trust it.
    assert!(text.contains("value-mismatch"), "{text}");

    assert_ne!(divergence.oracle, divergence.rust);
}

#[test]
fn a_divergence_carries_its_note_and_its_class_when_given() {
    let divergence = Divergence::value(
        RecordIdentity::loose("mini.gmdl"),
        "unk",
        "read by the oracle",
        "not exposed",
    )
    .with_class(DivergenceClass::Definitional)
    .with_note("a gap in the port's coverage, not a disagreement");
    let text = divergence.to_string();
    assert!(text.contains("definitional"), "{text}");
    assert!(text.contains("gap in the port's coverage"), "{text}");
    assert_eq!(divergence.class, DivergenceClass::Definitional);
    assert_eq!(divergence.class.label(), "definitional");
}

#[test]
fn a_byte_divergence_reports_the_first_offset_and_both_windows() {
    // The shape a decompression disagreement must take: where, and what.
    let rust = vec![0xAAu8; 32];
    let mut oracle = rust.clone();
    oracle[17] = 0xBB;
    assert_eq!(
        spore_differential::sample::sample_indices(1, 1),
        vec![0],
        "sanity: a one-record sample is that record"
    );
    // The helpers are public so a caller can format a divergence the same way
    // the comparison does.
    assert_eq!(
        find_diff(&rust, &oracle),
        17,
        "the first differing offset must be the one reported"
    );
}

fn find_diff(left: &[u8], right: &[u8]) -> usize {
    left.iter()
        .zip(right.iter())
        .position(|(a, b)| a != b)
        .unwrap_or_else(|| left.len().min(right.len()))
}

// --------------------------------------------------------- 4. the tolerance

#[test]
fn the_float_tolerance_is_applied_and_reported() {
    use spore_differential::floats::{agree, describe_tolerance, ReportBuilder};

    let identity = || RecordIdentity::new("Spore_Content.package", 7, 1, 2, 3);

    // (a) Inside the tolerance: Equal, and the report still names the tolerance.
    let mut inside = ReportBuilder::new("probe", "in-process");
    assert!(inside.float_field(identity(), "radius", 1.000_05, 1.0, FLOAT_TOLERANCE));
    let report = inside.finish();
    assert_eq!(report.outcome("equal"), 1);
    assert!(report.divergences.is_empty(), "{}", report.summary());
    assert_eq!(report.agreements, 1);
    let tolerance = report
        .tolerance
        .as_deref()
        .expect("the tolerance must be named");
    assert!(
        tolerance.contains("1e-4"),
        "tolerance not named: {tolerance}"
    );

    // (b) Outside the tolerance: Diverged, with both values and the measured
    //     difference quoted.
    let mut outside = ReportBuilder::new("probe", "in-process");
    assert!(!outside.float_field(identity(), "radius", 1.0, 1.5, FLOAT_TOLERANCE));
    let report = outside.finish();
    assert_eq!(report.outcome("diverged"), 1);
    assert_eq!(report.agreements, 0);
    let divergence = &report.divergences[0];
    assert_eq!(divergence.field, "radius");
    assert_eq!(divergence.oracle, "1.0");
    assert_eq!(divergence.rust, "1.5");
    let note = divergence
        .note
        .as_deref()
        .expect("a float divergence must say why");
    assert!(note.contains("1e-4"), "{note}");
    assert!(
        note.contains("0.5"),
        "the measured difference must be quoted: {note}"
    );
    assert!(report.tolerance.is_some());

    // (c) The two arms of the rule, so the tolerance is not a bare epsilon.
    assert!(agree(100_000.0, 100_005.0, FLOAT_TOLERANCE), "relative arm");
    assert!(agree(0.0, 1e-9, FLOAT_TOLERANCE), "absolute arm");
    assert!(!agree(0.0, 1e-2, FLOAT_TOLERANCE));

    // (d) Non-finite values are decided, not defaulted.
    assert!(agree(f64::NAN, f64::NAN, FLOAT_TOLERANCE), "two NaNs agree");
    assert!(!agree(f64::NAN, 0.0, FLOAT_TOLERANCE), "one NaN does not");

    assert!(describe_tolerance(FLOAT_TOLERANCE).contains("1e-4"));
}

// ---------------------------------------------------- 5. a shared refusal

#[test]
fn a_record_both_sides_refuse_is_agreement_not_divergence() {
    let agreement = Agreement::BothFailed {
        oracle: "ValueError: raster: unsupported fourcc 0x00001500".to_owned(),
        rust: "texture: unsupported fourcc 0x00001500 (want 0x35545844 = 'DXT5'; the 0x15xx \
               luminance family is a different format and is out of scope)"
            .to_owned(),
    };

    assert!(agreement.is_clean(), "two refusals are agreement");
    assert_eq!(agreement.label(), "both-failed");
    let text = agreement.to_string();
    assert!(
        text.contains("0x00001500"),
        "both messages must survive: {text}"
    );

    // And it must NOT look like a clean value agreement: the histogram is what
    // keeps "both refused" distinguishable from "both read the same value".
    let mut builder = spore_differential::floats::ReportBuilder::new("probe", "in-process");
    builder.record(agreement);
    let report = builder.finish();
    assert_eq!(report.outcome("both-failed"), 1);
    assert_eq!(report.outcome("equal"), 0);
    assert!(report.all_records_agree());
    assert!(report.divergences.is_empty());
}

#[test]
fn exactly_one_side_producing_a_value_is_a_finding() {
    use spore_differential::floats::ReportBuilder;
    let mut builder = ReportBuilder::new("probe", "in-process");
    builder.record(Agreement::OracleOnly(
        "the oracle decoded it; Rust refused".to_owned(),
    ));
    builder.record(Agreement::RustOnly(
        "Rust decoded it; the oracle refused".to_owned(),
    ));
    let report = builder.finish();
    assert_eq!(report.agreements, 0);
    assert_eq!(report.records_walked, 2);
    assert!(!report.all_records_agree());
    assert!(!report.is_clean());
}

// --------------------------------------------------------------- the harness

#[test]
fn a_report_that_walked_nothing_never_claims_every_record_agreed() {
    // `all_records_agree` on an empty report would be true for a vacuous reason,
    // which is exactly the kind of vacuous truth a differential harness must not
    // emit.
    let report = DifferentialReport::skipped("x", "nothing to do");
    assert!(!report.all_records_agree());
    assert!(!report.is_clean());
}

#[test]
fn the_divergence_list_is_capped_but_the_count_is_not() {
    let mut builder = spore_differential::floats::ReportBuilder::new("probe", "in-process")
        .with_max_divergences(3);
    for value in 0..100u32 {
        builder.divergence(Divergence::value(
            RecordIdentity::loose("many.package"),
            "field",
            value.to_string(),
            (value + 1).to_string(),
        ));
    }
    let report = builder.finish();
    assert_eq!(report.outcome("diverged"), 100, "the count must stay exact");
    assert!(report.divergences.len() <= 3, "the list must stay bounded");
    assert!(
        report.notes.iter().any(|note| note.contains("truncated")),
        "a truncated list must say so: {:?}",
        report.notes
    );
    assert!(
        report.summary().contains("showing 3 of 100 divergence(s)"),
        "a capped list must state the exact total it is a subset of: {}",
        report.summary()
    );
}

#[test]
fn the_oracle_config_names_what_to_set_when_the_oracles_are_missing() {
    let config = OracleConfig {
        python: "definitely-not-a-real-interpreter".to_owned(),
        tools_dir: std::path::PathBuf::from("/definitely/not/here"),
    };
    let reason = config
        .unavailable_reason()
        .expect("a missing tools dir must be reported as unavailable");
    assert!(reason.contains("OPENSPORE_ROOT"), "{reason}");
    assert!(!config.is_available());

    // And a suite run over a corpus it cannot check must skip, not fail.
    let root = empty_root("nooracle");
    let reports = run_suite(&Corpus::from_root(&root), &config);
    assert!(reports.iter().all(|report| report.status.is_skipped()));
    assert!(
        reports.iter().all(|report| !report.is_clean()),
        "an unreachable oracle must never look like agreement"
    );
    let _ = std::fs::remove_dir_all(&root);
}
