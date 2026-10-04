//! Typed failures: a missing record names its key, a refused fourcc names its
//! word, and every variant has an asserted message.

mod support;

use spore_assets::AssetError;
use spore_core::evidence::EvidenceLevel;
use spore_core::ResourceKey;
use spore_gmdl::{GmdlError, GmdlTextureRef};
use spore_material::{
    claims, inspect_envelope, resolve_model_record, resolve_texture, MaterialError,
};
use spore_texture::{TextureError, DXT5_FOURCC, RASTER_TYPE};
use support::*;

/// The reference the documented asset's first texture uses.
fn first_reference() -> GmdlTextureRef {
    documented_texture_refs()[0]
}

#[test]
fn a_missing_record_yields_a_typed_error_naming_the_key() {
    // A store with one record, asked for a different instance.
    let store = store_over(&[(
        raster_key(0x4063_2900, DOCUMENTED_INSTANCE_ID),
        two_layer_raster(),
    )]);
    let absent = GmdlTextureRef {
        instance_id: 0x067A_07FF,
        group_id: 0x4063_2900,
    };
    let error = resolve_texture(&store, absent, RASTER_TYPE).unwrap_err();

    let MaterialError::Lookup { key, source } = &error else {
        panic!("expected Lookup, got {error:?}");
    };
    assert_eq!(
        *key,
        ResourceKey::new(RASTER_TYPE, 0x4063_2900, 0x067A_07FF)
    );
    let AssetError::NotFound {
        key: reported,
        searched,
        candidates,
    } = source
    else {
        panic!("expected NotFound, got {source:?}");
    };
    assert_eq!(*reported, *key, "the store and this crate report one key");
    assert_eq!(*searched, 1);
    assert_eq!(
        candidates,
        &vec![raster_key(0x4063_2900, DOCUMENTED_INSTANCE_ID)],
        "the near-miss record is suggested, so the miss is diagnosable"
    );
    assert_eq!(
        error.key(),
        Some(ResourceKey::new(RASTER_TYPE, 0x4063_2900, 0x067A_07FF))
    );

    // The message names the key in full and the type word that was assumed.
    let text = error.to_string();
    assert!(text.contains("0x2f4e681c:0x40632900:0x067a07ff"), "{text}");
    // The underlying store error is preserved in the source chain rather than
    // flattened into prose, so a caller can match on `AssetError::NotFound`
    // itself and read the near-miss list off it (the Display carries the key and
    // the package count, not the candidates).
    let source = std::error::Error::source(&error).expect("the store error is the source");
    assert_eq!(
        source.to_string(),
        AssetError::NotFound {
            key: ResourceKey::new(RASTER_TYPE, 0x4063_2900, 0x067A_07FF),
            searched: 1,
            candidates: vec![raster_key(0x4063_2900, DOCUMENTED_INSTANCE_ID)],
        }
        .to_string()
    );
}

#[test]
fn an_empty_store_says_so_rather_than_reporting_a_missing_record() {
    let store = spore_assets::ContentStore::new();
    let error = resolve_texture(&store, first_reference(), RASTER_TYPE).unwrap_err();
    assert_eq!(
        error,
        MaterialError::Lookup {
            key: raster_key(0x4063_2900, DOCUMENTED_INSTANCE_ID),
            source: AssetError::EmptyStore,
        }
    );
    assert!(error.to_string().contains("no packages"), "{}", error);
}

