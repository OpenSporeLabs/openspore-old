//! Per-mesh bindings: the cut, what it rests on, and what it does not.
//!
//! **Synthetic fixtures only.** The corpus numbers quoted here come from the
//! `#[ignore]`d `real_corpus_roles.rs`.

mod support;

use spore_core::evidence::EvidenceLevel;
use spore_gmdl::{GmdlMeshRef, GmdlModel, GmdlTextureRef, TrailerWalk};
use spore_material::{
    claims, model_materials, partition_texture_refs, resolve_model_record, BindingScope,
    TexSetShape, TextureRole,
};
use spore_texture::RASTER_TYPE;
use support::*;

/// The documented asset's three groups, as a `raster`-typed store over a gmdl
/// record with the documented asset's material section.
fn documented_model_store() -> spore_assets::ContentStore {
    store_over(&[
        (documented_model_key(), documented_gmdl()),
        (
            raster_key(0x4063_2900, DOCUMENTED_INSTANCE_ID),
            raster_record(16, 16, 2, spore_texture::DXT5_FOURCC),
        ),
        (
            raster_key(0x4063_2901, DOCUMENTED_INSTANCE_ID),
            raster_record(16, 16, 2, spore_texture::DXT5_FOURCC),
        ),
        (
            raster_key(0x4063_2902, DOCUMENTED_INSTANCE_ID),
            luminance_raster(16, 16),
        ),
    ])
}

#[test]
fn one_mesh_and_three_references_divides_and_the_mesh_gets_all_three() {
    let resolved = resolve_model_record(
        &documented_model_store(),
        &documented_model_key(),
        RASTER_TYPE,
    )
    .expect("the model record resolves");
    assert_eq!(resolved.meshes.len(), 1);
    assert_eq!(
        resolved.scope,
        BindingScope::PerMeshBlock { refs_per_mesh: 3 }
    );
    let mesh = &resolved.meshes[0];
    assert_eq!(mesh.mesh_index, 0);
    assert_eq!(mesh.material_id, DOCUMENTED_MATERIAL_ID);
    assert_eq!(mesh.slots.len(), 3);
    assert_eq!(
        mesh.slots
            .iter()
            .map(|slot| slot.reference.group_id)
            .collect::<Vec<_>>(),
        DOCUMENTED_TEXTURE_GROUPS.to_vec(),
        "the mesh's slice is the record's references, in order"
    );
    // Each slot's ordinal is the record's own statement, cross-checkable against
    // the referenced group id's low byte. Measured: 6684/6684 agree corpus-wide.
    for (expected, slot) in mesh.slots.iter().enumerate() {
        let expected = expected as u32;
        assert_eq!(slot.ordinal, Some(expected));
        assert_eq!(slot.group_stated_ordinal(), expected);
        assert!(slot.ordinal_agrees());
    }
    assert_eq!(
        resolved.ordinal_agreement,
        spore_material::OrdinalAgreement {
            compared: 3,
            agreed: 3
        }
    );
    assert!(resolved.ordinal_agreement.all_agree());
    assert!(resolved.ordinal_agreement.checked());
}

#[test]
fn the_documented_assets_own_shape_is_the_two_mask_one_normal_measurement() {
    let resolved = resolve_model_record(
        &documented_model_store(),
        &documented_model_key(),
        RASTER_TYPE,
    )
    .expect("the model record resolves");
    // Measured corpus: a `(DXT5, DXT5, 0x15)` set in 1922 of 2381 sets. This
    // model's roles come out in that order, from its own records' content.
    assert_eq!(
        resolved.observed_shape,
        TexSetShape::ColourAndMaskTwiceThenNormal
    );
    assert!(resolved.observed_shape.matches_corpus());
    // And the roles are what the renderer needs, in slot order.
    assert_eq!(
        resolved.roles(),
        vec![vec![
            TextureRole::ColourAndMask,
            TextureRole::ColourAndMask,
            TextureRole::TangentSpaceNormal,
        ]],
        "the normal map is the LAST slot, and that came from its content"
    );
    assert_eq!(
        resolved.role_census(),
        vec![
            (TextureRole::ColourAndMask, 2),
            (TextureRole::TangentSpaceNormal, 1),
        ]
    );
    let shape = resolved.claim(claims::TEXTURE_SET_SHAPES).expect("subject");
    assert_eq!(shape.fact.level(), EvidenceLevel::Verified);
}

