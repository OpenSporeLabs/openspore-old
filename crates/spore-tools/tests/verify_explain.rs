//! `verify --explain`: one record, diagnosed.
//!
//! The bulk tally is the slow part of corpus work; these tests pin that the
//! single-record explanation agrees with it, and that a type with no decoder
//! produces an explanation rather than a summary that looks decoded.

mod common;

use common::{osptool, package_of, Row};

use spore_tools::{EXIT_NOT_FOUND, EXIT_OK};

#[test]
fn explaining_a_decodable_record_reports_what_the_decoder_produced() {
    let image = package_of(&[Row::new(
        spore_gmdl::GMDL_TYPE,
        0x4061_6201,
        0x067a_0801,
        common::read_fixture("mini.gmdl"),
    )]);
    let file = common::temp_package("explain-gmdl", &image);
    let path = file.path().display().to_string();
    let (code, out, err) = osptool(&[
        "verify",
        &path,
        "--explain",
        "0x00e6bce5:0x40616201:0x067a0801",
    ]);
    assert_eq!(code, EXIT_OK, "{err}");
    assert!(out.contains("verdict      decoded"), "{out}");
    assert!(out.contains("class        Gmdl"), "{out}");
    assert!(out.contains("decoded by the gmdl walker"), "{out}");
    assert!(
        out.contains("mesh(es)"),
        "a decoded verdict names the decode: {out}"
    );
    assert!(out.contains("index compression       none"), "{out}");
}

#[test]
fn a_refused_record_is_explained_with_the_decoders_own_words() {
    // A raster record whose framing does not close: the fourcc is what the walk
    // dispatched on, so it is the answer and it has to be printed even though the
    // record did not decode.
    let mut record = Vec::new();
    for value in [1u32, 4, 4, 1, 8, 0x3154_5844, 0, 0] {
        record.extend_from_slice(&value.to_le_bytes());
    }
    record.extend_from_slice(&[0u8; 16]);
    record.extend_from_slice(&[0u8; 7]); // too short for the layer payload
    let image = package_of(&[Row::new(spore_texture::RASTER_TYPE, 0x4066_2900, 1, record)]);
    let file = common::temp_package("explain-raster", &image);
    let path = file.path().display().to_string();
    let (code, out, err) = osptool(&[
        "verify",
        &path,
        "--explain",
        "0x2f4e681c:0x40662900:0x00000001",
    ]);
    assert_eq!(code, EXIT_OK, "{err}");
    assert!(out.contains("verdict      failed"), "{out}");
    assert!(
        out.contains("0x31545844"),
        "the fourcc is the answer: {out}"
    );
    assert!(
        out.contains("DXT1") && out.contains("REFUSED BY NAME"),
        "{out}"
    );
    // And it must not claim the envelope's dimensions are a decoded image.
    assert!(
        out.contains("not a decoded image"),
        "the dimensions printed are header words, and the report says so: {out}"
    );
}

#[test]
fn a_type_with_no_decoder_produces_no_summary_looking_line() {
    let image = package_of(&[
        Row::new(
            spore_core::record::type_id::PLT,
            0x406b_6a00,
            1,
            vec![0x11; 40],
        ),
        Row::new(spore_core::record::type_id::PROP, 0x0, 2, vec![0x22; 8]),
    ]);
    let file = common::temp_package("explain-nodecode", &image);
    let path = file.path().display().to_string();

    let (code, out, err) = osptool(&[
        "verify",
        &path,
        "--explain",
        "0x011989b7:0x406b6a00:0x00000001",
    ]);
    assert_eq!(code, EXIT_OK, "{err}");
    assert!(out.contains("verdict      no-decoder"), "{out}");
    assert!(out.contains("nothing was parsed"), "{out}");
    assert!(
        !out.contains("  decoded"),
        "there is no decoder, so there must be no decoded line at all: {out}"
    );

    let (code, out, err) = osptool(&[
        "verify",
        &path,
        "--explain",
        "0x00b1b104:0x00000000:0x00000002",
    ]);
    assert_eq!(code, EXIT_OK, "{err}");
    assert!(out.contains("verdict      container"), "{out}");
    assert!(out.contains("does not parse this container"), "{out}");
    assert!(out.contains("memory_size"), "{out}");
}

