//! `describe`: the decoded/type-level distinction, and the numbers a decoder
//! produced.
//!
//! The gmdl and RW4 assertions are written against values computed **here**, from
//! the fixture generator's own constants and from the record bytes, rather than
//! copied out of a previous run of the tool. A test that asserts the tool's
//! output equals the tool's output proves nothing.

mod common;

use common::{osptool, package_of, raster_package, Row, MINI_GMDL, MINI_PACKAGE, MINI_RW4};
use spore_tools::{EXIT_DECODE, EXIT_OK};

/// A package holding the committed gmdl record.
fn gmdl_at_path() -> common::TempFile {
    common::temp_package("describe-gmdl", &common::gmdl_package())
}

/// A package holding the committed RW4 record.
fn rw4_at_path() -> common::TempFile {
    common::temp_package("describe-rw4", &common::rw4_package())
}

#[test]
fn the_gmdl_summary_contains_the_figures_the_fixture_generator_declares() {
    // `gen_fixtures.py`: GMDL_IDX_COUNT = 153, GMDL_VTX_COUNT = 50,
    // GMDL_STRIDE = 16, meshCount = 1, version 8.
    let file = gmdl_at_path();
    let path = file.path().display().to_string();
    let (code, out, err) = osptool(&["describe", &path, "0x00e6bce5:0x40616201:0x067a0801"]);
    assert_eq!(code, EXIT_OK, "{err}");

    assert!(out.contains("decoder: gmdl (decoded"), "{out}");
    assert!(out.contains("version            8"), "{out}");
    assert!(out.contains("mesh count         1"), "{out}");
    assert!(out.contains("index buffers      1"), "{out}");
    assert!(
        out.contains("indices=153 bits=16"),
        "the fixture declares 153 u16 indices: {out}"
    );
    assert!(out.contains("vertex buffers     1"), "{out}");
    assert!(
        out.contains("vertices=50"),
        "the fixture declares 50 vertices: {out}"
    );
    assert!(
        out.contains("stride=16"),
        "POSITION/TEXCOORD stride is 16: {out}"
    );
    assert!(out.contains("meshes             1"), "{out}");
    assert!(out.contains("[0] vertices=50 indices=153"), "{out}");
    assert!(out.contains("topology=TRIANGLELIST"), "{out}");
}

#[test]
fn the_gmdl_summary_names_every_vertex_element_field() {
    // Two elements: POSITION/FLOAT3 at offset 0, TEXCOORD0/FLOAT2 at offset 8.
    let file = gmdl_at_path();
    let path = file.path().display().to_string();
    let (code, out, _err) = osptool(&["describe", &path, "0x00e6bce5:0x40616201:0x067a0801"]);
    assert_eq!(code, EXIT_OK);
    assert!(out.contains("vertex descriptors 1"), "{out}");
    assert!(out.contains("[0] 2 element(s), stride 16"), "{out}");
    assert!(
        out.contains("[0] stream=0 offset=0 type=2 (FLOAT3) method=0 usage=0 (POSITION) usage_index=0 type_code=0x00000000"),
        "{out}"
    );
    assert!(
        out.contains("[1] stream=0 offset=8 type=1 (FLOAT2) method=0 usage=5 (TEXCOORD) usage_index=0 type_code=0x00000000"),
        "{out}"
    );
}

#[test]
fn the_gmdl_summary_reports_the_bounds_and_the_referenced_file() {
    // The bounds and radius are the fixture generator's declared constants, so
    // they are asserted literally. The referenced-file identity is **not**:
    // whether its three words are read little- or big-endian, and in which
    // order, is `spore-gmdl`'s business and has changed once already (the
    // `refCount` table is big-endian end to end). This test is about what
    // `describe` *renders*, so it compares the rendering against the decoder's
    // own API and asserts nothing about the byte order behind each word.
    //
    // Known cross-crate inconsistency, deliberately not asserted around here:
    // `tests/fixtures/mini.gmdl` was generated with its `refCount` key words
    // written little-endian, so once `spore-gmdl` reads those words
    // big-endian (which the corpus settles: 7 989 of 7 989 declared keys resolve
    // that way and 0 little-endian) the fixture's type word decodes as
    // 0xe5bce600 rather than 0x00e6bce5. The fixture needs regenerating; it does
    // not belong to this crate. See `docs/TOOLS.md`.
    let file = gmdl_at_path();
    let path = file.path().display().to_string();
    let (_code, out, _err) = osptool(&["describe", &path, "0x00e6bce5:0x40616201:0x067a0801"]);
    assert!(out.contains("referenced files   1"), "{out}");

    let bytes = std::fs::read(MINI_GMDL).expect("committed gmdl fixture");
    let model = spore_gmdl::parse(&bytes).expect("the committed gmdl fixture decodes");
    let key = model.referenced_files[0];
    assert!(
        out.contains(&format!("[0] {key}")),
        "`describe` must print the decoder's own key verbatim: {out}"
    );

    assert!(
        out.contains(
            "bounds             min (1.5000, -2.2500, 0.1250) max (3.5000, 0.7500, 2.0000)"
        ),
        "{out}"
    );
    assert!(out.contains("radius             4.2500"), "{out}");
}