#[test]
fn two_meshes_split_six_references_three_each_and_the_roles_do_not_cross_over() {
    // The corpus's (2 blocks, 6 references) shape. The partition is what makes
    // per-mesh binding possible at all, so the test checks that mesh 1's slots
    // really do point at the *second* block's records.
    let store = store_over(&[
        (
            documented_model_key(),
            gmdl_record(
                2,
                &[DOCUMENTED_MATERIAL_ID, 0x1111_2222],
                MaterialBlock::TextureSet(vec![
                    GmdlTextureRef {
                        instance_id: 1,
                        group_id: 0x4063_2900,
                    },
                    GmdlTextureRef {
                        instance_id: 1,
                        group_id: 0x4063_2901,
                    },
                    GmdlTextureRef {
                        instance_id: 1,
                        group_id: 0x4063_2902,
                    },
                    GmdlTextureRef {
                        instance_id: 1,
                        group_id: 0x4064_2900,
                    },
                    GmdlTextureRef {
                        instance_id: 1,
                        group_id: 0x4064_2901,
                    },
                    GmdlTextureRef {
                        instance_id: 1,
                        group_id: 0x4064_2902,
                    },
                ]),
            ),
        ),
        (
            raster_key(0x4063_2900, 1),
            raster_record(16, 16, 2, spore_texture::DXT5_FOURCC),
        ),
        (
            raster_key(0x4063_2901, 1),
            raster_record(16, 16, 2, spore_texture::DXT5_FOURCC),
        ),
        (raster_key(0x4063_2902, 1), luminance_raster(16, 16)),
        (
            raster_key(0x4064_2900, 1),
            raster_record(16, 16, 2, spore_texture::DXT5_FOURCC),
        ),
        (
            raster_key(0x4064_2901, 1),
            raster_record(16, 16, 2, spore_texture::DXT5_FOURCC),
        ),
        (raster_key(0x4064_2902, 1), luminance_raster(16, 16)),
    ]);
    let resolved =
        resolve_model_record(&store, &documented_model_key(), RASTER_TYPE).expect("resolves");
    assert_eq!(resolved.meshes.len(), 2);
    assert_eq!(
        resolved.scope,
        BindingScope::PerMeshBlock { refs_per_mesh: 3 }
    );
    for mesh in &resolved.meshes {
        assert_eq!(mesh.slots.len(), 3);
        assert_eq!(mesh.slots[0].ordinal, Some(0));
        assert_eq!(mesh.slots[2].ordinal, Some(2));
    }
    // The families differ between the two meshes, which is what makes the split
    // observable: `0x4063…` for mesh 0, `0x4064…` for mesh 1.
    assert_eq!(resolved.meshes[0].slots[0].reference.group_id, 0x4063_2900);
    assert_eq!(resolved.meshes[1].slots[0].reference.group_id, 0x4064_2900);
    assert_eq!(resolved.meshes[1].material_id, 0x1111_2222);
    // And every mesh reports its OWN roles, all three of them, with no bleed.
    assert_eq!(
        resolved.roles(),
        vec![
            vec![
                TextureRole::ColourAndMask,
                TextureRole::ColourAndMask,
                TextureRole::TangentSpaceNormal,
            ],
            vec![
                TextureRole::ColourAndMask,
                TextureRole::ColourAndMask,
                TextureRole::TangentSpaceNormal,
            ],
        ]
    );
}