#[test]
fn explaining_a_record_that_cannot_be_extracted_says_the_container_refused_it() {
    let mut broken = package_of(&[Row::new(
        spore_gmdl::GMDL_TYPE,
        0x4061_6201,
        1,
        common::read_fixture("mini.gmdl"),
    )]);
    let size_at = 96 + 4 + 16;
    broken[size_at..size_at + 4].copy_from_slice(&(0x0010_0000u32 | 0x8000_0000).to_le_bytes());
    let file = common::temp_package("explain-unreadable", &broken);
    let path = file.path().display().to_string();
    let (code, out, err) = osptool(&[
        "verify",
        &path,
        "--explain",
        "0x00e6bce5:0x40616201:0x00000001",
    ]);
    assert_eq!(code, EXIT_OK, "{err}");
    assert!(out.contains("verdict      unreadable"), "{out}");
    assert!(
        out.contains("no decoder ran"),
        "nothing is claimed about content that was never read: {out}"
    );
}

#[test]
fn explaining_an_absent_record_is_exit_three_with_the_stores_near_misses() {
    let image = package_of(&[Row::new(
        spore_gmdl::GMDL_TYPE,
        0x4061_6201,
        0x067a_0801,
        common::read_fixture("mini.gmdl"),
    )]);
    let file = common::temp_package("explain-absent", &image);
    let path = file.path().display().to_string();
    let (code, _out, err) = osptool(&[
        "verify",
        &path,
        "--explain",
        "0x00e6bce5:0x40616201:0x067a0802",
    ]);
    assert_eq!(code, EXIT_NOT_FOUND, "{err}");
    assert!(err.contains("near-miss candidates"), "{err}");
}

#[test]
fn explaining_replaces_the_walk_rather_than_running_it_too() {
    let image = package_of(&[
        Row::new(
            spore_core::record::type_id::PLT,
            0x406b_6a00,
            1,
            vec![0x11; 40],
        ),
        Row::new(
            spore_core::record::type_id::PLT,
            0x406b_6a00,
            2,
            vec![0x11; 40],
        ),
    ]);
    let file = common::temp_package("explain-exclusive", &image);
    let path = file.path().display().to_string();
    let (code, out, err) = osptool(&[
        "verify",
        &path,
        "--explain",
        "0x011989b7:0x406b6a00:0x00000001",
    ]);
    assert_eq!(code, EXIT_OK, "{err}");
    assert!(
        !out.contains("no decoder     "),
        "the tally must not run alongside the explanation: {out}"
    );
    assert!(!out.contains("walked"), "{out}");
}

#[test]
fn the_explanation_agrees_with_the_bulk_tally_on_the_same_record() {
    // The two views must not disagree: a `failed` count of 1 with a `decoded`
    // explanation would send a reader after the wrong bug.
    let image = package_of(&[
        Row::new(
            spore_texture::RASTER_TYPE,
            0x4066_2900,
            1,
            common::synthetic_raster(8, 8, 1, 1),
        ),
        Row::new(
            spore_core::record::type_id::PLT,
            0x406b_6a00,
            1,
            vec![0x11; 40],
        ),
    ]);
    let file = common::temp_package("explain-agrees", &image);
    let path = file.path().display().to_string();

    let (code, tally, err) = osptool(&["verify", &path, "--type", "0x2f4e681c"]);
    assert_eq!(code, EXIT_OK, "{err}");
    assert!(tally.contains("decoded        1"), "{tally}");
    assert!(tally.contains("failed         0"), "{tally}");

    let (code, explained, err) = osptool(&[
        "verify",
        &path,
        "--explain",
        "0x2f4e681c:0x40662900:0x00000001",
    ]);
    assert_eq!(code, EXIT_OK, "{err}");
    assert!(explained.contains("verdict      decoded"), "{explained}");
}

#[test]
fn a_malformed_explain_argument_is_a_usage_error_naming_the_flag() {
    let image = package_of(&[Row::new(1, 1, 1, vec![0; 4])]);
    let file = common::temp_package("explain-badarg", &image);
    let path = file.path().display().to_string();
    let (code, _out, err) = osptool(&["verify", &path, "--explain", "not-a-triple"]);
    assert_eq!(spore_tools::EXIT_USAGE, code, "{err}");
    assert!(err.contains("`--explain`"), "{err}");
    let (code, _out, err) = osptool(&["verify", &path, "--explain"]);
    assert_eq!(spore_tools::EXIT_USAGE, code, "{err}");
    assert!(err.contains("`--explain` needs a value"), "{err}");
}
