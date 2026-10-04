//! `texture`: the PNG it writes, and the channel-order decision it reports.
//!
//! The unit tests in `src/png.rs` and `src/commands/texture.rs` cover the
//! encoder and the swizzle in isolation. These drive the whole command line,
//! because the thing that must not go wrong is the *combination*: a file that
//! opens, with the pixels the report claims, for a package that has no game
//! installed.

mod common;

use common::{osptool, package_of, Row};

use spore_tools::{EXIT_DECODE, EXIT_NOT_FOUND, EXIT_OK, EXIT_USAGE};

/// A synthetic `0x15` raster record: one texel of the corpus's flat normal.
fn flat_normal_record() -> Vec<u8> {
    // 2x2, 2 mips. Lanes are Z, X, Y, mask, so the flat normal (0, 0, 1) is
    // stored (255, 128, 128, mask) -- the literal constant the corpus records
    // spell out, and the one a swizzle bug cannot hide behind.
    const WIDTH: u32 = 2;
    const HEIGHT: u32 = 2;
    const MIPS: u32 = 2;
    let mut record = Vec::new();
    for value in [
        1u32,
        WIDTH,
        HEIGHT,
        MIPS,
        8,
        spore_texture::LUMINANCE_FOURCC,
        WIDTH * HEIGHT * 4,
    ] {
        record.extend_from_slice(&value.to_le_bytes());
    }
    // Base level: four texels, no length prefix.
    for texel in 0..4u8 {
        record.extend_from_slice(&[255, 128, 128, texel * 10]);
    }
    // Mip 1: a 1x1 level preceded by its own little-endian byte count.
    let mip_one: u32 = 4;
    record.extend_from_slice(&mip_one.to_le_bytes());
    record.extend_from_slice(&[255, 128, 128, 99]);
    record
}

/// A package holding the flat-normal `0x15` record.
fn normal_package() -> Vec<u8> {
    package_of(&[Row::new(
        spore_texture::RASTER_TYPE,
        0x4066_2900,
        0x067a_0901,
        flat_normal_record(),
    )])
}

const NORMAL_KEY: &str = "0x2f4e681c:0x40662900:0x067a0901";

#[test]
fn it_writes_a_png_a_viewer_can_open_and_the_pixels_are_the_swizzled_ones() {
    let file = common::temp_package("texture-normal", &normal_package());
    let path = file.path().display().to_string();
    let out_dir = common::TempDir::new("texture-out");
    let out = out_dir.path().join("flat.png");
    let out_arg = out.display().to_string();

    let (code, stdout, stderr) = osptool(&["texture", &path, NORMAL_KEY, "--out", &out_arg]);
    assert_eq!(code, EXIT_OK, "{stderr}");
    assert!(
        stdout.is_empty(),
        "stdout must carry only what was asked for, and `--json` was not: {stdout}"
    );

    let written = std::fs::read(&out).expect("the png must exist");
    let decoded = spore_tools::png::verify(&written).expect("the file must be a valid PNG");
    assert_eq!(decoded.width, 2);
    assert_eq!(decoded.height, 2);
    // The swizzle: stored (Z=255, X=128, Y=128) must come out as
    // (R=X=128, G=Y=128, B=Z=255) with the mask untouched.
    assert_eq!(
        &decoded.pixels[..8],
        &[128, 128, 255, 0, 128, 128, 255, 10],
        "the 0x15 family is stored Z, X, Y, mask and must be written as R=X, G=Y, B=Z, A=mask"
    );

    // And the report says which it did, in those words, on stderr.
    assert!(stderr.contains("swizzled"), "{stderr}");
    assert!(stderr.contains("Z, X, Y, mask"), "{stderr}");
    assert!(stderr.contains("0x2f4e681c"), "{stderr}");
    assert!(stderr.contains("wrote"), "{stderr}");
}

#[test]
fn stored_mode_writes_the_file_order_unchanged_and_says_so() {
    let file = common::temp_package("texture-stored", &normal_package());
    let path = file.path().display().to_string();
    let out_dir = common::TempDir::new("texture-stored-out");
    let out = out_dir.path().join("raw.png");
    let (code, _stdout, stderr) = osptool(&[
        "texture",
        &path,
        NORMAL_KEY,
        "--out",
        &out.display().to_string(),
        "--channels",
        "stored",
    ]);
    assert_eq!(code, EXIT_OK, "{stderr}");
    let decoded = spore_tools::png::verify(&std::fs::read(&out).unwrap()).expect("valid png");
    assert_eq!(
        &decoded.pixels[..4],
        &[255, 128, 128, 0],
        "`--channels stored` must not re-index anything"
    );
    assert!(stderr.contains("stored byte order, unchanged"), "{stderr}");
}

