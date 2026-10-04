//! `stats`: the censuses, their coverage and their determinism.

mod common;

use common::{osptool, package_of, Row};

use spore_tools::{EXIT_OK, EXIT_USAGE};

/// A package holding one record of every kind the census distinguishes: a
/// decodable raster, a `plt` with no decoder, a container, and the committed
/// gmdl.
fn mixed_package() -> Vec<u8> {
    package_of(&[
        Row::new(
            spore_texture::RASTER_TYPE,
            0x4066_2900,
            1,
            common::synthetic_raster(8, 8, 1),
        ),
        Row::new(
            spore_core::record::type_id::PLT,
            0x406b_6a00,
            2,
            vec![0x11; 40],
        ),
        Row::new(spore_core::record::type_id::PROP, 0x0, 3, vec![0x22; 8]),
        Row::new(
            spore_gmdl::GMDL_TYPE,
            0x4061_6201,
            4,
            common::read_fixture("mini.gmdl"),
        ),
    ])
}

#[test]
fn the_report_states_its_scope_and_labels_each_section_with_what_it_measured() {
    let file = common::temp_package("stats-scope", &mixed_package());
    let path = file.path().display().to_string();
    let (code, out, err) = osptool(&["stats", &path]);
    assert_eq!(code, EXIT_OK, "{err}");
    assert!(out.contains("records walked 4"), "{out}");
    assert!(out.contains("scope          every record"), "{out}");
    assert!(
        out.contains("types  (counted from the index; not a decode)"),
        "{out}"
    );
    assert!(
        out.contains("raster fourcc  (read from the 32-byte envelope"),
        "{out}"
    );
    assert!(out.contains("raster framing  (record length vs"), "{out}");
    assert!(
        !out.contains("raster layer splits"),
        "that section could only ever print `derived` once the corpus was measured, and a \
         section that can only print one thing is not a measurement: {out}"
    );
    assert!(out.contains("gmdl walk outcomes"), "{out}");
    assert!(out.contains("texture-set sizes"), "{out}");
    assert!(
        out.contains("decodes  (one full decode per record"),
        "{out}"
    );
}

#[test]
fn a_type_scope_narrows_the_walk_and_says_so() {
    let file = common::temp_package("stats-type", &mixed_package());
    let path = file.path().display().to_string();
    let (code, out, err) = osptool(&["stats", &path, "--type", "0x2f4e681c"]);
    assert_eq!(code, EXIT_OK, "{err}");
    assert!(out.contains("records walked 1"), "{out}");
    assert!(
        out.contains("scope          type 0x2f4e681c (raster)"),
        "{out}"
    );
    assert!(
        out.contains("0x35545844 DXT5/BC3 \u{2014} 16 bytes per 4x4 block (decoded)"),
        "{out}"
    );
}

#[test]
fn a_dxt1_record_is_censused_as_decoded_and_never_labelled_refused() {
    // The corpus refuted the old premise here. `0x31545844` used to print "DXT1
    // (present in the corpus, refused by name)" while `spore_texture` decoded all
    // 306 of its records as BC1; a census that calls a decodable format refused
    // is a confident falsehood, and this asserts against one.
    let image = package_of(&[Row::new(
        spore_texture::RASTER_TYPE,
        0x4066_2900,
        1,
        common::synthetic_raster_of(spore_texture::DXT1_FOURCC, 8, 8, 2),
    )]);
    let file = common::temp_package("stats-dxt1", &image);
    let (code, out, err) = osptool(&["stats", &file.path().display().to_string()]);
    assert_eq!(code, EXIT_OK, "{err}");
    assert!(
        out.contains("0x31545844 DXT1/BC1 \u{2014} 8 bytes per 4x4 block (decoded)"),
        "{out}"
    );
    assert!(
        !out.contains("refused by name"),
        "DXT1 decodes; the census must not say otherwise: {out}"
    );
    // And the decode section agrees, in the decoder's own words.
    assert!(out.contains("raster decoded"), "{out}");
    assert!(
        !out.contains("texture: unsupported fourcc"),
        "no format in the corpus is unsupported any more: {out}"
    );
}

