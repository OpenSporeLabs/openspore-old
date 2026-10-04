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
            common::synthetic_raster(8, 8, 1, 1),
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
    assert!(out.contains("0x35545844 DXT5 (decoded)"), "{out}");
}

#[test]
fn an_unsupported_fourcc_is_censused_and_labelled_refused_not_undecoded() {
    // A DXT1 record: present in the corpus, deliberately not decoded. The census
    // must count it and say what happened to it, not fold it into DXT5.
    let mut record = Vec::new();
    for value in [1u32, 4, 4, 1, 8, 0x3154_5844, 0, 0] {
        record.extend_from_slice(&value.to_le_bytes());
    }
    record.extend_from_slice(&[0u8; 16]);
    record.extend_from_slice(&[0u8; 32]);
    let image = package_of(&[Row::new(spore_texture::RASTER_TYPE, 0x4066_2900, 1, record)]);
    let file = common::temp_package("stats-dxt1", &image);
    let (code, out, err) = osptool(&["stats", &file.path().display().to_string()]);
    assert_eq!(code, EXIT_OK, "{err}");
    assert!(
        out.contains("0x31545844 DXT1 (present in the corpus, refused by name)"),
        "{out}"
    );
    assert!(
        out.contains("texture: unsupported fourcc"),
        "and the decode section says the same thing in the decoder's words: {out}"
    );
    assert!(
        out.contains("layers=unavailable"),
        "no layer-count rule applies to DXT1, and `unavailable` is not `0`: {out}"
    );
}

#[test]
fn a_raster_record_too_short_for_an_envelope_is_counted_and_never_folded_in() {
    let image = package_of(&[
        Row::new(spore_texture::RASTER_TYPE, 1, 1, vec![0u8; 16]),
        Row::new(
            spore_texture::RASTER_TYPE,
            1,
            2,
            common::synthetic_raster(8, 8, 1, 1),
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