#[test]
fn the_swizzle_question_is_carried_as_a_non_finding_not_as_a_yes() {
    let file = common::temp_package("texture-json", &normal_package());
    let path = file.path().display().to_string();
    let out_dir = common::TempDir::new("texture-json-out");
    let out = out_dir.path().join("flat.png");
    let (code, stdout, stderr) = osptool(&[
        "texture",
        &path,
        NORMAL_KEY,
        "--out",
        &out.display().to_string(),
        "--json",
    ]);
    assert_eq!(code, EXIT_OK, "{stderr}");
    assert!(stdout.starts_with('{'), "one JSON object: {stdout}");
    let document = spore_tools::json::parse(&stdout).expect("valid JSON");
    let spore_tools::json::Json::Obj(pairs) = &document else {
        panic!("expected an object")
    };
    let get = |key: &str| {
        pairs
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value.clone())
            .unwrap_or_else(|| panic!("no key {key}"))
    };
    assert_eq!(
        get("channels"),
        spore_tools::json::Json::from("swizzled_from_zxy")
    );
    assert_eq!(get("type_id"), spore_tools::json::Json::from("0x2f4e681c"));
    assert_eq!(get("fourcc"), spore_tools::json::Json::from("0x00000015"));
    assert_eq!(get("width"), spore_tools::json::Json::from(2u32));
    assert_eq!(get("record"), spore_tools::json::Json::from(NORMAL_KEY));

    // The unestablished question: present, with no value.
    let spore_tools::json::Json::Obj(fields) = get("swizzle_should_be_applied") else {
        panic!("expected an object")
    };
    let lookup = |name: &str| {
        fields
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.clone())
    };
    assert_eq!(
        lookup("state"),
        Some(spore_tools::json::Json::from("unavailable"))
    );
    assert_eq!(
        lookup("reason"),
        Some(spore_tools::json::Json::from(
            "luminance_0x15_channel_swizzle"
        ))
    );
    assert!(
        lookup("value").is_none(),
        "an unavailable fact must carry no value at all: {stdout}"
    );
}

#[test]
fn a_mip_level_can_be_selected_and_an_absent_one_is_refused_by_name() {
    let file = common::temp_package("texture-mip", &normal_package());
    let path = file.path().display().to_string();
    let out_dir = common::TempDir::new("texture-mip-out");

    let top = out_dir.path().join("top.png");
    let (code, _stdout, stderr) = osptool(&[
        "texture",
        &path,
        NORMAL_KEY,
        "--out",
        &top.display().to_string(),
        "--mip",
        "1",
    ]);
    assert_eq!(code, EXIT_OK, "{stderr}");
    let decoded = spore_tools::png::verify(&std::fs::read(&top).unwrap()).unwrap();
    assert_eq!(
        (decoded.width, decoded.height),
        (1, 1),
        "mip 1 of a 2x2 base level is 1x1"
    );
    assert_eq!(decoded.pixels, vec![128, 128, 255, 99]);

    let absent = out_dir.path().join("absent.png");
    let (code, stdout, stderr) = osptool(&[
        "texture",
        &path,
        NORMAL_KEY,
        "--out",
        &absent.display().to_string(),
        "--mip",
        "7",
    ]);
    assert_eq!(code, EXIT_DECODE, "{stderr}");
    assert!(stderr.contains("has 2 mip level(s)"), "{stderr}");
    assert!(stdout.is_empty());
    assert!(
        !absent.exists(),
        "a refused request must not leave a file behind"
    );

    let layer = out_dir.path().join("layer.png");
    let (code, _stdout, stderr) = osptool(&[
        "texture",
        &path,
        NORMAL_KEY,
        "--out",
        &layer.display().to_string(),
        "--layer",
        "3",
    ]);
    assert_eq!(code, EXIT_DECODE);
    assert!(stderr.contains("has 1 layer(s)"), "{stderr}");
}

#[test]
fn a_type_with_no_decoder_refuses_and_never_writes_a_file() {
    // The honesty contract at the write path: `plt` has no raster decoder, so the
    // command says so and produces nothing that could be mistaken for a decode.
    let image = package_of(&[Row::new(
        spore_core::record::type_id::PLT,
        0x406b_6a00,
        1,
        vec![0x11; 40],
    )]);
    let file = common::temp_package("texture-plt", &image);
    let path = file.path().display().to_string();
    let out_dir = common::TempDir::new("texture-plt-out");
    let out = out_dir.path().join("nope.png");
    let (code, stdout, stderr) = osptool(&[
        "texture",
        &path,
        "0x011989b7:0x406b6a00:0x00000001",
        "--out",
        &out.display().to_string(),
    ]);
    assert_eq!(code, EXIT_DECODE, "{stderr}");
    assert!(stderr.contains("no raster decoder"), "{stderr}");
    assert!(stderr.contains("nothing was parsed"), "{stderr}");
    assert!(stdout.is_empty());
    assert!(!out.exists(), "no file may be written for a refused record");
}