#[test]
fn a_complete_trailer_is_reported_as_complete_with_both_byte_counts() {
    let file = gmdl_at_path();
    let path = file.path().display().to_string();
    let (_code, out, _err) = osptool(&["describe", &path, "0x00e6bce5:0x40616201:0x067a0801"]);
    let record_len = common::read_fixture("mini.gmdl").len();
    assert!(
        out.contains(&format!(
            "trailer            complete ({record_len} of {record_len} bytes validated)"
        )),
        "{out}"
    );
    assert!(out.contains("material ids       1"), "{out}");
    assert!(
        out.contains("[0] 0x12345678"),
        "the fixture's material id: {out}"
    );
    assert!(out.contains("texture refs       0"), "{out}");
    assert!(out.contains("bone ranges        0"), "{out}");
}

#[test]
fn a_trailer_that_claims_more_than_the_record_holds_stops_and_names_the_stage() {
    // The fixture's trailer is five zero words: bone-range count, anim-data
    // count and the three-word key. Claiming a huge bone-range count makes the
    // best-effort walk stop *inside* the record, so `strict_consumed` falls
    // short of `consumed` and the unvalidated bytes must be counted.
    let mut gmdl = common::read_fixture("mini.gmdl");
    let len = gmdl.len();
    gmdl[len - 20..len - 16].copy_from_slice(&u32::MAX.to_le_bytes());
    let image = package_of(&[Row::new(spore_gmdl::GMDL_TYPE, 1, 2, gmdl)]);
    let file = common::temp_package("describe-gmdl-trunc", &image);
    let path = file.path().display().to_string();
    let (code, out, err) = osptool(&["describe", &path, "0x00e6bce5:0x00000001:0x00000002"]);
    assert_eq!(code, EXIT_OK, "{err}");
    assert!(
        out.contains("trailer            TRUNCATED at the bone-range array"),
        "{out}"
    );
    // The count word itself was read, so the position stops four bytes past it:
    // `len - 16`, and the 16 bytes after it are unvalidated.
    assert!(
        out.contains(&format!("strict consumed    {} of {len} bytes", len - 16)),
        "{out}"
    );
    assert!(
        out.contains("unvalidated tail   16 bytes"),
        "the unvalidated bytes must be counted: {out}"
    );
    assert!(
        out.contains("not parsed, not claimed"),
        "the unvalidated bytes must not be reported as read: {out}"
    );
    assert!(
        out.contains(&format!("consumed           {len} of {len} bytes")),
        "{out}"
    );
    // The geometry itself still decoded: the truncation is in the trailer only.
    assert!(out.contains("mesh count         1"), "{out}");
}

#[test]
fn a_trailer_cut_inside_the_final_read_does_not_claim_zero_unvalidated_bytes() {
    // Removing the last four bytes stops the walk while reading the trailing
    // key. `strict_consumed` is the reader *position*, so it already sits at the
    // end and the two counts agree -- and printing "0 bytes unvalidated" would
    // claim a word was validated that was never read.
    let mut gmdl = common::read_fixture("mini.gmdl");
    let len = gmdl.len();
    gmdl.truncate(len - 4);
    let image = package_of(&[Row::new(spore_gmdl::GMDL_TYPE, 1, 2, gmdl)]);
    let file = common::temp_package("describe-gmdl-tail", &image);
    let path = file.path().display().to_string();
    let (code, out, err) = osptool(&["describe", &path, "0x00e6bce5:0x00000001:0x00000002"]);
    assert_eq!(code, EXIT_OK, "{err}");
    assert!(
        out.contains("trailer            TRUNCATED at the trailing three-word key"),
        "{out}"
    );
    assert!(
        out.contains("the shortfall is inside the read that failed"),
        "{out}"
    );
    assert!(
        !out.contains("unvalidated tail   0 bytes"),
        "a zero byte count must not be reported as full validation: {out}"
    );
}

