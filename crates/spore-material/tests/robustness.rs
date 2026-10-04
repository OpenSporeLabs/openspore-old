//! No panics, anywhere.
//!
//! Every entry point is fed truncated records, byte-mutated records and absurd
//! headers, and must answer `Ok` or `Err`. A panic in a decoder reached from an
//! asset pipeline is a crash in the game, and this crate sits on the path from a
//! 995 MB content package to the render loop.
//!
//! The sweeps are exhaustive over the prefix lengths and over single-byte
//! mutations rather than sampled, because "the test happened to pick a bad byte"
//! is not evidence.

mod support;

use spore_assets::ContentStore;
use spore_core::ResourceKey;
use spore_gmdl::GmdlTextureRef;
use spore_material::{
    inspect_envelope, model_materials, resolve_model_record, resolve_model_textures,
    resolve_texture, MaterialError,
};
use spore_texture::{DXT5_FOURCC, RASTER_TYPE};
use support::*;

/// The reference every sweep in this file resolves.
fn target() -> GmdlTextureRef {
    GmdlTextureRef {
        instance_id: DOCUMENTED_INSTANCE_ID,
        group_id: 0x4063_2900,
    }
}

/// A store whose single record holds `bytes` at the documented key.
fn store_with(bytes: Vec<u8>) -> ContentStore {
    store_over(&[(raster_key(0x4063_2900, DOCUMENTED_INSTANCE_ID), bytes)])
}

/// Runs every raster entry point over `bytes` and asserts each answers.
fn exercise_all_entry_points(bytes: Vec<u8>) {
    let store = store_with(bytes.clone());
    // Each of these may succeed or fail; none may panic.
    let _ = resolve_texture(&store, target(), RASTER_TYPE);
    let _ = inspect_envelope(&store, target(), RASTER_TYPE);
    // Assumed types that are not rasters, including one that is not a record
    // type at all.
    for assumed in [
        RASTER_TYPE,
        RW4_TYPE,
        PNG_TYPE,
        spore_gmdl::GMDL_TYPE,
        0xDEAD_BEEF,
        0,
        u32::MAX,
    ] {
        let _ = resolve_texture(&store, target(), assumed);
        let _ = inspect_envelope(&store, target(), assumed);
    }
    // A wildcard reference on both sides, which must be refused before any I/O.
    let _ = resolve_texture(
        &store,
        GmdlTextureRef {
            instance_id: spore_core::WILDCARD,
            group_id: spore_core::WILDCARD,
        },
        RASTER_TYPE,
    );

    // The model entry points, over a model that references this record.
    let model_store = store_over(&[
        (documented_model_key(), documented_gmdl()),
        (raster_key(0x4063_2900, DOCUMENTED_INSTANCE_ID), bytes),
    ]);
    if let Ok(model) = spore_gmdl::parse(&documented_gmdl()) {
        let _ = resolve_model_textures(&model_store, &model, RASTER_TYPE);
        let _ = model_materials(&model);
    }
    let _ = resolve_model_record(&model_store, &documented_model_key(), RASTER_TYPE);
    let _ = resolve_model_record(
        &model_store,
        &ResourceKey::new(spore_gmdl::GMDL_TYPE, 0, 0),
        RASTER_TYPE,
    );
    // A non-gmdl key, which must be refused before any read.
    let _ = resolve_model_record(
        &model_store,
        &ResourceKey::new(RASTER_TYPE, 0x4063_2900, DOCUMENTED_INSTANCE_ID),
        RASTER_TYPE,
    );
}

#[test]
fn no_prefix_of_a_raster_record_panics_any_entry_point() {
    let record = dxt5_raster();
    for length in 0..=record.len() {
        exercise_all_entry_points(record[..length].to_vec());
    }
    // And the record itself, which must succeed.
    let store = store_with(record);
    assert!(resolve_texture(&store, target(), RASTER_TYPE).is_ok());
}

#[test]
fn no_single_byte_mutation_of_a_raster_record_panics() {
    let record = dxt5_raster();
    for offset in 0..record.len() {
        for value in [0x00u8, 0x01, 0x7F, 0x80, 0xFF] {
            let mut mutated = record.clone();
            mutated[offset] = value;
            exercise_all_entry_points(mutated);
        }
    }
}