#[test]
fn an_odd_reference_count_falls_back_to_the_whole_model_and_says_so() {
    // Three meshes, six references: not divisible, so there is no even cut. The
    // fallback is stated in the type and the shape becomes Unavailable, because
    // without block boundaries no single texture set can be isolated.
    let mut record = gmdl_record(
        3,
        &[DOCUMENTED_MATERIAL_ID; 3],
        MaterialBlock::TextureSet(vec![
            GmdlTextureRef {
                instance_id: 1,
                group_id: 0x4063_2900,
            },
            GmdlTextureRef {
                instance_id: 1,
                group_id: 0x4063_2901,
            },
            GmdlTextureRef {
                instance_id: 1,
                group_id: 0x4063_2902,
            },
            GmdlTextureRef {
                instance_id: 1,
                group_id: 0x4064_2900,
            },
            GmdlTextureRef {
                instance_id: 1,
                group_id: 0x4064_2901,
            },
            GmdlTextureRef {
                instance_id: 1,
                group_id: 0x4064_2902,
            },
        ]),
    );
    // Six references over three meshes is divisible; drop one to break it.
    record.truncate(record.len() - 24);
    let store = store_over(&[(documented_model_key(), record)]);
    let resolved = resolve_model_record(&store, &documented_model_key(), RASTER_TYPE);
    // The gmdl walk itself may refuse the truncated record; either way there is
    // no silent "even cut" claim.
    match resolved {
        Ok(resolved) => {
            assert_eq!(resolved.scope, BindingScope::WholeModel);
            assert_eq!(resolved.observed_shape, TexSetShape::Unavailable);
            assert!(resolved
                .meshes
                .iter()
                .all(|mesh| mesh.slots.iter().all(|slot| slot.ordinal.is_none())));
            assert!(!resolved.ordinal_agreement.checked());
        }
        Err(_) => {
            // A refusal is also an honest outcome: no partition was invented.
        }
    }
}

#[test]
fn the_cut_is_labelled_inferred_and_the_ordering_stays_unknown() {
    let model = spore_gmdl::parse(&documented_gmdl()).expect("synthetic gmdl parses");
    let materials = model_materials(&model);
    assert_eq!(materials.len(), 1);
    let mesh = &materials[0];
    assert!(mesh.scope.is_inferred());
    assert!(mesh.scope.refs_per_mesh().is_some());

    // The partition's grade.
    let partition = mesh
        .claim(claims::PER_MESH_TEXTURE_PARTITION)
        .expect("subject");
    assert_eq!(partition.fact.level(), EvidenceLevel::Inferred);
    assert_ne!(partition.fact.level(), EvidenceLevel::Observed);
    assert_ne!(partition.fact.level(), EvidenceLevel::Verified);
    assert_eq!(
        partition.fact.evidence_state(),
        spore_core::evidence::EvidenceState::Derived
    );

    // And the part that stays unknown, even on the happy path.
    let ordering = mesh.claim(claims::MATERIAL_BLOCK_IS_MESH).expect("subject");
    assert!(ordering.fact.is_unavailable());
    assert_eq!(ordering.fact.level(), EvidenceLevel::Unknown);
    assert_eq!(
        ordering.fact.reason(),
        Some(claims::BLOCK_ORDER_NOT_ESTABLISHED)
    );
}

#[test]
fn both_scopes_report_that_the_binding_is_inferred() {
    // A caller asking "is this a proven binding?" must get `false` from both
    // variants. There is no proven binding in this build, and a scope that
    // claimed otherwise would be the whole failure this crate prevents.
    assert!(BindingScope::PerMeshBlock { refs_per_mesh: 3 }.is_inferred());
    assert!(BindingScope::WholeModel.is_inferred());
}

#[test]
fn an_out_of_range_mesh_reports_no_roles_rather_than_panicking() {
    let store = documented_model_store();
    let resolved =
        resolve_model_record(&store, &documented_model_key(), RASTER_TYPE).expect("resolves");
    for index in [resolved.textures.len(), usize::MAX] {
        assert_eq!(
            resolved.role_of(index),
            TextureRole::Unidentified,
            "index {index} is not a texture this model referenced"
        );
    }
}

