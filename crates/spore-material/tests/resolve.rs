//! Resolution: the happy path, encounter order, per-reference failure, and the
//! `assumed_type` parameter.
//!
//! Every byte here is synthetic (see `tests/support/`); the *shapes* are the
//! ones `osptool` measured on `Spore_Content.package` on 2026-10-04.

mod support;

use spore_assets::ContentStore;
use spore_core::evidence::{EvidenceLevel, EvidenceState};
use spore_core::ResourceKey;
use spore_gmdl::GmdlTextureRef;
use spore_material::{
    claims, inspect_envelope, resolve_model_record, resolve_model_textures, resolve_texture,
    MaterialError,
};
use spore_texture::{DXT5_FOURCC, RASTER_TYPE};
use support::*;

/// A reference into `group`, at the documented instance.
fn reference(group: u32) -> GmdlTextureRef {
    GmdlTextureRef {
        instance_id: DOCUMENTED_INSTANCE_ID,
        group_id: group,
    }
}

/// The store the documented asset's three references resolve against:
/// two 512x512-style DXT5 records and one luminance record.
fn documented_store() -> ContentStore {
    store_over(&[
        (
            raster_key(0x4063_2900, DOCUMENTED_INSTANCE_ID),
            raster_record(8, 8, 2, DXT5_FOURCC, 2),
        ),
        (
            raster_key(0x4063_2901, DOCUMENTED_INSTANCE_ID),
            raster_record(8, 8, 2, DXT5_FOURCC, 2),
        ),
        (
            raster_key(0x4063_2902, DOCUMENTED_INSTANCE_ID),
            luminance_raster(8, 8),
        ),
    ])
}

/// The same three references, but the third is CORRUPT rather than merely
/// unusual.
///
/// This exists because `documented_store` used to carry a `0x15` record as its
/// "this one fails" case. That stopped being true when the corpus showed the
/// whole `0x15` family decodes. Failure-path coverage needs a record that is
/// actually broken, or the batch error handling is left untested while the test
/// names still claim to cover it.
fn documented_store_with_a_corrupt_third() -> ContentStore {
    store_over(&[
        (documented_model_key(), documented_gmdl()),
        (
            raster_key(0x4063_2900, DOCUMENTED_INSTANCE_ID),
            raster_record(8, 8, 2, DXT5_FOURCC, 2),
        ),
        (
            raster_key(0x4063_2901, DOCUMENTED_INSTANCE_ID),
            raster_record(8, 8, 2, DXT5_FOURCC, 2),
        ),
        (
            raster_key(0x4063_2902, DOCUMENTED_INSTANCE_ID),
            malformed_luminance_raster(8, 8),
        ),
    ])
}