#[test]
fn absurd_raster_headers_are_refused_rather_than_allocated() {
    // Every one of these is a header a hostile or corrupt record could carry.
    let absurd: [(u32, u32, u32, u32, u32, usize); 8] = [
        (0, 0, 0, 0, DXT5_FOURCC, 1), // zero everything
        (u32::MAX, u32::MAX, u32::MAX, u32::MAX, DXT5_FOURCC, 1), // every field max
        (1, 0xFFFF_FFFF, 0xFFFF_FFFF, 1, DXT5_FOURCC, 1), // dimensions that cannot allocate
        (1, 0x7FFF_FFFF, 0x7FFF_FFFF, 0xFFFF_FFFF, DXT5_FOURCC, 1), // mip count overflows
        (1, 0x8000_0000, 0x8000_0000, 1, DXT5_FOURCC, 1), // dimensions whose block count overflows
        (1, 1, 1, 1, 0, 1),           // fourcc zero
        (1, 1, 1, 1, 0x1500_0000, 1), // the luminance family, other spelling
        (1, 4, 4, 31, DXT5_FOURCC, 4), // a chain that does not divide
    ];
    for (_version, width, height, mips, fourcc, layers) in absurd {
        // Only the header is materialized: the point is that the decoders
        // refuse the *declared* geometry without allocating for it.
        let record = raster_header(width, height, mips, fourcc, layers * 2);
        exercise_all_entry_points(record.clone());

        // And the refusal must come from the *texture* layer, not from a store
        // miss. Without this the sweep would pass for the wrong reason if the
        // fixture's key were ever wrong: every case would be a `Lookup` and the
        // decoders would never have been reached.
        let error = resolve_texture(&store_with(record), target(), RASTER_TYPE)
            .expect_err("an absurd header must never decode");
        assert!(
            matches!(
                error,
                MaterialError::Decode { .. } | MaterialError::UnsupportedFourcc { .. }
            ),
            "{width}x{height} mips={mips} fourcc=0x{fourcc:08x} layers={layers}: \
             expected a texture-layer refusal, got {error:?}"
        );
    }
    // The 0x15xx spelling is refused by name, whatever the byte order.
    let store = store_with(raster_header(4, 4, 1, 0x1500_0000, 2));
    let error = resolve_texture(&store, target(), RASTER_TYPE).unwrap_err();
    assert!(matches!(
        error,
        MaterialError::UnsupportedFourcc {
            fourcc: 0x1500_0000,
            ..
        }
    ));
}

#[test]
fn no_prefix_of_a_model_record_panics() {
    let record = documented_gmdl();
    for length in 0..=record.len() {
        let store = store_over(&[
            (documented_model_key(), record[..length].to_vec()),
            (
                raster_key(0x4063_2900, DOCUMENTED_INSTANCE_ID),
                dxt5_raster(),
            ),
        ]);
        let _ = resolve_model_record(&store, &documented_model_key(), RASTER_TYPE);
        let _ = resolve_model_textures(&store, &spore_gmdl::parse(&record).unwrap(), RASTER_TYPE);
    }
}

#[test]
fn no_single_byte_mutation_of_a_model_record_panics() {
    let record = documented_gmdl();
    for offset in 0..record.len() {
        for value in [0x00u8, 0x01, 0x7F, 0x80, 0xFF] {
            let mut mutated = record.clone();
            mutated[offset] = value;
            let store = store_over(&[
                (documented_model_key(), mutated),
                (
                    raster_key(0x4063_2900, DOCUMENTED_INSTANCE_ID),
                    dxt5_raster(),
                ),
            ]);
            let _ = resolve_model_record(&store, &documented_model_key(), RASTER_TYPE);
        }
    }
}

#[test]
fn a_model_whose_shader_id_has_no_size_does_not_panic_and_resolves_nothing() {
    let store = store_over(&[(documented_model_key(), undocumented_shader_gmdl())]);
    let error = resolve_model_record(&store, &documented_model_key(), RASTER_TYPE).unwrap_err();
    assert!(matches!(error, MaterialError::ModelDecode { .. }));
    // An empty raster payload and an empty store both answer rather than panic.
    exercise_all_entry_points(Vec::new());
    let empty = ContentStore::new();
    let _ = resolve_texture(&empty, target(), RASTER_TYPE);
    let _ = resolve_model_record(&empty, &documented_model_key(), RASTER_TYPE);
}