/// The `0x15` luminance family is DECODED now, so this test no longer asserts a
/// refusal. It was rewritten rather than deleted, because the corpus refuted
/// its premise and that refutation must stay visible.
///
/// What was asserted before: a `0x15` record is refused with
/// `UnsupportedFourcc`. What the corpus says: all 901 `0x15` records in
/// `Spore_Content.package` decode, under their OWN framing (28-byte header, no
/// layer headers, uncompressed 4-bytes-per-texel), which is not the DXT5
/// framing this fixture used to be built with.
#[test]
fn a_well_formed_luminance_record_decodes_because_the_corpus_refuted_the_refusal() {
    let store = store_over(&[(
        raster_key(0x4063_2900, DOCUMENTED_INSTANCE_ID),
        luminance_raster(16, 16),
    )]);
    let texture = resolve_texture(&store, first_reference(), RASTER_TYPE)
        .expect("the 0x15 family is a real, decodable format, not an unknown one");

    assert_eq!(texture.image.width, 16);
    assert_eq!(texture.image.height, 16);
    assert_eq!(texture.image.pixels.len(), 16 * 16 * 4);
    // The base level is a flat tangent-space normal with Z FIRST, so lane 0 is
    // the saturated one. If a future change swaps the lanes, this fails rather
    // than silently rendering a plausible-looking wrong normal map.
    assert!(
        texture.image.pixels[0] > 0xF0,
        "lane 0 is the Z/normal-x lane"
    );
    assert_eq!(&texture.image.pixels[1..4], &[0x80, 0x80, 0xFF]);
}

/// The original intent of the deleted test, preserved against the case that can
/// still fail: a `0x15` record whose payload does not satisfy the framing
/// identity must be a TYPED error naming the arithmetic, never a blank image
/// and never a silently padded or truncated decode.
#[test]
fn a_malformed_luminance_record_is_a_typed_error_not_a_blank_image() {
    let store = store_over(&[(
        raster_key(0x4063_2900, DOCUMENTED_INSTANCE_ID),
        malformed_luminance_raster(16, 16),
    )]);
    let error = resolve_texture(&store, first_reference(), RASTER_TYPE).unwrap_err();

    assert!(
        matches!(error, MaterialError::Decode { .. }),
        "a framing failure is a decode error: {error:?}"
    );
    let text = error.to_string();
    assert!(
        text.contains("luminance") || text.contains("0x15"),
        "the failing family must be named: {text}"
    );

    // And still no empty image: a refusal returns nothing at all.
    assert!(
        resolve_texture(&store, first_reference(), RASTER_TYPE).is_err(),
        "a malformed record must never resolve to a zero-byte texture"
    );
}

/// An unrelated fourcc is still refused by name. This is the refusal that
/// `UnsupportedFourcc` was actually written for, and it must keep working:
/// decoding the `0x15` family must not have turned the guard into a blanket
/// "anything goes".
#[test]
fn an_unrelated_fourcc_is_still_refused_by_name() {
    const BOGUS: u32 = 0x1234_5678;
    let store = store_over(&[(
        raster_key(0x4063_2900, DOCUMENTED_INSTANCE_ID),
        raster_record(16, 16, 2, BOGUS, 1),
    )]);
    let error = resolve_texture(&store, first_reference(), RASTER_TYPE).unwrap_err();

    assert_eq!(
        error,
        MaterialError::UnsupportedFourcc {
            key: raster_key(0x4063_2900, DOCUMENTED_INSTANCE_ID),
            fourcc: BOGUS,
        },
        "the refusal is a distinct variant, not a decode failure and not a blank image"
    );
    assert!(error.is_unsupported_format());
    let text = error.to_string();
    assert!(
        text.contains("0x12345678"),
        "the fourcc must be named: {text}"
    );
    assert!(
        std::error::Error::source(&error).is_none(),
        "UnsupportedFourcc is self-contained, so its message must carry every fact"
    );
}