#[test]
fn a_synthetic_raster_resolves_to_the_base_mip_of_layer_zero() {
    let store = store_over(&[(
        raster_key(0x4063_2900, DOCUMENTED_INSTANCE_ID),
        two_layer_raster(),
    )]);
    let resolved = resolve_texture(&store, reference(0x4063_2900), RASTER_TYPE).unwrap();

    // Envelope, carried verbatim.
    assert_eq!(resolved.envelope.fourcc, DXT5_FOURCC);
    assert_eq!(resolved.envelope.version, 1);
    assert_eq!(resolved.envelope.width, 8);
    assert_eq!(resolved.envelope.height, 8);
    assert_eq!(resolved.envelope.mip_count, 2);

    // Counts: mips from the envelope, layers DERIVED from the record size.
    assert_eq!(resolved.mip_count, 2);
    assert_eq!(resolved.layer_count, 2);
    assert_eq!(
        resolved.mip_count, resolved.envelope.mip_count as usize,
        "the published count is the envelope's, not a re-derived one"
    );

    // The image is the base mip of the FIRST layer, and it is not empty.
    assert_eq!(resolved.image.width, 8);
    assert_eq!(resolved.image.height, 8);
    assert_eq!(resolved.pixel_len(), 8 * 8 * 4);
    assert_eq!(resolved.pixel_len(), resolved.image.pixels.len());
    assert!(
        resolved.pixels_decoded(),
        "a full resolve decodes pixels, and says so"
    );

    // The first block of layer 0 is the flat block, whose every texel is
    // palette[0] = (8,0,0) with the reversed-free ramp's entry round(2*255/8).
    assert_eq!(&resolved.image.pixels[0..4], &[8, 0, 0, 64]);
    // Block index 2 of layer 0 mip 0 is `addressing_block((1*37 + 1*11 + 2) % 16)
    // = addressing_block(2)`, whose texel `t` takes code `(2 >> t) & 1`: texel 0
    // takes code 0 and texel 1 — the only set bit of 2 — takes code 1. Block 2
    // starts at texel 16, i.e. byte 64 of the base mip. A decode that read the
    // wrong layer, mip or block would not put a zero alpha exactly there.
    let block_2 = 2 * 4 * 4 * 4;
    assert_eq!(&resolved.image.pixels[block_2..block_2 + 4], &[8, 0, 0, 64]);
    let block_2_texel_1 = block_2 + 4;
    assert_eq!(
        &resolved.image.pixels[block_2_texel_1..block_2_texel_1 + 4],
        &[0, 0, 0, 0],
        "code 1 is palette[1] = (0,0,0) with the ramp's zero alpha"
    );

    assert_eq!(resolved.kind.as_str(), "raster");
    assert_eq!(resolved.kind.type_id(), RASTER_TYPE);
    assert_eq!(
        resolved.key,
        ResourceKey::new(RASTER_TYPE, 0x4063_2900, DOCUMENTED_INSTANCE_ID),
        "the key published is the one used, with the type made explicit"
    );
}