#[test]
fn the_framing_cross_check_is_reported_for_every_raster_record() {
    // The section that replaced `raster layer splits`. It is the measurement that
    // refuted the two-layer reading, so it has to be reproducible from the tool
    // rather than asserted in a comment: the record length must equal
    // `framing::record_size`, and `field_18` must be the base level's own bytes.
    let image = package_of(&[
        Row::new(
            spore_texture::RASTER_TYPE,
            0x4066_2900,
            1,
            common::synthetic_raster(8, 8, 1),
        ),
        // A DXT1 record, so both block sizes are covered by the same run.
        Row::new(
            spore_texture::RASTER_TYPE,
            0x4066_2900,
            2,
            common::synthetic_raster_of(spore_texture::DXT1_FOURCC, 8, 8, 1),
        ),
        // The uncompressed family, whose header is 28 bytes and whose level 1 is
        // prefixed where the block formats' is not.
        Row::new(
            spore_texture::RASTER_TYPE,
            0x4066_2900,
            3,
            common::synthetic_raster_of(spore_texture::LUMINANCE_FOURCC, 8, 8, 2),
        ),
    ]);
    let file = common::temp_package("stats-framing", &image);
    let (code, out, err) = osptool(&["stats", &file.path().display().to_string()]);
    assert_eq!(code, EXIT_OK, "{err}");
    assert!(
        out.contains("raster framing  (record length vs framing::record_size"),
        "{out}"
    );
    assert!(
        out.contains("record length: exact"),
        "every synthetic record here satisfies the identity: {out}"
    );
    assert!(
        out.contains("field_18: the base level's byte count"),
        "and field_18 agrees with the geometry in all three formats: {out}"
    );
    // No layer count in any raster row: it is the refuted premise. Scoped to the
    // rows, because the section's own title says the word in order to deny it.
    let geometry_rows = numeric_rows(&out, "raster geometry", "raster framing");
    // Two rows, not three: the geometry census is keyed on `(width, height,
    // mips)` alone, so the DXT5 and the DXT1 record — same shape, different
    // format — share one row and the fourcc census is what separates them. The
    // framing rows below do carry the format, in the `field_18` axis.
    assert_eq!(geometry_rows.len(), 2, "{geometry_rows:?}");
    assert!(
        geometry_rows[0].contains("2") && geometry_rows[0].contains("8x8 mips=1"),
        "{geometry_rows:?}"
    );
    assert!(geometry_rows[1].contains("8x8 mips=2"), "{geometry_rows:?}");
    assert!(
        geometry_rows.iter().all(|row| !row.contains("layer")),
        "{geometry_rows:?}"
    );
    assert!(
        !geometry_rows.iter().any(|row| row.contains("chains=1")),
        "there is no chain count in a geometry row: {geometry_rows:?}"
    );
}

/// The histogram rows of one section, without its (multi-line) title.
///
/// Every `write_section` title is prose that may legitimately *mention* the very
/// word a test is asserting is absent — `raster geometry` says "there is no layer
/// count" precisely in order to deny one — so a substring search over the whole
/// section would test the sentence rather than the data. Rows are the lines whose
/// first non-space character is a digit, which is the shape `write_section`
/// prints them in.
fn numeric_rows<'a>(out: &'a str, section: &str, next_section: &str) -> Vec<&'a str> {
    let body = out
        .split_once(section)
        .unwrap_or_else(|| panic!("no `{section}` section in:\n{out}"))
        .1;
    let body = body.split_once(next_section).map_or(body, |(head, _)| head);
    body.lines()
        .filter(|line| line.trim_start().starts_with(|c: char| c.is_ascii_digit()))
        .collect()
}