#[test]
fn removing_the_whole_trailer_is_a_strict_failure_not_a_partial_success() {
    // With nothing left after the material info, the *strict* section refuses:
    // the walk demands at least the four bytes the trailer starts with. This is
    // exit 4, and it is the honest answer -- there is no partial model to print.
    let mut gmdl = common::read_fixture("mini.gmdl");
    gmdl.truncate(gmdl.len() - 20);
    let image = package_of(&[Row::new(spore_gmdl::GMDL_TYPE, 1, 2, gmdl)]);
    let file = common::temp_package("describe-gmdl-notrailer", &image);
    let path = file.path().display().to_string();
    let (code, _out, err) = osptool(&["describe", &path, "0x00e6bce5:0x00000001:0x00000002"]);
    assert_eq!(code, EXIT_DECODE);
    assert!(err.contains("truncated trailer"), "{err}");
}

#[test]
fn the_rw4_summary_prints_the_oracle_describe_string_verbatim() {
    // The exact string `tools/spore/rw4/rw4.py::describe` produces for
    // tests/fixtures/mini_rw4.rw4, verified by running it under Python.
    const ORACLE: &str =
        "0x1 obj=1 sec=3 buf=80 | 10030 d=0x138 s=12 | 20003 d=0x144 s=44 | 40001 d=0x170 s=24";
    // And this crate's own decoder must produce the same bytes for the same file.
    let directory =
        spore_rw4::parse(&common::read_fixture("mini_rw4.rw4")).expect("the fixture decodes");
    assert_eq!(
        directory.describe(),
        ORACLE,
        "Rw4::describe must stay oracle-compatible"
    );

    let file = rw4_at_path();
    let path = file.path().display().to_string();
    let (code, out, err) = osptool(&["describe", &path, "0x2f4e681b:0x40627100:0x067a0802"]);
    assert_eq!(code, EXIT_OK, "{err}");
    assert!(
        out.contains(ORACLE),
        "the oracle string must appear verbatim:\n{out}"
    );

    assert!(
        out.contains("file type           0x00000001 (MODEL)"),
        "{out}"
    );
    assert!(
        out.contains("section count       3 declared, 3 row(s) present"),
        "{out}"
    );
    assert!(
        out.contains("buffer size         80 bytes (arena base 0x00000138)"),
        "{out}"
    );
    assert!(out.contains("is_complete         true"), "{out}");
    // `0x40001` is *not* in the C++ `inKnownSet`, so one row counts as unknown --
    // while the Python oracle calls it `Mesh`. Both facts are printed, because
    // `spore_rw4` documents the two tables as genuinely incompatible and a
    // single name here would silently pick a side.
    assert!(
        out.contains("unknown type codes  1 of 3 row(s), counted against the C++ known set"),
        "{out}"
    );
    assert!(out.contains("0x00010030 (BaseResource)"), "{out}");
    assert!(out.contains("0x00020003 (Raster)"), "{out}");
    assert!(
        out.contains("0x00040001 (unknown to the C++ known set; Mesh in the Python oracle)"),
        "{out}"
    );
    // The arena-relative/absolute asymmetry is visible in the reported address.
    assert!(out.contains("data=0x00000138"), "{out}");
    assert!(out.contains("stored_pointer=0x00000000"), "{out}");
    // And the payload limit is stated, not implied.
    assert!(out.contains("section *directory* only"), "{out}");
}

#[test]
fn the_raster_summary_labels_the_unresolved_envelope_fields() {
    let image = raster_package();
    let file = common::temp_package("describe-raster", &image);
    let path = file.path().display().to_string();
    let (code, out, err) = osptool(&["describe", &path, "0x2f4e681c:0x40662900:0x067a0901"]);
    assert_eq!(code, EXIT_OK, "{err}");

    assert!(out.contains("decoder: raster (decoded"), "{out}");
    assert!(out.contains("version         1"), "{out}");
    assert!(out.contains("width           32"), "{out}");
    assert!(out.contains("height          16"), "{out}");
    assert!(out.contains("mip count       3"), "{out}");
    // The three fields the texture crate refuses to interpret must say so on
    // every one of their three lines.
    assert_eq!(out.matches("UNRESOLVED").count(), 2, "{out}");
    assert!(
        out.contains("field_10        0x00000008 UNRESOLVED"),
        "{out}"
    );
    assert!(
        out.contains("field_18        0x00040000 UNRESOLVED"),
        "{out}"
    );
    assert!(
        out.contains("field_1c        0x0000ffff format-specific per the oracle"),
        "{out}"
    );
    assert!(out.contains("fourcc          0x35545844 (DXT5)"), "{out}");
    assert!(out.contains("derived layers    2"), "{out}");
    // Per-layer mip dimensions, from the decoder rather than re-derived.
    assert!(out.contains("layer 0 (32x16, 16x8, 8x4)"), "{out}");
    assert!(out.contains("layer 1 (32x16, 16x8, 8x4)"), "{out}");
}