#[test]
fn the_resolution_records_what_it_assumed_and_what_it_does_not_know() {
    let store = documented_store();
    let resolved = resolve_texture(&store, reference(0x4063_2900), RASTER_TYPE).unwrap();

    let assumed = resolved
        .claim(claims::ASSUMED_RECORD_TYPE)
        .expect("subject");
    assert!(assumed.fact.is_available());
    assert_eq!(assumed.fact.level(), EvidenceLevel::Inferred);
    assert_eq!(assumed.fact.evidence_state(), EvidenceState::Derived);
    assert_eq!(assumed.fact.provenance().len(), 1);
    assert!(
        assumed
            .fact
            .value()
            .unwrap()
            .starts_with("ASSUMED, not read from the record"),
        "the assumption must be visible in the value, not only in the subject"
    );

    // The reference itself carries no type: a non-finding, provenance free.
    let absent = resolved
        .claim(claims::TEXTURE_REFERENCE_TYPE_WORD)
        .expect("subject");
    assert!(absent.fact.is_unavailable());
    assert_eq!(
        absent.fact.reason(),
        Some(claims::TEXTURE_REF_HAS_NO_TYPE_WORD)
    );
    assert_eq!(absent.fact.level(), EvidenceLevel::Unknown);
    assert!(absent.fact.provenance().is_empty());

    // The three unnamed envelope words stay unknown.
    for subject in [
        claims::ENVELOPE_FIELD_10_MEANING,
        claims::ENVELOPE_FIELD_18_MEANING,
        claims::ENVELOPE_FIELD_1C_MEANING,
    ] {
        let claim = resolved.claim(subject).expect(subject);
        assert!(claim.fact.is_unavailable(), "{subject}");
        assert_eq!(claim.fact.level(), EvidenceLevel::Unknown, "{subject}");
        assert!(claim.fact.provenance().is_empty(), "{subject}");
    }

    // The luminance family used to be in that loop. It is not any more: the
    // corpus refuted the refusal, `spore-texture` decoded all 901 records, and
    // the claim moved to VERIFIED. Keeping it here would have been a fifth
    // place asserting the decoder was wrong.
    let luminance = resolved
        .claim(claims::LUMINANCE_FOURCC_FAMILY)
        .expect("subject");
    assert!(
        luminance.fact.is_available(),
        "the 0x15 family is decoded, so its fact is a value now"
    );
    assert_eq!(luminance.fact.level(), EvidenceLevel::Verified);
    assert!(
        !luminance.fact.provenance().is_empty(),
        "a verified claim must say where it was measured"
    );

    // `raster` IS named by the canonical type table, so that one is a value.
    let named = resolved
        .claim(claims::ASSUMED_RECORD_TYPE_NAME)
        .expect("subject");
    assert!(named.fact.is_available());
    assert_eq!(named.fact.value(), Some(&"raster"));
    assert_eq!(named.fact.level(), EvidenceLevel::Verified);
    assert_eq!(named.fact.evidence_state(), EvidenceState::Persisted);

    // The group is NOT named, which is a non-finding for all three documented
    // groups.
    let group = resolved
        .claim(claims::RESOLVED_GROUP_NAME)
        .expect("subject");
    assert!(group.fact.is_unavailable());
    assert_eq!(
        group.fact.reason(),
        Some(claims::NO_GROUP_NAME_IN_CANONICAL_TABLE)
    );
    // The unknown set is enumerated rather than floored. It used to be
    // `>= 6`, which survived the luminance claim moving from non-finding to
    // VERIFIED only by accident -- a floor cannot notice a fact being *lost*.
    let unknowns: Vec<&str> = resolved
        .unknowns()
        .map(|claim| claim.subject.as_str())
        .collect();
    assert_eq!(
        unknowns,
        vec![
            claims::TEXTURE_REFERENCE_TYPE_WORD,
            claims::RESOLVED_GROUP_NAME,
            claims::ENVELOPE_FIELD_10_MEANING,
            claims::ENVELOPE_FIELD_18_MEANING,
            claims::ENVELOPE_FIELD_1C_MEANING,
            // The mask layer's PURPOSE is a non-finding even though its content
            // is known and graded: content is not purpose. Adding it here is
            // the point -- a claim that stops being recorded is as much a
            // regression as one that starts.
            claims::MASK_LAYER_PURPOSE,
        ],
        "six facts are genuinely unknown, and this list is the whole of them"
    );
    assert!(
        !unknowns.contains(&claims::LUMINANCE_FOURCC_FAMILY),
        "the 0x15 family is no longer one of the unknowns"
    );

    // The codec's departures from the BC3 specification are re-published, so a
    // caller reading only these claims learns that the pixels are not spec DXT5.
    let deviations = resolved
        .claim(claims::DXT5_SPEC_DEVIATIONS)
        .expect("subject");
    assert_eq!(deviations.fact.level(), EvidenceLevel::Verified);
}

#[test]
fn every_recorded_claim_is_graded_exactly_as_the_claims_table_documents() {
    let store = documented_store();
    let resolved = resolve_texture(&store, reference(0x4063_2900), RASTER_TYPE).unwrap();
    assert!(!resolved.claims.is_empty());
    for claim in &resolved.claims {
        let spec = claims::spec_of(&claim.subject)
            .unwrap_or_else(|| panic!("{} is not a declared subject", claim.subject));
        if claim.fact.is_unavailable() {
            assert_eq!(
                claim.fact.level(),
                EvidenceLevel::Unknown,
                "{}: a non-finding is UNKNOWN whatever its documented grade",
                claim.subject
            );
            assert_eq!(
                claim.fact.reason(),
                spec.kind.reason(),
                "{}: the reason code comes from the claims table",
                claim.subject
            );
            assert!(claim.fact.provenance().is_empty(), "{}", claim.subject);
        } else {
            assert!(
                !claim.fact.provenance().is_empty(),
                "{}: an unsourced claim is the failure this crate prevents",
                claim.subject
            );
            assert_eq!(
                claim.fact.level(),
                spec.kind.level(),
                "{}: a recorded value carries its documented grade",
                claim.subject
            );
        }
        assert!(
            claims::spec_of(&claim.subject).is_some(),
            "{} must be a declared subject",
            claim.subject
        );
    }
}