#[test]
fn a_record_whose_length_disagrees_with_its_own_envelope_is_still_in_both_censuses() {
    // The "both censuses" property, preserved and sharpened.
    //
    // It used to be observable in the corpus: 373 records had a readable
    // envelope whose length did not match the framing the envelope implied, and
    // they fell out of the fourcc census — a silent hole, and the census then read
    // "1374 DXT5" where the truth is 1747. The framing is now correct, so the
    // corpus can no longer show the property, and a test that only asserted it
    // over the corpus would have silently stopped testing anything.
    //
    // So it is pinned here with a record whose length deliberately disagrees: the
    // fourcc and the three geometry words come out of the envelope and must
    // still be counted, and the disagreement must be reported by name rather than
    // swallowing the record.
    let mut record = common::synthetic_raster(8, 8, 1);
    // 8x8 DXT5 with one level is a 32-byte header plus 4 blocks of 16 = 96 bytes.
    // Adding 16 makes the record 112, which nothing implies.
    assert_eq!(record.len(), 96);
    record.extend_from_slice(&[0xAB; 16]);
    let image = package_of(&[Row::new(spore_texture::RASTER_TYPE, 0x4066_2900, 1, record)]);
    let file = common::temp_package("stats-misframed", &image);
    let (code, out, err) = osptool(&["stats", &file.path().display().to_string()]);
    assert_eq!(code, EXIT_OK, "{err}");

    // In the fourcc census: the envelope was read, so the fourcc is established.
    assert!(out.contains("0x35545844 DXT5/BC3"), "{out}");
    // In the geometry census: so are the three geometry words.
    assert!(out.contains("8x8 mips=1"), "{out}");
    // And the disagreement is named with both numbers rather than dropped.
    assert!(
        out.contains("record length: texture: record is 112 bytes but fourcc 0x35545844"),
        "{out}"
    );
    // `field_18` is a separate axis and still closed: the header word agrees with
    // the geometry even though the record's length does not.
    assert!(
        out.contains("field_18: the base level's byte count"),
        "one cross-check passing must not be allowed to mask the other: {out}"
    );
    // And the decode section counts it as a failure, with the crate's words.
    assert!(out.contains("texture: record is 112 bytes"), "{out}");

    // Coverage: the fourcc census still covers the record exactly once.
    let fourcc_lines: Vec<&str> = out
        .lines()
        .skip_while(|line| !line.contains("raster fourcc"))
        .take(4)
        .filter(|line| line.trim_start().starts_with(|c: char| c.is_ascii_digit()))
        .collect();
    let total: u64 = fourcc_lines
        .iter()
        .filter_map(|line| line.split_whitespace().next())
        .filter_map(|word| word.parse::<u64>().ok())
        .sum();
    assert_eq!(total, 1, "{fourcc_lines:?}");
}

#[test]
fn a_fourcc_with_no_framing_rule_is_censused_as_an_absence_and_not_decoded() {
    // A readable envelope whose format this build has no measured rule for. Its
    // fourcc and geometry are established; the framing is an **absence**, which is
    // not a refusal of the record and is not folded into any of the three
    // formats' rows.
    let mut record = Vec::new();
    for value in [1u32, 4, 4, 1, 8, 0xDEAD_BEEF, 0, 0] {
        record.extend_from_slice(&value.to_le_bytes());
    }
    record.extend_from_slice(&[0u8; 32]);
    let image = package_of(&[Row::new(spore_texture::RASTER_TYPE, 0x4066_2900, 1, record)]);
    let file = common::temp_package("stats-nofourcc", &image);
    let (code, out, err) = osptool(&["stats", &file.path().display().to_string()]);
    assert_eq!(code, EXIT_OK, "{err}");
    assert!(
        out.contains(
            "0xdeadbeef (this build has no measured framing rule for it) \u{2014} not decoded"
        ),
        "{out}"
    );
    assert!(
        out.contains(
            "record length: no framing rule for fourcc 0xdeadbeef (an absence, not a refusal)"
        ),
        "{out}"
    );
    // No `field_18` verdict: with no frame there is no base level to size, and
    // inventing one would be the old derivation's error. Scoped to the rows,
    // because the section's own title names the two cross-checks it reports.
    let framing_rows = numeric_rows(&out, "raster framing", "decodes  (one full decode");
    assert_eq!(framing_rows.len(), 1, "{framing_rows:?}");
    assert!(
        !framing_rows.iter().any(|row| row.contains("field_18")),
        "{framing_rows:?}"
    );
    // And the decode section refuses it in the crate's words.
    assert!(out.contains("texture: unsupported fourcc"), "{out}");
}