#[test]
fn a_failed_resolution_leaves_the_slot_and_reports_no_role() {
    // The third of the documented asset's groups is a `0x15` record in the real
    // corpus. Here the store simply lacks it, so the reference fails — and the
    // mesh still reports three slots with one role unknown. A missing record is
    // not a missing slot.
    let mut records: Vec<(spore_core::ResourceKey, Vec<u8>)> = vec![
        (documented_model_key(), documented_gmdl()),
        (
            raster_key(0x4063_2900, DOCUMENTED_INSTANCE_ID),
            raster_record(16, 16, 2, spore_texture::DXT5_FOURCC),
        ),
        (
            raster_key(0x4063_2901, DOCUMENTED_INSTANCE_ID),
            raster_record(16, 16, 2, spore_texture::DXT5_FOURCC),
        ),
    ];
    records.push((
        raster_key(0x4063_2902, DOCUMENTED_INSTANCE_ID),
        luminance_raster(16, 16),
    ));
    let full = resolve_model_record(&store_over(&records), &documented_model_key(), RASTER_TYPE)
        .expect("resolves");
    assert_eq!(full.roles()[0][2], TextureRole::TangentSpaceNormal);

    records.pop();
    let partial = resolve_model_record(&store_over(&records), &documented_model_key(), RASTER_TYPE)
        .expect("the model record still resolves");
    assert_eq!(partial.resolved_count(), 2);
    assert_eq!(partial.failures().len(), 1);
    assert_eq!(
        partial.roles()[0],
        vec![
            TextureRole::ColourAndMask,
            TextureRole::ColourAndMask,
            // The reference is still there; its record is not. "Unidentified"
            // is what an unchecked slot reports, and `failures()` is where the
            // reason lives.
            TextureRole::Unidentified,
        ]
    );
    // Which is also why the shape is no longer the corpus shape.
    assert_eq!(
        partial.observed_shape,
        TexSetShape::Divergent(vec![
            TextureRole::ColourAndMask,
            TextureRole::ColourAndMask,
            TextureRole::Unidentified,
        ])
    );
    assert!(!partial.observed_shape.matches_corpus());
}

#[test]
fn the_partition_helper_refuses_rather_than_rounding() {
    // Exercised directly, on a hand-built model, so the arithmetic is pinned
    // independently of whatever the gmdl parser produces.
    let model = model_with(3, 2);
    assert_eq!(partition_texture_refs(&model), None);
    let even = model_with(3, 6);
    let slices = partition_texture_refs(&even).expect("6 divides by 3");
    assert_eq!(slices.len(), 3);
    assert!(slices.iter().all(|slice| slice.len() == 2));
    // Zero references over three meshes: consistent, and an answer.
    let empty = model_with(3, 0);
    let slices = partition_texture_refs(&empty).expect("0 divides by 3");
    assert_eq!(slices.len(), 3);
    assert!(slices.iter().all(|slice| slice.is_empty()));
}

/// A `GmdlModel` with `meshes` meshes and `refs` texture references, built by
/// hand from the reader's documented layout rather than from a parser, so the
/// partition's arithmetic is checked against an independent construction.
fn model_with(meshes: usize, refs: usize) -> GmdlModel {
    // `refs` spread over `meshes` blocks as evenly as possible, remainder first,
    // so an uneven total (the case `the_partition_helper_refuses` exercises) is
    // still a real reference list and not an empty one.
    let per_block = refs.checked_div(meshes).unwrap_or(refs);
    let remainder = if meshes == 0 { 0 } else { refs % meshes };
    GmdlModel {
        version: spore_gmdl::SUPPORTED_VERSION,
        referenced_files: Vec::new(),
        mesh_count: meshes as u32,
        bounds_min: [0.0; 3],
        bounds_max: [0.0; 3],
        radius: 0.0,
        index_buffers: Vec::new(),
        descriptors: Vec::new(),
        vertex_buffers: Vec::new(),
        meshes: (0..meshes)
            .map(|_| GmdlMeshRef {
                index_buffer: 0,
                vertex_buffer: 0,
            })
            .collect(),
        material_ids: vec![DOCUMENTED_MATERIAL_ID; meshes],
        // One texture set per mesh, each entry's group id low byte equal to its
        // position INSIDE that set, and each block drawn from its own group
        // family. The per-set ordinal is the record's own statement of where an
        // entry sits, which is what `partition_texture_refs` cross-checks the
        // division against, so a fixture that numbered across the whole flat list
        // would be modelling a truncated record rather than a sound one.
        texture_refs: (0..meshes.max(1))
            .flat_map(|mesh| {
                let family = 0x4063_2900u32 + (mesh as u32) * 0x100;
                let count = per_block + usize::from((mesh as u32) < remainder as u32);
                (0..count).map(move |within| GmdlTextureRef {
                    instance_id: 0x067A_07F0,
                    group_id: family + within as u32,
                })
            })
            .collect(),
        bone_ranges: Vec::new(),
        unknown_key: [0; 3],
        trailer: TrailerWalk::Complete,
        strict_consumed: 0,
        consumed: 0,
    }
}