#[test]
fn results_come_back_in_the_order_the_record_referenced_them() {
    // Three resolvable textures of three distinct sizes, so "in order" is
    // observable by value and not merely by counting.
    let store = store_over(&[
        (
            raster_key(0x4063_2900, DOCUMENTED_INSTANCE_ID),
            raster_record(8, 8, 2, DXT5_FOURCC, 2),
        ),
        (
            raster_key(0x4063_2901, DOCUMENTED_INSTANCE_ID),
            raster_record(16, 16, 1, DXT5_FOURCC, 1),
        ),
        (
            raster_key(0x4063_2902, DOCUMENTED_INSTANCE_ID),
            raster_record(4, 4, 1, DXT5_FOURCC, 1),
        ),
    ]);
    let model = spore_gmdl::parse(&documented_gmdl()).unwrap();
    assert_eq!(model.texture_refs, documented_texture_refs());

    let results = resolve_model_textures(&store, &model, RASTER_TYPE);
    assert_eq!(results.len(), 3);
    let widths: Vec<u32> = results
        .iter()
        .map(|result| result.as_ref().expect("all three resolve").image.width)
        .collect();
    assert_eq!(widths, vec![8, 16, 4], "encounter order, not sorted order");
    let groups: Vec<u32> = results
        .iter()
        .map(|result| result.as_ref().unwrap().key.group_id)
        .collect();
    assert_eq!(groups, DOCUMENTED_TEXTURE_GROUPS.to_vec());
}

#[test]
fn one_failed_reference_does_not_abort_the_batch() {
    // The documented asset's real shape: two DXT5 records and one `0x15`
    // luminance record. This test used to assert the third FAILED, because the
    // `0x15` family was refused by name. The corpus refuted that -- all 901
    // `0x15` records in `Spore_Content` decode -- so the batch-resilience
    // property is now pinned against a record that is genuinely corrupt.
    let store = documented_store_with_a_corrupt_third();
    let model = spore_gmdl::parse(&documented_gmdl()).unwrap();
    let results = resolve_model_textures(&store, &model, RASTER_TYPE);

    assert_eq!(results.len(), 3);
    assert!(results[0].is_ok(), "first reference");
    assert!(results[1].is_ok(), "second reference");
    let third = results[2].as_ref().unwrap_err();
    assert!(
        matches!(third, MaterialError::Decode { .. }),
        "the corrupt third reference is a decode error naming its arithmetic: {third:?}"
    );
    // Positional alignment with the reference list is what makes the vec usable.
    assert_eq!(results.iter().filter(|entry| entry.is_ok()).count(), 2);
}

/// The complement of the test above, and the reason the fixture had to change:
/// with three VALID records the batch is three successes. This is what
/// `textures=3/3` on the real asset is made of.
#[test]
fn a_fully_valid_model_resolves_every_reference() {
    let store = documented_store();
    let model = spore_gmdl::parse(&documented_gmdl()).unwrap();
    let results = resolve_model_textures(&store, &model, RASTER_TYPE);

    assert_eq!(results.len(), 3);
    assert_eq!(
        results.iter().filter(|entry| entry.is_ok()).count(),
        3,
        "the 0x15 family is no longer the odd one out"
    );
}

#[test]
fn a_missing_record_in_the_middle_leaves_its_neighbours_intact() {
    let store = store_over(&[
        (
            raster_key(0x4063_2900, DOCUMENTED_INSTANCE_ID),
            raster_record(8, 8, 2, DXT5_FOURCC, 2),
        ),
        // 0x40632901 is deliberately absent.
        (
            raster_key(0x4063_2902, DOCUMENTED_INSTANCE_ID),
            raster_record(4, 4, 1, DXT5_FOURCC, 1),
        ),
    ]);
    let model = spore_gmdl::parse(&documented_gmdl()).unwrap();
    let results = resolve_model_textures(&store, &model, RASTER_TYPE);

    assert_eq!(results.len(), 3);
    assert!(results[0].is_ok());
    assert!(results[1].is_err());
    assert!(
        results[2].is_ok(),
        "a failure must not remove a later success"
    );
    assert_eq!(results[2].as_ref().unwrap().image.width, 4);
}