#[test]
fn a_raster_record_too_short_for_an_envelope_is_counted_and_never_folded_in() {
    let image = package_of(&[
        Row::new(spore_texture::RASTER_TYPE, 1, 1, vec![0u8; 16]),
        Row::new(
            spore_texture::RASTER_TYPE,
            1,
            2,
            common::synthetic_raster(8, 8, 1),
        ),
    ]);
    let file = common::temp_package("stats-short", &image);
    let (code, out, err) = osptool(&["stats", &file.path().display().to_string()]);
    assert_eq!(code, EXIT_OK, "{err}");
    assert!(
        out.contains("<envelope unreadable>"),
        "a record with no envelope is counted, not dropped: {out}"
    );
    // Both records appear exactly once across the fourcc census, so the two
    // sections cannot disagree about coverage.
    let fourcc_lines: Vec<&str> = out
        .lines()
        .skip_while(|line| !line.contains("raster fourcc"))
        .take(4)
        .filter(|line| line.trim_start().starts_with(|c: char| c.is_ascii_digit()))
        .collect();
    let total: u64 = fourcc_lines
        .iter()
        .filter_map(|line| line.split_whitespace().next())
        .filter_map(|word| word.parse::<u64>().ok())
        .sum();
    assert_eq!(
        total, 2,
        "every raster record is in the census: {fourcc_lines:?}"
    );
}

#[test]
fn two_runs_print_the_same_bytes() {
    let file = common::temp_package("stats-determinism", &mixed_package());
    let path = file.path().display().to_string();
    let args = ["stats", &path];
    let (first_code, first, first_err) = osptool(&args);
    let (second_code, second, second_err) = osptool(&args);
    assert_eq!(first_code, EXIT_OK, "{first_err}");
    assert_eq!(second_code, EXIT_OK, "{second_err}");
    assert_eq!(
        first, second,
        "every histogram is sorted by (count desc, label asc), so two runs must agree"
    );
}

#[test]
fn the_json_document_is_json_and_carries_every_section() {
    let file = common::temp_package("stats-json", &mixed_package());
    let path = file.path().display().to_string();
    let (code, out, err) = osptool(&["stats", &path, "--json"]);
    assert_eq!(code, EXIT_OK, "{err}");
    let document = spore_tools::json::parse(&out).expect("valid JSON on stdout");
    let spore_tools::json::Json::Obj(pairs) = &document else {
        panic!("expected an object")
    };
    for key in [
        "index_rows",
        "stored_bytes",
        "packages",
        "types",
        "groups",
        "raster_fourcc",
        "raster_geometry",
        "raster_framing",
        "decodes",
        "gmdl_walk",
        "texture_set_sizes",
    ] {
        assert!(
            pairs.iter().any(|(name, _)| name == key),
            "the census must carry `{key}`: {out}"
        );
    }
    // And stdout carries only the document: nothing else was printed there.
    assert!(!out.contains("raster fourcc  (read"), "{out}");
}

#[test]
fn a_container_and_a_no_decoder_type_are_counted_but_not_decoded() {
    let file = common::temp_package("stats-nodecode", &mixed_package());
    let path = file.path().display().to_string();
    let (code, out, err) = osptool(&["stats", &path]);
    assert_eq!(code, EXIT_OK, "{err}");
    // The type histogram counts every row...
    assert!(out.contains("0x00b1b104 (prop)"), "{out}");
    assert!(out.contains("0x011989b7 (plt)"), "{out}");
    // ...and the decode census says nothing about them, because nothing was
    // read. A container row appearing under `decodes` would be a false claim.
    let decodes = out
        .split("decodes  (one full decode")
        .nth(1)
        .expect("the decodes section");
    let decodes = decodes.split("gmdl walk outcomes").next().unwrap_or("");
    assert!(!decodes.contains("prop"), "{decodes}");
    assert!(!decodes.contains("plt"), "{decodes}");
}