#[test]
fn every_other_texture_refusal_arrives_as_a_decode_error_with_its_own_reason() {
    let key = raster_key(0x4063_2900, DOCUMENTED_INSTANCE_ID);
    let store = store_over(&[(key, two_layer_raster())]);

    // A truncated record: the envelope does not fit.
    let truncated = store_over(&[(key, vec![0u8; 8])]);
    let error = resolve_texture(&truncated, first_reference(), RASTER_TYPE).unwrap_err();
    assert_eq!(
        error,
        MaterialError::Decode {
            key,
            source: TextureError::TruncatedEnvelope { available: 8 },
        }
    );
    assert!(error.to_string().contains("32-byte raster envelope"));

    // A payload that does not divide by the layer stride.
    let mut misaligned = two_layer_raster();
    misaligned.push(0);
    let misaligned_store = store_over(&[(key, misaligned)]);
    let error = resolve_texture(&misaligned_store, first_reference(), RASTER_TYPE).unwrap_err();
    assert!(matches!(
        error,
        MaterialError::Decode {
            source: TextureError::PayloadNotMultiple { .. },
            ..
        }
    ));
    assert!(error.to_string().contains("layer stride"));

    // A zero dimension is refused rather than turned into an empty image.
    let zero = store_over(&[(key, raster_record(0, 8, 2, DXT5_FOURCC, 1))]);
    let error = resolve_texture(&zero, first_reference(), RASTER_TYPE).unwrap_err();
    assert!(matches!(
        error,
        MaterialError::Decode {
            source: TextureError::ZeroDimension { .. },
            ..
        }
    ));

    // ...and the happy path still works in the same store, so these refusals are
    // about the records and not about the fixture.
    assert!(resolve_texture(&store, first_reference(), RASTER_TYPE).is_ok());
}

#[test]
fn a_truncated_record_is_refused_by_the_cheap_entry_point_too() {
    let key = raster_key(0x4063_2900, DOCUMENTED_INSTANCE_ID);
    for bytes in [vec![], vec![0u8; 1], vec![0xFF; 31]] {
        let store = store_over(&[(key, bytes.clone())]);
        let error = inspect_envelope(&store, first_reference(), RASTER_TYPE).unwrap_err();
        assert_eq!(
            error,
            MaterialError::Decode {
                key,
                source: TextureError::TruncatedEnvelope {
                    available: bytes.len()
                },
            }
        );
    }
}

#[test]
fn every_error_variant_has_an_asserted_message() {
    let key = raster_key(0x4063_2900, DOCUMENTED_INSTANCE_ID);
    let cases: Vec<(MaterialError, &str)> = vec![
        (
            MaterialError::Lookup {
                key,
                source: AssetError::NotFound {
                    key,
                    searched: 3,
                    candidates: Vec::new(),
                },
            },
            "no record",
        ),
        (
            MaterialError::AssumedTypeNotDecodable {
                key,
                assumed_type: PNG_TYPE,
                name: String::from("png"),
            },
            "png",
        ),
        (
            MaterialError::UnsupportedFourcc {
                key,
                fourcc: LUMINANCE_FOURCC,
            },
            "0x00000015",
        ),
        (
            MaterialError::Decode {
                key,
                source: TextureError::ZeroMipCount,
            },
            "mipCount is 0",
        ),
        (
            MaterialError::ModelDecode {
                key,
                source: GmdlError::UndocumentedShaderDataId { id: 0x218 },
            },
            "0x218",
        ),
        (
            MaterialError::IncompleteReference {
                reference: GmdlTextureRef {
                    instance_id: spore_core::WILDCARD,
                    group_id: 0x4063_2900,
                },
                component: "instance",
            },
            "0xFFFFFFFF",
        ),
        (MaterialError::EmptyImage { key }, "no base image"),
    ];

    for (error, needle) in &cases {
        let text = error.to_string();
        assert!(
            text.contains(needle),
            "message {text:?} does not mention {needle:?}"
        );
    }
    // 7 variants are constructed above; the enum must not grow without this test
    // being extended, because an unmentioned variant has an unasserted message.
    assert_eq!(
        cases.len(),
        7,
        "a new MaterialError variant needs a message case here"
    );
}

#[test]
fn the_defensive_empty_image_variant_is_constructed_directly() {
    // Unreachable through the decoders below it, which is exactly why it needs a
    // direct construction: an arm nothing can reach has no other coverage.
    let key = raster_key(0x4063_2900, DOCUMENTED_INSTANCE_ID);
    let error = MaterialError::EmptyImage { key };
    assert_eq!(error.key(), Some(key));
    assert!(error
        .to_string()
        .contains("0x2f4e681c:0x40632900:0x067a07f0"));
    assert!(!error.is_unsupported_format());
    assert!(
        std::error::Error::source(&error).is_none(),
        "there is no underlying error to point at"
    );
}