#[test]
fn the_assumed_type_is_honoured_and_the_failure_names_it() {
    let store = documented_store();
    let key = ResourceKey::new(RASTER_TYPE, 0x4063_2900, DOCUMENTED_INSTANCE_ID);

    // Wrong type, no record of that type at that identity: a MISS, and the key
    // in the error carries the type word that was assumed.
    let error = resolve_texture(&store, reference(0x4063_2900), RW4_TYPE).unwrap_err();
    assert_eq!(
        error.key(),
        Some(ResourceKey::new(
            RW4_TYPE,
            0x4063_2900,
            DOCUMENTED_INSTANCE_ID
        ))
    );
    assert!(matches!(error, MaterialError::Lookup { .. }));
    let text = error.to_string();
    assert!(
        text.contains("0x2f4e681b"),
        "the assumed type must be in the message: {text}"
    );

    // Wrong type, and a record of that type IS present: not a miss, a refusal to
    // decode a container that is not a texture.
    let with_rw4 = store_over(&[(
        ResourceKey::new(RW4_TYPE, 0x4063_2900, DOCUMENTED_INSTANCE_ID),
        b"RW4w32\0\0\x04\x00454\x00000".to_vec(),
    )]);
    let error = resolve_texture(&with_rw4, reference(0x4063_2900), RW4_TYPE).unwrap_err();
    assert_eq!(
        error,
        MaterialError::AssumedTypeNotDecodable {
            key: ResourceKey::new(RW4_TYPE, 0x4063_2900, DOCUMENTED_INSTANCE_ID),
            assumed_type: RW4_TYPE,
            name: String::from("rw4"),
        }
    );
    assert!(error.is_unsupported_format());
    let text = error.to_string();
    assert!(text.contains("rw4"), "{text}");
    assert!(
        text.contains("0x2f4e681c"),
        "the wanted type must be named: {text}"
    );

    // A raw PNG record is found, and is still not a texture.
    let with_png = store_over(&[(
        ResourceKey::new(PNG_TYPE, 0x4063_2900, DOCUMENTED_INSTANCE_ID),
        b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR".to_vec(),
    )]);
    let error = resolve_texture(&with_png, reference(0x4063_2900), PNG_TYPE).unwrap_err();
    assert_eq!(
        error,
        MaterialError::AssumedTypeNotDecodable {
            key: ResourceKey::new(PNG_TYPE, 0x4063_2900, DOCUMENTED_INSTANCE_ID),
            assumed_type: PNG_TYPE,
            name: String::from("png"),
        }
    );
    let text = error.to_string();
    assert!(text.contains("png"), "{text}");
    assert!(text.contains("0x2f7d0004"), "{text}");

    // A type with no canonical name still names itself in hex rather than
    // inventing a name for it.
    let unknown = store_over(&[(
        ResourceKey::new(0xDEAD_BEEF, 0x4063_2900, DOCUMENTED_INSTANCE_ID),
        vec![0u8; 4],
    )]);
    let error = resolve_texture(&unknown, reference(0x4063_2900), 0xDEAD_BEEF).unwrap_err();
    assert!(matches!(
        error,
        MaterialError::AssumedTypeNotDecodable { name, .. } if name == "0xdeadbeef"
    ));

    // And the right type still resolves, with the raster record present.
    assert!(resolve_texture(&store, reference(0x4063_2900), RASTER_TYPE).is_ok());
    assert_eq!(key.type_id, RASTER_TYPE);
}