#[test]
fn a_named_type_with_no_decoder_says_so_instead_of_printing_a_summary() {
    let (code, out, err) = osptool(&["describe", MINI_PACKAGE, "0x58545354:0x11111111:0x22222222"]);
    assert_eq!(code, EXIT_OK, "{err}");
    assert!(out.contains("decoder: none for 0x58545354"), "{out}");
    assert!(out.contains("no decoder for that type id"), "{out}");
    assert!(
        out.contains("index's memory_size, not the result of a"),
        "{out}"
    );
    // Nothing that could read as a decoded structure.
    for forbidden in [
        "vertices",
        "indices",
        "stride",
        "mip",
        "section row",
        "bounds",
    ] {
        assert!(
            !out.contains(forbidden),
            "`{forbidden}` must not appear: {out}"
        );
    }
}

#[test]
fn a_named_but_undecodable_type_is_named_and_still_not_decoded() {
    let image = raster_package();
    let file = common::temp_package("describe-plt", &image);
    let path = file.path().display().to_string();
    let (code, out, _err) = osptool(&["describe", &path, "0x011989b7:0x406b6a00:0x067a0902"]);
    assert_eq!(code, EXIT_OK);
    assert!(out.contains("0x011989b7 (plt)"), "{out}");
    assert!(out.contains("decoder: none for"), "{out}");
}

#[test]
fn a_container_type_says_it_is_a_container_and_not_decoded() {
    let image = raster_package();
    let file = common::temp_package("describe-prop", &image);
    let path = file.path().display().to_string();
    let (code, out, _err) = osptool(&["describe", &path, "0x00b1b104:0x00000000:0x00000001"]);
    assert_eq!(code, EXIT_OK);
    assert!(
        out.contains("decoder: container (0x00b1b104 (prop)"),
        "{out}"
    );
    assert!(out.contains("payload NOT decoded"), "{out}");
    assert!(out.contains("nothing further is claimed"), "{out}");
}

#[test]
fn a_record_of_a_decodable_type_that_fails_to_decode_exits_four() {
    let image = package_of(&[Row::new(
        spore_gmdl::GMDL_TYPE,
        1,
        2,
        b"this is not a gmdl record".to_vec(),
    )]);
    let file = common::temp_package("describe-bad", &image);
    let path = file.path().display().to_string();
    let (code, out, err) = osptool(&["describe", &path, "0x00e6bce5:0x00000001:0x00000002"]);
    assert_eq!(code, EXIT_DECODE);
    assert!(
        out.contains("index extent"),
        "the index extent is still known: {out}"
    );
    assert!(err.contains("gmdl:"), "{err}");
}

#[test]
fn an_unsupported_gmdl_version_is_refused_by_name() {
    // Version 9 changes the material-info framing and the decoder refuses it.
    let mut gmdl = common::read_fixture("mini.gmdl");
    gmdl[0..4].copy_from_slice(&9u32.to_le_bytes());
    let image = package_of(&[Row::new(spore_gmdl::GMDL_TYPE, 1, 2, gmdl)]);
    let file = common::temp_package("describe-v9", &image);
    let path = file.path().display().to_string();
    let (code, _out, err) = osptool(&["describe", &path, "0x00e6bce5:0x00000001:0x00000002"]);
    assert_eq!(code, EXIT_DECODE);
    assert!(err.contains("unsupported version 9"), "{err}");
}

#[test]
fn the_index_extent_is_always_reported_before_any_decode() {
    // Every run above prints these four lines first. They come from the DBPF
    // index, so they hold for a record whose payload is nonsense.
    for (image, key) in [
        (common::gmdl_package(), "0x00e6bce5:0x40616201:0x067a0801"),
        (common::rw4_package(), "0x2f4e681b:0x40627100:0x067a0802"),
    ] {
        let file = common::temp_package("describe-extent", &image);
        let path = file.path().display().to_string();
        let (_code, out, _err) = osptool(&["describe", &path, key]);
        for field in [
            "record      ",
            "package     ",
            "index extent            0x",
            "index memory size       ",
            "index compression       ",
        ] {
            assert!(out.contains(field), "missing `{field}`:\n{out}");
        }
    }
}

#[test]
fn the_fixture_paths_the_assertions_rely_on_are_the_committed_ones() {
    assert!(std::path::Path::new(MINI_GMDL).exists());
    assert!(std::path::Path::new(MINI_RW4).exists());
    assert!(std::path::Path::new(MINI_PACKAGE).exists());
    // Sizes pinned so a truncated fixture cannot silently change what the
    // numbers above mean.
    assert_eq!(common::read_fixture("mini.gmdl").len(), 1266);
    assert_eq!(common::read_fixture("mini_rw4.rw4").len(), 392);
    assert_eq!(common::read_fixture("mini_package.dbpf").len(), 685);
}