#[test]
fn the_limit_caps_a_histogram_and_zero_means_every_row() {
    let file = common::temp_package("stats-limit", &mixed_package());
    let path = file.path().display().to_string();
    let (code, out, err) = osptool(&["stats", &path, "--limit", "1"]);
    assert_eq!(code, EXIT_OK, "{err}");
    assert!(out.contains("further row(s) not shown"), "{out}");

    let (_code, all, _err) = osptool(&["stats", &path, "--limit", "0"]);
    assert!(!all.contains("further row(s) not shown"), "{all}");
}

#[test]
fn the_decode_section_counts_exactly_what_verify_counts() {
    // Two commands, one corpus, one answer. `stats`'s `decodes` section used to
    // measure `spore_gmdl::parse` alone while `verify` measured parse *and*
    // per-mesh extraction, which on the real corpus made them report 619 and 737
    // failing gmdl records respectively. Both numbers were defensible; having two
    // is the defect.
    let file = common::temp_package("stats-agrees", &mixed_package());
    let path = file.path().display().to_string();

    let (_code, census, err) = osptool(&["stats", &path, "--json"]);
    assert_eq!(_code, EXIT_OK, "{err}");
    let document = spore_tools::json::parse(&census).expect("valid JSON");
    let spore_tools::json::Json::Obj(pairs) = &document else {
        panic!("expected an object")
    };
    let decodes = pairs
        .iter()
        .find(|(k, _)| k == "decodes")
        .map(|(_, v)| v.clone())
        .expect("decodes key");
    let spore_tools::json::Json::Arr(rows) = decodes else {
        panic!("decodes must be an array")
    };
    let census_total: i64 = rows
        .iter()
        .filter_map(|row| {
            let spore_tools::json::Json::Obj(fields) = row else {
                return None;
            };
            fields
                .iter()
                .find(|(k, _)| k == "count")
                .and_then(|(_, v)| match v {
                    spore_tools::json::Json::Int(n) => Some(*n),
                    _ => None,
                })
        })
        .sum();

    let (_code, tally, err) = osptool(&["verify", &path]);
    assert_eq!(_code, EXIT_OK, "{err}");
    let read = |label: &str| -> u64 {
        tally
            .lines()
            .find_map(|line| line.strip_prefix(label))
            .and_then(|value| value.trim().parse().ok())
            .unwrap_or_else(|| panic!("no `{label}` line: {tally}"))
    };
    // The census covers only the decodable classes: a container or a no-decoder
    // type has no decode outcome, and inventing one would be a false claim.
    // So it must account for `decoded + failed`, not for every row walked.
    assert_eq!(
        census_total as u64,
        read("decoded") + read("failed"),
        "the decode census must account for every record the tally decoded or refused: {census}"
    );
    let census_decoded: i64 = rows
        .iter()
        .filter(|row| {
            let spore_tools::json::Json::Obj(fields) = row else {
                return false;
            };
            fields
                .iter()
                .any(|(k, v)| k == "outcome" && *v == spore_tools::json::Json::from("decoded"))
        })
        .filter_map(|row| {
            let spore_tools::json::Json::Obj(fields) = row else {
                return None;
            };
            fields
                .iter()
                .find(|(k, _)| k == "count")
                .and_then(|(_, v)| match v {
                    spore_tools::json::Json::Int(n) => Some(*n),
                    _ => None,
                })
        })
        .sum();
    assert_eq!(
        census_decoded as u64,
        read("decoded"),
        "`stats` and `verify` must not disagree about how many records decoded"
    );
}

#[test]
fn at_least_one_package_is_required() {
    let (code, _out, err) = osptool(&["stats"]);
    assert_eq!(code, EXIT_USAGE, "{err}");
    assert!(err.contains("`stats` takes"), "{err}");
}