#[test]
fn inspecting_an_envelope_answers_the_cheap_question_without_pixels() {
    let store = documented_store();
    let envelope = inspect_envelope(&store, reference(0x4063_2900), RASTER_TYPE).unwrap();
    assert_eq!(envelope.envelope.width, 8);
    assert_eq!(envelope.envelope.height, 8);
    assert_eq!(envelope.mip_count, 2);
    assert_eq!(
        envelope.pixel_len(),
        0,
        "no pixels were decoded, and a count of zero is the honest report"
    );
    assert!(
        !envelope.pixels_decoded(),
        "an empty image must be distinguishable from a decode that produced nothing"
    );
    assert_eq!(
        envelope.layer_count, 0,
        "the layer count is derived from the payload, which was never read"
    );
    assert!(
        envelope.claim(claims::ASSUMED_RECORD_TYPE).is_some(),
        "the envelope answer carries the same assumption record as the full one"
    );
    assert!(envelope
        .claim(claims::TEXTURE_REFERENCE_TYPE_WORD)
        .is_some());

    // The `0x15` record's envelope is inspectable, and `inspect_envelope`
    // reports NO pixels: that is the entire point of the cheap entry point, and
    // it is why "the header parsed" can never be mistaken for "the texture
    // decoded".
    let luminance = inspect_envelope(&store, reference(0x4063_2902), RASTER_TYPE).unwrap();
    assert_eq!(luminance.envelope.fourcc, LUMINANCE_FOURCC);
    assert_eq!(
        luminance.layer_count, 0,
        "inspect derives no layer count: it reads no payload, so it reports zero"
    );

    // It now decodes as well. That half of this assertion used to be a refusal,
    // which is the change the corpus forced.
    let decoded = resolve_texture(&store, reference(0x4063_2902), RASTER_TYPE)
        .expect("the 0x15 family decodes");
    assert_eq!(
        decoded.layer_count, 1,
        "the decode does derive a layer count"
    );

    // And the refusal path is still reachable, for a record that is actually
    // broken rather than merely in a family that used to be unsupported.
    let broken = store_over(&[
        (documented_model_key(), documented_gmdl()),
        (
            raster_key(0x4063_2900, DOCUMENTED_INSTANCE_ID),
            raster_record(8, 8, 2, DXT5_FOURCC, 2),
        ),
        (
            raster_key(0x4063_2901, DOCUMENTED_INSTANCE_ID),
            raster_record(8, 8, 2, DXT5_FOURCC, 2),
        ),
        (
            raster_key(0x4063_2902, DOCUMENTED_INSTANCE_ID),
            malformed_luminance_raster(8, 8),
        ),
    ]);
    let refused = resolve_texture(&broken, reference(0x4063_2902), RASTER_TYPE).unwrap_err();
    assert!(
        matches!(refused, MaterialError::Decode { .. }),
        "a corrupt payload is a typed refusal, not an empty image: {refused:?}"
    );
    assert!(
        inspect_envelope(&broken, reference(0x4063_2902), RASTER_TYPE).is_ok(),
        "reading a header is not decoding a format, so inspect still answers"
    );
}