#[test]
fn a_model_record_with_no_materials_resolves_to_no_materials() {
    // meshCount 0, no material ids, no texture refs: three absences, none of
    // which is an error and none of which is silently turned into a default.
    let mut record = Vec::new();
    support::push_u32(&mut record, spore_gmdl::SUPPORTED_VERSION);
    record.extend_from_slice(&0u32.to_be_bytes()); // referenced files
    support::push_u32(&mut record, 0); // mesh count
    for value in [0.0f32, 0.0, 0.0, 1.0, 1.0, 1.0] {
        record.extend_from_slice(&value.to_le_bytes());
    }
    record.extend_from_slice(&0.0f32.to_le_bytes());
    for _ in 0..3 {
        support::push_u32(&mut record, 0); // buffers, descriptors, vertex buffers
    }
    support::push_u32(&mut record, 0); // the observed zero word
    support::push_u32(&mut record, 0); // material-info block count
                                       // Five trailer words: bone-range count, anim-data count and the three-word
                                       // key. A sixth would leave four bytes unread and make the record look
                                       // partially walked when it is not.
    for _ in 0..5 {
        support::push_u32(&mut record, 0); // trailer words
    }

    let store = store_over(&[(documented_model_key(), record)]);
    let resolved = resolve_model_record(&store, &documented_model_key(), RASTER_TYPE).unwrap();
    assert!(resolved.textures.is_empty());
    assert!(resolved.meshes.is_empty());
    assert_eq!(resolved.resolved_count(), 0);
    assert!(resolved.failures().is_empty());
}

/// This test asserted that a `0x15` record answers `inspect_envelope` but is
/// refused by `resolve_texture`. The corpus refuted the second half: the whole
/// `0x15` family decodes (901/901 records in `Spore_Content`), under its own
/// framing. Rewritten rather than deleted, because the refutation must stay
/// visible and because the inspect/decode split it protected is still real.
#[test]
fn a_luminance_record_answers_inspect_and_decodes_now() {
    let store = store_over(&[(
        raster_key(0x4063_2900, DOCUMENTED_INSTANCE_ID),
        luminance_raster(8, 8),
    )]);
    let envelope = inspect_envelope(&store, target(), RASTER_TYPE).unwrap();
    assert_eq!(envelope.envelope.fourcc, LUMINANCE_FOURCC);

    let resolved = resolve_texture(&store, target(), RASTER_TYPE)
        .expect("the 0x15 family is decodable, not refused");
    assert_eq!(resolved.image.width, 8);
    assert_eq!(resolved.image.height, 8);
    // `inspect_envelope` derives no layer count because it reads no payload;
    // the decode does derive one. The two entry points therefore still cannot
    // be confused for one another, which was the property worth protecting.
    assert_eq!(resolved.layer_count, 1);
}

/// The inspect/decode split, on the case that can still fail: a CORRUPT `0x15`
/// record must be inspectable -- reading a header is not decoding a format --
/// while refusing to produce pixels.
#[test]
fn a_corrupt_luminance_record_still_answers_inspect_but_not_decode() {
    let store = store_over(&[(
        raster_key(0x4063_2900, DOCUMENTED_INSTANCE_ID),
        malformed_luminance_raster(8, 8),
    )]);
    let envelope = inspect_envelope(&store, target(), RASTER_TYPE).unwrap();
    assert_eq!(
        envelope.envelope.fourcc, LUMINANCE_FOURCC,
        "the cheap question is still answerable"
    );
    assert!(
        resolve_texture(&store, target(), RASTER_TYPE).is_err(),
        "but a broken payload must not yield pixels"
    );
}

#[test]
fn a_record_whose_payload_is_not_the_declared_size_is_refused_by_the_store() {
    // The DBPF layer's own size invariant: a row whose stored and memory sizes
    // disagree is corruption, and this crate must not paper over it.
    let mut records = dbpf_package(&[(
        raster_key(0x4063_2900, DOCUMENTED_INSTANCE_ID),
        dxt5_raster(),
    )]);
    // The memory-size word is the sixth row word of the first row: 4 (flags) +
    // type + group + instance + offset + stored = 24 bytes in, memory size at 24.
    let memory_size_at = 96 + 4 + 24;
    records[memory_size_at..memory_size_at + 4].copy_from_slice(&7u32.to_le_bytes());
    let mut store = ContentStore::new();
    store
        .push(spore_assets::Package::from_vec("mutated", records).expect("the index still parses"));
    let error = resolve_texture(&store, target(), RASTER_TYPE).unwrap_err();
    assert!(matches!(error, MaterialError::Lookup { .. }));
    assert!(error.to_string().contains("0x2f4e681c"), "{}", error);
}
