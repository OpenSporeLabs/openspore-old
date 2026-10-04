//! Robustness: no input makes the tool panic.
//!
//! A tool that reads a 995 MB package of records it did not author will be handed
//! truncated, misaligned and outright corrupt data. Every path here is required
//! to end in a typed error, a counted failure, or a report that says a type has no
//! decoder — never in a panic, and never in output that reads as a successful
//! decode.

mod common;

use common::{osptool, package_of, raster_package, synthetic_raster, Row, TempDir, TempFile};

/// Asserts a run finished normally: one of the documented exit codes, and no
/// panic (a panic here would fail the test, which is the point).
fn assert_clean(code: i32, out: &str, err: &str, what: &str) {
    assert!(
        (0..=5).contains(&code),
        "{what}: exit code {code} is outside the documented set\nstdout:\n{out}\nstderr:\n{err}"
    );
    assert!(!out.contains("panicked"), "{what}: {out}");
}

/// Runs `describe` over a mutated payload and returns the exit code.
fn describe_mutated(tag: &str, payload: Vec<u8>, at: usize, flip: u8) -> (i32, String, String) {
    let mut bytes = payload;
    bytes[at] ^= flip;
    let image = package_of(&[Row::new(spore_gmdl::GMDL_TYPE, 1, 1, bytes)]);
    let file = TempFile::with(tag, &image);
    let path = file.path().display().to_string();
    osptool(&["describe", &path, "0x00e6bce5:0x00000001:0x00000001"])
}

/// Runs `verify` over a mutated payload.
fn verify_mutated(tag: &str, payload: Vec<u8>, at: usize, flip: u8) -> (i32, String, String) {
    let mut bytes = payload;
    bytes[at] ^= flip;
    let image = package_of(&[Row::new(spore_gmdl::GMDL_TYPE, 1, 1, bytes)]);
    let file = TempFile::with(tag, &image);
    let path = file.path().display().to_string();
    osptool(&["verify", &path])
}

#[test]
fn flipping_a_byte_at_many_offsets_in_the_gmdl_never_panics() {
    let gmdl = common::read_fixture("mini.gmdl");
    // A stride that reaches the header, the bounds, the index buffer, the
    // descriptors, the vertex buffer, the material info and the trailer.
    for at in (0..gmdl.len()).step_by(7) {
        let (code, out, err) = describe_mutated("robust-gmdl", gmdl.clone(), at, 0xFF);
        assert_clean(code, &out, &err, &format!("describe, flip at {at}"));
        let (code, out, err) = verify_mutated("robust-gmdl-v", gmdl.clone(), at, 0xFF);
        assert_clean(code, &out, &err, &format!("verify, flip at {at}"));
    }
    // And a low bit, which is far likelier to leave a record *looking* valid.
    for at in (0..gmdl.len()).step_by(11) {
        let (code, out, err) = describe_mutated("robust-gmdl-lo", gmdl.clone(), at, 0x01);
        assert_clean(code, &out, &err, &format!("describe, low flip at {at}"));
    }
}

#[test]
fn truncating_the_gmdl_at_every_length_never_panics() {
    // A prefix sweep is the cheapest total check there is: every possible short
    // record reaches the decoder exactly once.
    let gmdl = common::read_fixture("mini.gmdl");
    for length in 0..=gmdl.len() {
        let image = package_of(&[Row::new(
            spore_gmdl::GMDL_TYPE,
            1,
            1,
            gmdl[..length].to_vec(),
        )]);
        let file = TempFile::with("robust-trunc", &image);
        let path = file.path().display().to_string();
        let (code, out, err) = osptool(&["describe", &path, "0x00e6bce5:0x00000001:0x00000001"]);
        assert_clean(
            code,
            &out,
            &err,
            &format!("describe, gmdl truncated to {length}"),
        );
        let (code, out, err) = osptool(&["verify", &path]);
        assert_clean(
            code,
            &out,
            &err,
            &format!("verify, gmdl truncated to {length}"),
        );
        // The index extent is always readable, whatever the payload is.
        assert!(out.is_empty() || out.contains("index rows"), "{out}");
    }
}

#[test]
fn flipping_and_truncating_the_rw4_never_panics() {
    let rw4 = common::read_fixture("mini_rw4.rw4");
    for at in (0..rw4.len()).step_by(5) {
        let mut bytes = rw4.clone();
        bytes[at] ^= 0xFF;
        let image = package_of(&[Row::new(spore_rw4::RW4_TYPE, 1, 1, bytes)]);
        let file = TempFile::with("robust-rw4", &image);
        let path = file.path().display().to_string();
        let (code, out, err) = osptool(&["describe", &path, "0x2f4e681b:0x00000001:0x00000001"]);
        assert_clean(code, &out, &err, &format!("describe, rw4 flip at {at}"));
    }
    for length in 0..=rw4.len() {
        let image = package_of(&[Row::new(spore_rw4::RW4_TYPE, 1, 1, rw4[..length].to_vec())]);
        let file = TempFile::with("robust-rw4-trunc", &image);
        let path = file.path().display().to_string();
        let (code, out, err) = osptool(&["describe", &path, "0x2f4e681b:0x00000001:0x00000001"]);
        assert_clean(
            code,
            &out,
            &err,
            &format!("describe, rw4 truncated to {length}"),
        );
    }
}