#[test]
fn a_png_typed_record_points_at_extract_rather_than_at_a_decode() {
    let image = package_of(&[Row::new(
        spore_core::record::type_id::PNG,
        0x4066_2900,
        1,
        vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a],
    )]);
    let file = common::temp_package("texture-png", &image);
    let path = file.path().display().to_string();
    let out_dir = common::TempDir::new("texture-png-out");
    let out = out_dir.path().join("raw.png");
    let (code, _stdout, stderr) = osptool(&[
        "texture",
        &path,
        "0x2f7d0004:0x40662900:0x00000001",
        "--out",
        &out.display().to_string(),
    ]);
    assert_eq!(code, EXIT_DECODE, "{stderr}");
    assert!(stderr.contains("osptool extract"), "{stderr}");
    assert!(!out.exists());
}

#[test]
fn a_missing_record_is_exit_three_and_writes_nothing() {
    let file = common::temp_package("texture-miss", &normal_package());
    let path = file.path().display().to_string();
    let out_dir = common::TempDir::new("texture-miss-out");
    let out = out_dir.path().join("miss.png");
    let (code, _stdout, stderr) = osptool(&[
        "texture",
        &path,
        "0x2f4e681c:0x40662900:0x000000ff",
        "--out",
        &out.display().to_string(),
    ]);
    assert_eq!(code, EXIT_NOT_FOUND, "{stderr}");
    assert!(!out.exists());
}

#[test]
fn the_out_flag_is_required_and_never_names_a_path_inside_this_repository() {
    let file = common::temp_package("texture-flags", &normal_package());
    let path = file.path().display().to_string();

    let (code, _stdout, stderr) = osptool(&["texture", &path, NORMAL_KEY]);
    assert_eq!(code, EXIT_USAGE);
    assert!(stderr.contains("`--out` needs a value"), "{stderr}");

    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("the checkout has a parent")
        .join("definitely-not-here.png");
    let (code, _stdout, stderr) = osptool(&[
        "texture",
        &path,
        NORMAL_KEY,
        "--out",
        &root.display().to_string(),
    ]);
    assert_eq!(code, EXIT_USAGE, "{stderr}");
    assert!(stderr.contains("inside this repository"), "{stderr}");
    assert!(
        !root.exists(),
        "the refused path must not have been created"
    );

    // A relative path is resolved against the working directory, which is inside
    // the repository when the tests run, so `texture` must refuse it too.
    let (code, _stdout, stderr) = osptool(&["texture", &path, NORMAL_KEY, "--out", "x.png"]);
    assert_eq!(code, EXIT_USAGE, "{stderr}");
    assert!(stderr.contains("inside this repository"), "{stderr}");
    assert!(!std::path::Path::new("x.png").exists());
}

#[test]
fn the_channels_vocabulary_is_closed_and_a_bad_word_is_a_usage_error() {
    let file = common::temp_package("texture-channels", &normal_package());
    let path = file.path().display().to_string();
    let out_dir = common::TempDir::new("texture-channels-out");
    let (code, _stdout, stderr) = osptool(&[
        "texture",
        &path,
        NORMAL_KEY,
        "--out",
        &out_dir.path().join("x.png").display().to_string(),
        "--channels",
        "rgba",
    ]);
    assert_eq!(code, EXIT_USAGE, "{stderr}");
    assert!(stderr.contains("`auto` or `stored`"), "{stderr}");
}

#[test]
fn a_dxt5_record_is_written_verbatim_with_its_signature_called_out() {
    // Spore's DXT5 decode is black/red by construction. The image looks broken
    // and is not, so the report has to say so every time rather than leaving a
    // reader to conclude the tool is wrong.
    let image = common::synthetic_raster(8, 8, 1, 1);
    let file = common::temp_package(
        "texture-dxt5",
        &common::package_of(&[Row::new(spore_texture::RASTER_TYPE, 0x4066_2900, 1, image)]),
    );
    let path = file.path().display().to_string();
    let out_dir = common::TempDir::new("texture-dxt5-out");
    let out = out_dir.path().join("dxt5.png");
    let (code, _stdout, stderr) = osptool(&[
        "texture",
        &path,
        "0x2f4e681c:0x40662900:0x00000001",
        "--out",
        &out.display().to_string(),
    ]);
    assert_eq!(code, EXIT_OK, "{stderr}");
    assert!(
        stderr.contains("red in {0, 8}") && stderr.contains("green and blue 0"),
        "the DXT5 signature must be stated: {stderr}"
    );
    assert!(
        !stderr.contains("swizzled"),
        "DXT5 is never swizzled: {stderr}"
    );
    let decoded = spore_tools::png::verify(&std::fs::read(&out).unwrap()).unwrap();
    assert_eq!((decoded.width, decoded.height), (8, 8));
    assert_eq!(decoded.pixels.len(), 8 * 8 * 4);
}