#[test]
fn a_model_record_resolves_through_the_gmdl_layer() {
    let store = store_over(&[
        (documented_model_key(), documented_gmdl()),
        (
            raster_key(0x4063_2900, DOCUMENTED_INSTANCE_ID),
            raster_record(8, 8, 2, DXT5_FOURCC, 2),
        ),
        (
            raster_key(0x4063_2901, DOCUMENTED_INSTANCE_ID),
            raster_record(8, 8, 2, DXT5_FOURCC, 2),
        ),
        (
            raster_key(0x4063_2902, DOCUMENTED_INSTANCE_ID),
            luminance_raster(8, 8),
        ),
    ]);

    let resolved = resolve_model_record(&store, &documented_model_key(), RASTER_TYPE).unwrap();
    assert_eq!(resolved.key, documented_model_key());
    assert_eq!(resolved.textures.len(), 3);
    assert_eq!(
        resolved.resolved_count(),
        3,
        "all three references now resolve"
    );

    // One mesh, no name, SIX non-findings, and this mesh's own slice of the
    // model's texture set. One mesh and three references divides evenly and the
    // record's own group-id ordinals agree with the division, so the scope is
    // the inferred per-mesh cut rather than a fallback.
    //
    // Six, not five: `per_mesh_cut_contradicted_by_record_ordinals` was added
    // with the third scope, and on this record it is a non-finding -- the record
    // was asked whether the cut was its block boundaries and it said yes.
    // (Asserted against `material` below, once it is bound.)
    assert_eq!(resolved.meshes.len(), 1);
    assert_eq!(
        resolved.scope,
        spore_material::BindingScope::PerMeshBlock { refs_per_mesh: 3 }
    );
    let material = &resolved.meshes[0];
    assert_eq!(material.material_id, DOCUMENTED_MATERIAL_ID);
    assert_eq!(material.name, None);
    assert_eq!(material.slots.len(), 3);
    assert_eq!(material.unknowns().count(), 6);

    // With a genuinely corrupt third record, the failure is still paired with
    // the reference it belongs to, in order.
    let corrupt = resolve_model_record(
        &documented_store_with_a_corrupt_third(),
        &documented_model_key(),
        RASTER_TYPE,
    )
    .unwrap();
    assert_eq!(corrupt.resolved_count(), 2);
    let failures = corrupt.failures();
    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0].0, reference(0x4063_2902));
    assert!(
        matches!(failures[0].1, MaterialError::Decode { .. }),
        "a corrupt record is a decode error: {:?}",
        failures[0].1
    );
}

#[test]
fn a_gmdl_record_whose_shader_id_has_no_documented_size_is_refused_by_name() {
    // Shader-data id 0x218 is in a gap of spore-gmdl's table, so the walk stops
    // inside the material info and there is no texture list to resolve at all.
    let store = store_over(&[(documented_model_key(), undocumented_shader_gmdl())]);
    let error = resolve_model_record(&store, &documented_model_key(), RASTER_TYPE).unwrap_err();
    assert_eq!(
        error,
        MaterialError::ModelDecode {
            key: documented_model_key(),
            source: spore_gmdl::GmdlError::UndocumentedShaderDataId { id: 0x218 },
        }
    );
    let text = error.to_string();
    assert!(text.contains("0x218"), "{text}");
    assert!(text.contains("gmdl:"), "{text}");

    // The model really does name no texture reference, so nothing was silently
    // dropped: the parse failed rather than returning an empty list.
    assert!(
        spore_gmdl::parse(&undocumented_shader_gmdl()).is_err(),
        "the refusal belongs to the gmdl layer, and this proves it"
    );
}

#[test]
fn a_non_gmdl_key_is_refused_before_any_read() {
    let store = documented_store();
    let key = ResourceKey::new(RASTER_TYPE, 0x4063_2900, DOCUMENTED_INSTANCE_ID);
    let error = resolve_model_record(&store, &key, RASTER_TYPE).unwrap_err();
    assert!(matches!(
        error,
        MaterialError::AssumedTypeNotDecodable { assumed_type, .. } if assumed_type == RASTER_TYPE
    ));
}

#[test]
fn a_wildcard_reference_is_refused_by_name() {
    let store = documented_store();
    for (reference, component) in [
        (
            GmdlTextureRef {
                instance_id: spore_core::WILDCARD,
                group_id: 0x4063_2900,
            },
            "instance",
        ),
        (
            GmdlTextureRef {
                instance_id: DOCUMENTED_INSTANCE_ID,
                group_id: spore_core::WILDCARD,
            },
            "group",
        ),
    ] {
        let error = resolve_texture(&store, reference, RASTER_TYPE).unwrap_err();
        assert_eq!(
            error,
            MaterialError::IncompleteReference {
                reference,
                component
            },
            "component: {component}"
        );
        assert_eq!(error.key(), None);
        assert!(error.to_string().contains("0xFFFFFFFF"));
        assert!(error.to_string().contains(component));
    }
}