#[test]
fn a_synthetic_raster_survives_flipping_and_truncation() {
    let raster = synthetic_raster(16, 16, 3);
    for at in (0..raster.len()).step_by(3) {
        let mut bytes = raster.clone();
        bytes[at] ^= 0x7F;
        let image = package_of(&[Row::new(spore_texture::RASTER_TYPE, 1, 1, bytes)]);
        let file = TempFile::with("robust-raster", &image);
        let path = file.path().display().to_string();
        let (code, out, err) = osptool(&["describe", &path, "0x2f4e681c:0x00000001:0x00000001"]);
        assert_clean(code, &out, &err, &format!("describe, raster flip at {at}"));
    }
    for length in (0..=raster.len()).step_by(2) {
        let image = package_of(&[Row::new(
            spore_texture::RASTER_TYPE,
            1,
            1,
            raster[..length].to_vec(),
        )]);
        let file = TempFile::with("robust-raster-trunc", &image);
        let path = file.path().display().to_string();
        let (code, out, err) = osptool(&["describe", &path, "0x2f4e681c:0x00000001:0x00000001"]);
        assert_clean(
            code,
            &out,
            &err,
            &format!("describe, raster truncated to {length}"),
        );
    }
}

#[test]
fn a_corrupt_package_image_never_panics_in_any_command() {
    // The index is the first thing parsed and the easiest thing to corrupt: the
    // row count, the index offset, the magic, the shared-id flags.
    let base = common::read_fixture("mini_package.dbpf");
    for at in (0..base.len()).step_by(13) {
        let mut bytes = base.clone();
        bytes[at] ^= 0xFF;
        let file = TempFile::with("robust-index", &bytes);
        let path = file.path().display().to_string();
        for command in [
            vec!["list", &path],
            vec!["verify", &path],
            vec!["types", "--package", &path],
            vec!["manifest", &path, "--stats"],
            vec!["find", &path, "0x58545354:0x11111111:0x22222222"],
            vec!["describe", &path, "0x58545354:0x11111111:0x22222222"],
        ] {
            let (code, out, err) = osptool(&command);
            assert_clean(
                code,
                &out,
                &err,
                &format!("{command:?}, index flip at {at}"),
            );
        }
    }
    // And every prefix of the package image.
    for length in (0..=base.len()).step_by(29) {
        let file = TempFile::with("robust-index-trunc", &base[..length]);
        let path = file.path().display().to_string();
        for command in [
            vec!["list", &path],
            vec!["verify", &path],
            vec!["manifest", &path],
        ] {
            let (code, out, err) = osptool(&command);
            assert_clean(
                code,
                &out,
                &err,
                &format!("{command:?}, image truncated to {length}"),
            );
        }
    }
}

#[test]
fn a_record_claiming_an_absurd_size_is_refused_rather_than_allocating() {
    // `stored_size` is masked to 31 bits, so the largest claimable extent is 2 GB.
    // The extent is bounds-checked against the image before any copy.
    let image = common::read_fixture("mini_package.dbpf");
    // Row 0's stored-size word: 100 + 16.
    let mut bytes = image.clone();
    bytes[116..120].copy_from_slice(&0x7FFF_FFFFu32.to_le_bytes());
    let file = TempFile::with("robust-hugetextent-2", &bytes);
    let path = file.path().display().to_string();
    let (code, _out, err) = osptool(&[
        "extract",
        &path,
        "0x58545354:0x11111111:0x22222222",
        "--out",
        "/tmp",
    ]);
    assert!(
        code == spore_tools::EXIT_DECODE || code == spore_tools::EXIT_IO,
        "{err}"
    );
    assert!(err.contains("extent") || err.contains("No such"), "{err}");
}

#[test]
fn an_extreme_raster_header_is_refused_rather_than_allocated() {
    // A header claiming 0xFFFFFFFF texels and a huge mip count must be an error,
    // not an enormous allocation.
    let mut raster = synthetic_raster(4, 4, 1);
    raster[4..8].copy_from_slice(&u32::MAX.to_le_bytes()); // width
    raster[8..12].copy_from_slice(&u32::MAX.to_le_bytes()); // height
    raster[12..16].copy_from_slice(&0xFFFF_FFFFu32.to_le_bytes()); // mip_count
    let image = package_of(&[Row::new(spore_texture::RASTER_TYPE, 1, 1, raster)]);
    let file = TempFile::with("robust-raster-huge", &image);
    let path = file.path().display().to_string();
    let (code, _out, err) = osptool(&["describe", &path, "0x2f4e681c:0x00000001:0x00000001"]);
    assert_eq!(code, spore_tools::EXIT_DECODE, "{err}");
    assert!(err.contains("texture:"), "{err}");
}