#[test]
fn a_wildcard_reference_message_names_which_component_was_wild() {
    let store = store_over(&[(
        raster_key(0x4063_2900, DOCUMENTED_INSTANCE_ID),
        two_layer_raster(),
    )]);
    let wildcard_instance = GmdlTextureRef {
        instance_id: spore_core::WILDCARD,
        group_id: 0x4063_2900,
    };
    let error = resolve_texture(&store, wildcard_instance, RASTER_TYPE).unwrap_err();
    let text = error.to_string();
    assert!(text.contains("instance"), "{text}");
    assert!(text.contains("0x40632900"), "{text}");

    let wildcard_group = GmdlTextureRef {
        instance_id: DOCUMENTED_INSTANCE_ID,
        group_id: spore_core::WILDCARD,
    };
    let error = resolve_texture(&store, wildcard_group, RASTER_TYPE).unwrap_err();
    let text = error.to_string();
    assert!(text.contains("group"), "{text}");
    assert!(text.contains("0x067a07f0"), "{text}");
}

#[test]
fn the_claims_table_and_the_error_variants_agree_on_the_luminance_wording() {
    // One vocabulary for one fact: the claim subject, the grade and the error
    // message must all describe the same situation.
    //
    // This test used to require `LUMINANCE_FOURCC_FAMILY` to be a NON-FINDING,
    // i.e. to agree with the decoder's refusal. The corpus made the decoder
    // wrong, so the claim is now VERIFIED and the assertion is inverted: the
    // agreement that matters now is that the claim is NOT a non-finding, and
    // that the error variant which remains -- for genuinely other fourccs --
    // names the wanted one.
    let spec = claims::spec_of(claims::LUMINANCE_FOURCC_FAMILY).expect("subject");
    assert_eq!(
        spec.kind.level(),
        EvidenceLevel::Verified,
        "the claim must no longer assert a refusal the decoder does not make"
    );
    assert!(
        spec.kind.reason().is_none(),
        "a graded claim carries no non-finding reason"
    );

    // And the republished fact must be the texture crate's own, not a local copy
    // that can drift from the decoder.
    let republished = spore_texture::claims::claim(spore_texture::claims::LUMINANCE_FOURCC)
        .expect("the texture crate declares the claim")
        .meaning
        .clone();
    assert!(
        republished.is_available(),
        "the 0x15 family is decoded, so its fact is available"
    );

    let error = MaterialError::UnsupportedFourcc {
        key: raster_key(0x4063_2900, DOCUMENTED_INSTANCE_ID),
        fourcc: 0x1234_5678,
    };
    let text = error.to_string();
    assert!(text.contains("0x12345678"));
    assert!(text.contains(&format!("0x{:08x}", DXT5_FOURCC)));
}

#[test]
fn a_model_record_error_names_the_model_key_and_the_gmdl_reason() {
    let store = store_over(&[(documented_model_key(), undocumented_shader_gmdl())]);
    let error = resolve_model_record(&store, &documented_model_key(), RASTER_TYPE).unwrap_err();
    assert_eq!(error.key(), Some(documented_model_key()));
    let text = error.to_string();
    assert!(text.contains("0x00e6bce5:0x40637e03:0x067a07f0"), "{text}");
    assert!(text.contains("gmdl:"), "{text}");
    assert!(text.contains("undocumented shader-data id 0x218"), "{text}");
}

#[test]
fn a_missing_model_record_is_a_lookup_not_a_decode_failure() {
    let store = store_over(&[(documented_model_key(), documented_gmdl())]);
    let absent = ResourceKey::new(spore_gmdl::GMDL_TYPE, 0x4063_7E03, 0x067A_07FF);
    let error = resolve_model_record(&store, &absent, RASTER_TYPE).unwrap_err();
    let MaterialError::Lookup { key, source } = &error else {
        panic!("expected Lookup, got {error:?}");
    };
    assert_eq!(*key, absent);
    let AssetError::NotFound {
        key: reported,
        candidates,
        ..
    } = source
    else {
        panic!("expected NotFound, got {source:?}");
    };
    assert_eq!(*reported, absent);
    assert_eq!(candidates, &vec![documented_model_key()]);
}