#[test]
fn a_gmdl_claiming_a_million_meshes_is_refused_rather_than_allocated() {
    let mut gmdl = common::read_fixture("mini.gmdl");
    // meshCount sits after the 12-byte referenced-file key: 8 + 12.
    gmdl[20..24].copy_from_slice(&0x0010_0000u32.to_le_bytes());
    let (code, out, err) = describe_mutated("robust-manymeshes", gmdl, 0, 0x00);
    assert_clean(code, &out, &err, "gmdl claiming 1 048 576 meshes");
    assert_eq!(code, spore_tools::EXIT_DECODE, "{err}");
    assert!(err.contains("gmdl:"), "{err}");
    // The index extent is still reported, because it is a fact about the index.
    assert!(out.contains("index extent"), "{out}");
}

#[test]
fn an_empty_and_a_one_byte_package_are_handled_not_panicked_on() {
    for contents in [vec![], vec![0u8], vec![b'D'], b"DBPF".to_vec()] {
        let file = TempFile::with("robust-tiny", &contents);
        let path = file.path().display().to_string();
        for command in [
            vec!["list", &path],
            vec!["verify", &path],
            vec!["manifest", &path],
            vec!["types", "--package", &path],
        ] {
            let (code, out, err) = osptool(&command);
            assert_clean(
                code,
                &out,
                &err,
                &format!("{command:?} on {} bytes", contents.len()),
            );
        }
    }
}

#[test]
fn a_zero_length_record_of_a_decodable_type_is_refused() {
    // `memory_size == 0` with `stored_size == 0` is a legal row shape; the
    // decoder must refuse the payload rather than report an empty model.
    let image = package_of(&[Row::new(spore_gmdl::GMDL_TYPE, 1, 1, Vec::new())]);
    let file = TempFile::with("robust-emptypayload", &image);
    let path = file.path().display().to_string();
    let (code, _out, err) = osptool(&["describe", &path, "0x00e6bce5:0x00000001:0x00000001"]);
    assert_eq!(code, spore_tools::EXIT_DECODE, "{err}");
    assert!(err.contains("empty input"), "{err}");
}

#[test]
fn a_mutated_raster_package_still_verifies_without_panicking() {
    let image = raster_package();
    for at in (0..image.len()).step_by(17) {
        let mut bytes = image.clone();
        bytes[at] ^= 0x5A;
        let file = TempFile::with("robust-raster-pkg", &bytes);
        let path = file.path().display().to_string();
        let (code, out, err) = osptool(&["verify", &path]);
        assert_clean(
            code,
            &out,
            &err,
            &format!("verify, raster package flip at {at}"),
        );
        // Whatever happened, the counts still add up: a corrupted index may
        // refuse the package outright (exit 4, no report), but if a report came
        // out it must account for every row.
        if code == spore_tools::EXIT_OK {
            assert!(out.contains("walked         3"), "{out}");
            let walked = out
                .lines()
                .find_map(|line| line.strip_prefix("walked         "))
                .and_then(|value| value.trim().parse::<u64>().ok())
                .expect("a walked count");
            let bucket = |name: &str| -> u64 {
                out.lines()
                    .find_map(|line| line.strip_prefix(&format!("{name:<13}")))
                    .and_then(|value| value.trim().parse::<u64>().ok())
                    .unwrap_or(0)
            };
            assert_eq!(
                bucket("decoded") + bucket("failed") + bucket("container") + bucket("no decoder"),
                walked,
                "the buckets must partition the walk at offset {at}:\n{out}"
            );
        }
    }
}

#[test]
fn extract_into_a_full_or_unwritable_directory_is_an_io_error_not_a_panic() {
    // A directory path where a file should be.
    let dir = TempDir::new("robust-outdir");
    let package = TempFile::with("robust-outdir-pkg", &common::gmdl_package());
    let path = package.path().display().to_string();
    // Point `--out` at the package file itself: the join then names a path under
    // a regular file, and the write must fail as an I/O error.
    let (code, _out, err) = osptool(&[
        "extract",
        &path,
        "0x00e6bce5:0x40616201:0x067a0801",
        "--out",
        &path,
    ]);
    assert_eq!(code, spore_tools::EXIT_IO, "{err}");
    // The package is untouched by the failed write.
    assert_eq!(std::fs::read(&path).unwrap(), common::gmdl_package());
    // And a normal directory still works, so the failure above was not universal.
    let out = dir.path().display().to_string();
    let (code, _out, err) = osptool(&[
        "extract",
        &path,
        "0x00e6bce5:0x40616201:0x067a0801",
        "--out",
        &out,
    ]);
    assert_eq!(code, spore_tools::EXIT_OK, "{err}");
}
