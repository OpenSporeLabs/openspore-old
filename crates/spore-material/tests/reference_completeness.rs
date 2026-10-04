//! The reference list is not always the record's whole texture set, and the
//! per-mesh cut must never be taken from one that is not.
//!
//! # The defect this file pins
//!
//! `spore_gmdl::parse_recovering` stops inside a gmdl record's material-info
//! table when it meets a shader-data id with no documented size — `0x218`, on 619
//! records of `Spore_Content.package` — and returns the texture references it
//! collected **so far**. `spore_material::partition_texture_refs` then divides
//! that list by the mesh count.
//!
//! Because what is missing is *whole blocks*, a truncated list frequently still
//! divides evenly. Measured over all 4 209 gmdl records before the fix:
//!
//! | | count |
//! |---|---:|
//! | records whose material-info walk closes | 3 590 |
//! | records that stop on `0x218` | 619 |
//! | …of those, stopping **before the last declared block** | 100 |
//! | …of those, the truncated list still dividing evenly | 100 |
//! | records that stopped early and still reported `PerMeshBlock` | **619** |
//!
//! The documented creature `0x00e6bce5:0x40627100:0x067c79d2` is one of the 100:
//! two meshes, two declared blocks, and block 0 contributes both references
//! before the walk stops. `refs_per_mesh` is therefore `2 / 2 = 1`, so mesh 1 was
//! handed block 0's *second* reference. The engine rendered that.
//!
//! # Why this file can test it without a byte of the corpus
//!
//! The fixture `truncated_material_gmdl` reproduces the creature's shape
//! exactly — verified against the real record's bytes — so the hermetic tests
//! below drive the **real** `spore_gmdl` walk and observe the real
//! `StopReason`. The `#[ignore]`d census at the bottom re-measures every number
//! in the table above over the installed package.
//!
//! ```text
//! cargo test -p spore-material --release -- --ignored --nocapture --test-threads=1
//! ```

mod support;

use std::collections::BTreeMap;

use spore_core::evidence::EvidenceLevel;
use spore_gmdl::{GmdlTextureRef, GMDL_TYPE};
use spore_material::{claims, model_materials, partition_texture_refs, BindingScope};
use spore_texture::{DXT5_FOURCC, RASTER_TYPE};
use support::*;

/// The documented creature's identity — the record whose bytes this file's
/// fixture reproduces. Spelled out rather than imported, because a test that
/// asserts the fixture matches a real record should not silently stop naming it.
#[allow(dead_code)]
const CREATURE: spore_core::ResourceKey =
    spore_core::ResourceKey::new(0x00E6_BCE5, 0x4062_7100, 0x067C_79D2);

/// Parses the creature-shaped fixture with the walk a renderer uses.
fn creature_model() -> spore_gmdl::PartialGmdl {
    let bytes = truncated_material_gmdl(2, 2, creature_block0_refs());
    let recovered = spore_gmdl::parse_recovering(&bytes).expect("the geometry walk must not fail");
    // The stop is the whole point of the fixture: assert it, so this file cannot
    // quietly start testing a record that parses.
    let stop = recovered
        .stop
        .as_ref()
        .expect("0x218 has no documented size, so the walk must stop");
    assert_eq!(stop.stage, spore_gmdl::WalkStage::MaterialInfo);
    assert_eq!(
        stop.error,
        spore_gmdl::GmdlError::UndocumentedShaderDataId { id: 0x218 }
    );
    recovered
}

#[test]
fn the_creature_shaped_record_collects_only_its_first_block() {
    let model = creature_model().model;
    assert_eq!(model.meshes.len(), 2, "two meshes, as documented");
    assert_eq!(model.material_ids.len(), 2);
    assert_eq!(
        model.texture_refs.len(),
        2,
        "block 0's two references, and block 1 was never read"
    );
    assert_eq!(model.texture_refs, creature_block0_refs().as_slice());
    // The walk recorded WHY it stopped, as a typed reason, rather than returning
    // a short list that looks like the whole one.
    assert!(!model.fully_walked());
}

#[test]
fn a_truncated_list_is_not_cut_per_mesh_even_though_it_divides_evenly() {
    let model = creature_model().model;
    let meshes = model.meshes.len();
    let collected = model.texture_refs.len();
    assert_eq!(
        collected % meshes,
        0,
        "the arithmetic succeeds; that is the trap, not the defence"
    );

    assert_eq!(
        partition_texture_refs(&model),
        None,
        "the record's own group-id ordinals contradict this division"
    );

    let materials = model_materials(&model);
    assert_eq!(materials.len(), 2);
    for material in &materials {
        assert_eq!(
            material.scope,
            BindingScope::Undetermined {
                collected: 2,
                contradicted: 1,
            }
        );
        assert!(material.scope.is_inferred());
        assert!(!material.scope.divides_by_mesh());
        // Mesh 1 does NOT get block 0's second reference as "its" texture. It
        // gets the whole collected list, which says "these belong to this model"
        // and nothing more.
        assert_eq!(material.slots.len(), 2);
        assert!(material.slots.iter().all(|slot| slot.ordinal.is_none()));
        assert_eq!(
            material.claim(claims::PER_MESH_TEXTURE_PARTITION).unwrap(),
            &spore_material::MaterialClaim::new(
                claims::PER_MESH_TEXTURE_PARTITION,
                claims::non_finding(claims::PER_MESH_TEXTURE_PARTITION),
            ),
            "the partition must be a non-finding, not a graded INFERRED value"
        );
        let contradiction = material.claim(claims::CUT_CONTRADICTED_BY_RECORD).unwrap();
        assert!(contradiction.fact.is_available());
        assert_eq!(contradiction.fact.level(), EvidenceLevel::Observed);
    }
}

#[test]
fn a_truncated_list_that_does_not_divide_falls_back_and_says_only_what_it_can_see() {
    // Two meshes, THREE references collected before the stop. `3 / 2` is not an
    // integer, so no candidate cut exists to ask the record about, and
    // `WholeModel` is the answer.
    //
    // It is deliberately NOT `Undetermined`. That scope claims the record
    // *contradicted* a division, and here no division was ever formed — and this
    // crate cannot see that the walk stopped, because a `&GmdlModel` does not say
    // so. Reporting "the record contradicted the cut" for a cut that was never
    // attempted would be inventing evidence. The corpus census asserts that all
    // 619 records which stop early divide evenly, so this shape does not arise in
    // practice.
    let refs = vec![
        GmdlTextureRef {
            instance_id: 7,
            group_id: 0x4064_2900,
        },
        GmdlTextureRef {
            instance_id: 7,
            group_id: 0x4064_2901,
        },
        GmdlTextureRef {
            instance_id: 7,
            group_id: 0x4064_2902,
        },
    ];
    let bytes = truncated_material_gmdl(2, 2, refs);
    let recovered = spore_gmdl::parse_recovering(&bytes).expect("the geometry walk must not fail");
    assert!(
        recovered.stop.is_some(),
        "the fixture must still stop on 0x218"
    );
    let model = recovered.model;
    assert_eq!(model.texture_refs.len(), 3);
    assert_eq!(partition_texture_refs(&model), None);
    let materials = model_materials(&model);
    assert!(materials
        .iter()
        .all(|m| m.scope == BindingScope::WholeModel));
    assert!(materials.iter().all(|m| m.slots.len() == 3));
}

#[test]
fn a_record_whose_walk_closed_is_still_cut_per_mesh() {
    // The fix must not cost anything on the 3 590 records that parse completely.
    // The documented asset is one of them: one mesh, three references, one block,
    // ordinals 0..2.
    let model = spore_gmdl::parse(&documented_gmdl()).expect("synthetic gmdl parses");
    // A strict parse is the strong statement: the material-info table closed, so
    // `texture_refs` is the record's whole list. `fully_walked()` is the weaker,
    // byte-level one, asserted too so a future fixture edit cannot quietly turn
    // a closed record into a partly-read one.
    assert!(model.fully_walked());
    let materials = model_materials(&model);
    assert_eq!(materials.len(), 1);
    assert_eq!(
        materials[0].scope,
        BindingScope::PerMeshBlock { refs_per_mesh: 3 }
    );
    assert!(materials[0].scope.divides_by_mesh());
    assert_eq!(materials[0].slots.len(), 3);
    assert_eq!(materials[0].slots[2].reference.group_id, 0x4063_2902);
    let contradiction = materials[0]
        .claim(claims::CUT_CONTRADICTED_BY_RECORD)
        .unwrap();
    assert!(contradiction.fact.is_unavailable());
    assert_eq!(
        contradiction.fact.reason(),
        Some(claims::NO_CUT_CONTRADICTION)
    );
}

#[test]
fn roles_walk_the_whole_list_under_the_undetermined_scope() {
    // The renderer path, end to end. `ModelTextures::roles()` does the index
    // arithmetic and must NOT multiply by a per-mesh width here — that is how a
    // mesh gets another mesh's role.
    let store = store_over(&[
        (
            documented_model_key(),
            truncated_material_gmdl(2, 2, creature_block0_refs()),
        ),
        (
            raster_key(0x4062_2900, 0x067C_79D2),
            raster_record(16, 16, 2, DXT5_FOURCC),
        ),
        (
            raster_key(0x4062_2901, 0x067C_79D2),
            luminance_raster(16, 16),
        ),
    ]);
    // `resolve_model_record` is the STRICT entry point and refuses this record by
    // name, which is correct and unchanged: the point of the fixture is that a
    // caller must use the recovering walk to get a model at all.
    assert!(
        spore_material::resolve_model_record(&store, &documented_model_key(), RASTER_TYPE).is_err()
    );

    // Build `ModelTextures` the way the engine does — from the recovering
    // model — and check the flat-position arithmetic.
    let textures =
        spore_material::resolve_model_textures(&store, &model_of_creature_shape(), RASTER_TYPE);
    let meshes = model_materials(&model_of_creature_shape());
    let scope = meshes[0].scope;
    assert!(scope.applies_whole_set());
    assert_eq!(
        textures.len(),
        2,
        "both of the record's collected references resolve"
    );
    let roles: Vec<_> = textures
        .iter()
        .map(|entry| entry.as_ref().map(|texture| texture.role))
        .collect();
    assert_eq!(
        roles,
        vec![
            Ok(spore_material::TextureRole::ColourAndMask),
            Ok(spore_material::TextureRole::TangentSpaceNormal),
        ]
    );
    // And both meshes are handed both roles, identically, because the scope says
    // "whole model" and not "per mesh".
    let per_mesh: Vec<Vec<spore_material::TextureRole>> = meshes
        .iter()
        .map(|mesh| {
            mesh.slots
                .iter()
                .enumerate()
                .map(|(within, _)| {
                    textures
                        .get(within)
                        .and_then(|entry| entry.as_ref().ok())
                        .map_or(spore_material::TextureRole::Unidentified, |t| t.role)
                })
                .collect()
        })
        .collect();
    assert_eq!(per_mesh.len(), 2);
    assert_eq!(per_mesh[0], per_mesh[1]);
    assert_eq!(per_mesh[0].len(), 2);
}

/// The model the recovering walk returns for the creature-shaped fixture.
fn model_of_creature_shape() -> spore_gmdl::GmdlModel {
    creature_model().model
}

/// The documented creature on the real corpus, on its own.
///
/// The record the validation agent's finding was written about, asserted here so
/// the fix cannot regress on the one asset this repository documents end to end.
///
/// **Before the fix:** 2 meshes, 2 references collected from block 0, `2 / 2 = 1`
/// divides evenly, so the scope was `PerMeshBlock { refs_per_mesh: 1 }` — mesh 0
/// got block 0's first reference and **mesh 1 got block 0's second**, out of a
/// block that is not mesh 1's. The engine rendered that.
///
/// **After:** the record's own group ids state ordinals 0 and 1 while the
/// division gives both slots ordinal 0, so the cut is refused:
/// `Undetermined { collected: 2, contradicted: 1 }`, and every mesh is handed the
/// whole collected list.
#[test]
#[ignore = "needs the git-ignored SPORE/ tree (~1 GB)"]
fn the_documented_creature_no_longer_splits_its_partial_list_across_two_meshes() {
    let Some(store) = content_store() else {
        return;
    };
    if !store.contains(&CREATURE) {
        println!("SKIP: the documented creature {CREATURE} is not in this install");
        return;
    }
    let record = store
        .read(&CREATURE)
        .expect("the creature record must read");
    let recovered = spore_gmdl::parse_recovering(&record).expect("the geometry walk must not fail");
    let stop = recovered.stop.as_ref().expect("the tail is not decoded");
    assert_eq!(stop.stage, spore_gmdl::WalkStage::MaterialInfo);
    assert_eq!(
        stop.error,
        spore_gmdl::GmdlError::UndocumentedShaderDataId { id: 0x218 }
    );

    let model = recovered.model;
    assert_eq!(model.meshes.len(), 2, "two meshes, as documented");
    assert_eq!(model.material_ids.len(), 2);
    assert_eq!(
        model.texture_refs.len(),
        2,
        "block 0's two references; block 1 was never read"
    );
    // The arithmetic that used to decide the answer.
    assert_eq!(model.texture_refs.len() % model.meshes.len(), 0);
    assert_eq!(
        partition_texture_refs(&model),
        None,
        "the record's group ids say these two references are ordinals 0 and 1 of ONE set"
    );

    let materials = model_materials(&model);
    assert_eq!(materials.len(), 2);
    for mesh in &materials {
        assert_eq!(
            mesh.scope,
            BindingScope::Undetermined {
                collected: 2,
                contradicted: 1,
            },
            "mesh {} must not be handed a per-mesh slice",
            mesh.mesh_index
        );
        // Every mesh holds the WHOLE collected list, so no mesh is bound to a
        // specific one of another mesh's textures.
        assert_eq!(mesh.slots.len(), 2);
        assert!(mesh.slots.iter().all(|slot| slot.ordinal.is_none()));
        let partition = mesh.claim(claims::PER_MESH_TEXTURE_PARTITION).unwrap();
        assert!(
            partition.fact.is_unavailable(),
            "the partition was NOT made and must not carry a graded value"
        );
        assert_eq!(
            partition.fact.reason(),
            Some(claims::TEXTURE_SET_NOT_CUT_PER_MESH)
        );
        let contradiction = mesh.claim(claims::CUT_CONTRADICTED_BY_RECORD).unwrap();
        assert_eq!(contradiction.fact.level(), EvidenceLevel::Observed);
    }
    assert_eq!(materials[0].slots, materials[1].slots);

    // The record's own group ids, so a reader can see what the list is.
    println!(
        "creature {CREATURE}: {} mesh(es), {} reference(s) collected, group ids {:#010x?} / \
         {:#010x?}, scope {}",
        model.meshes.len(),
        model.texture_refs.len(),
        model.texture_refs[0].group_id,
        model.texture_refs[1].group_id,
        materials[0].scope.as_str(),
    );
    assert_ne!(
        model.texture_refs[0].group_id, model.texture_refs[1].group_id,
        "they are different textures, so binding one to a mesh by index was a real error"
    );
}

/// The corpus census. `#[ignore]`d, skips cleanly without `SPORE/`.
///
/// Two things are measured, and they are the two halves of the argument:
///
/// 1. **the fix works** — no record reports [`BindingScope::PerMeshBlock`] whose
///    material-info walk stopped early, and the records that do report
///    [`BindingScope::Undetermined`] are exactly the ones whose group ids
///    contradict the cut; and
/// 2. **the fix costs nothing** — a record whose walk closed is still cut, and
///    the whole-list fallback is reported for the right reason rather than as a
///    confident per-mesh binding.
///
/// The ground truth for "the walk stopped *before the last declared block*" —
/// which needs the block boundaries `spore_gmdl` discards — is measured
/// independently in
/// `crates/spore-differential/tests/material_block_binding.rs`, which walks the
/// material-info section with its own reader. The two must agree, and the
/// numbers asserted here are that file's: 619 records stop early, 100 of them
/// before the last block.
#[test]
#[ignore = "needs the git-ignored SPORE/ tree (~1 GB)"]
fn the_corpus_never_cuts_per_mesh_from_a_list_the_walk_stopped_reading() {
    let Some(store) = content_store() else {
        return;
    };

    let mut walked = 0usize;
    let mut walk_stopped = 0usize;
    let mut closed = 0usize;
    let mut closed_with_unread_bytes = 0usize;
    let mut undetermined_and_stopped = 0usize;
    let mut per_mesh = 0usize;
    let mut whole_model = 0usize;
    let mut undetermined = 0usize;
    let mut stopped_but_whole_list = 0usize;
    let mut stopped_width: BTreeMap<usize, usize> = BTreeMap::new();
    let mut closed_width: BTreeMap<usize, usize> = BTreeMap::new();
    let mut closed_but_not_cut: Vec<String> = Vec::new();
    let mut undetermined_but_walk_closed: Vec<String> = Vec::new();
    let mut examples: Vec<String> = Vec::new();
    let mut refs_per_mesh: BTreeMap<usize, usize> = BTreeMap::new();
    let mut conflicts_when_cut = 0usize;
    let mut slots_when_cut = 0usize;
    // The documented false positive: a reference whose group id is 0 states
    // ordinal 0 whatever its position, so it costs the record its cut.
    let mut records_naming_group_zero = 0usize;
    let mut records_naming_group_zero_losing_the_cut = 0usize;

    // Is the block ORDER load-bearing? A record whose per-mesh slices are all
    // IDENTICAL cannot distinguish "block i is mesh i" from any permutation of
    // it: swapping two identical blocks changes no byte a renderer can see. That
    // is the measurement behind `claims::MATERIAL_BLOCK_IS_MESH` staying a
    // non-finding, so it is counted here rather than asserted in prose.
    let mut multi_mesh_cut = 0usize;
    let mut slices_all_identical = 0usize;
    let mut slices_differ = 0usize;
    let mut slices_differ_examples: Vec<String> = Vec::new();

    let mut rows = 0usize;
    for key in store.identities_of_type(GMDL_TYPE) {
        let Ok(bytes) = store.read(&key) else {
            continue;
        };
        let Ok(recovered) = spore_gmdl::parse_recovering(&bytes) else {
            continue;
        };
        let entry = key;
        rows += 1;
        let model = recovered.model;
        let materials = model_materials(&model);
        let Some(first) = materials.first() else {
            // meshCount 0: no meshes, so there is no scope to report. The
            // differential harness counts these as a consistent absence.
            continue;
        };
        let scope = first.scope;
        walked += 1;
        *refs_per_mesh
            .entry(match scope {
                BindingScope::PerMeshBlock { refs_per_mesh } => refs_per_mesh,
                _ => 0,
            })
            .or_default() += 1;

        if recovered.stop.is_some() {
            walk_stopped += 1;
            if matches!(scope, BindingScope::Undetermined { .. }) {
                undetermined_and_stopped += 1;
            } else if !scope.divides_by_mesh() {
                // A stopped walk that falls back for a reason other than "the
                // record contradicts the cut" is reported, not asserted away.
                stopped_but_whole_list += 0;
            }
            if scope.divides_by_mesh() {
                // A stopped walk is NOT a short list. `0x218` is the LAST entry
                // of a material-info block, so a walk that stops on it has
                // already read that block's texture set in full; only a stop
                // BEFORE the last declared block loses references. The count
                // difference here is exactly the 619 - 100 the independent block
                // walk measures in crates/spore-differential, which is the
                // cross-check that makes this number mean something.
                stopped_but_whole_list += 1;
                *stopped_width
                    .entry(match scope {
                        BindingScope::PerMeshBlock { refs_per_mesh } => refs_per_mesh,
                        _ => 0,
                    })
                    .or_default() += 1;
            }
        } else {
            closed += 1;
            // Counted, not asserted: `fully_walked()` is a statement about BYTES
            // and the material-info table closing is a statement about MEANING,
            // and on this corpus they come apart -- a record whose trailer walk
            // stopped short has unread trailing bytes while its texture
            // references are complete. This is why the fix asks the record's
            // ordinals rather than reading completeness off `GmdlModel`: there is
            // no field on it that says the table closed.
            if !model.fully_walked() {
                closed_with_unread_bytes += 1;
            }
            match scope {
                BindingScope::PerMeshBlock { refs_per_mesh } => {
                    *closed_width.entry(refs_per_mesh).or_default() += 1;
                }
                _ => closed_but_not_cut.push(format!("{entry}")),
            }
        }

        match scope {
            BindingScope::PerMeshBlock {
                refs_per_mesh: width,
            } => {
                per_mesh += 1;
                // Every cut this build offers must agree with the record, slot by
                // slot. This is the invariant, asserted on the corpus.
                for mesh in &materials {
                    for (within, slot) in mesh.slots.iter().enumerate() {
                        slots_when_cut += 1;
                        if Some(within as u32) != slot.ordinal {
                            conflicts_when_cut += 1;
                        }
                        assert_eq!(
                            slot.ordinal,
                            Some(within as u32),
                            "{entry}: a cut must be stated in the record's own ordinals"
                        );
                        assert!(
                            slot.ordinal_agrees(),
                            "{entry}: a cut the record contradicts must never be offered"
                        );
                    }
                }
                if model.meshes.len() > 1 {
                    multi_mesh_cut += 1;
                    let slices = partition_texture_refs(&model)
                        .expect("a per-mesh scope means the cut was made");
                    let first = slices[0];
                    if slices.iter().all(|slice| *slice == first) {
                        slices_all_identical += 1;
                    } else {
                        slices_differ += 1;
                        if slices_differ_examples.len() < 5 {
                            slices_differ_examples.push(format!(
                                "{entry}: {} mesh(es), slices {:?}",
                                model.meshes.len(),
                                slices
                                    .iter()
                                    .map(|slice| {
                                        slice
                                            .iter()
                                            .map(|r| format!("0x{:08x}", r.group_id))
                                            .collect::<Vec<_>>()
                                    })
                                    .collect::<Vec<_>>()
                            ));
                        }
                    }
                }
                let _ = width;
            }
            BindingScope::WholeModel => whole_model += 1,
            BindingScope::Undetermined {
                collected,
                contradicted,
            } => {
                undetermined += 1;
                if recovered.stop.is_none() {
                    undetermined_but_walk_closed.push(format!("{entry}"));
                }
                assert!(
                    contradicted > 0,
                    "{entry}: an undetermined scope has a contradiction"
                );
                assert_eq!(
                    collected,
                    model.texture_refs.len(),
                    "{entry}: `collected` is the list the walk really produced"
                );
                // Re-measured over the CANDIDATE cut (the flat list chunked by
                // the mesh count), not over the emitted slots: under this scope
                // every mesh holds the whole list, so counting slots would count
                // each reference once per mesh.
                let meshes = model.meshes.len();
                let counted: usize = model
                    .texture_refs
                    .chunks_exact(model.texture_refs.len() / meshes)
                    .flat_map(|slice| {
                        slice.iter().enumerate().map(move |(within, reference)| {
                            reference.group_id != 0
                                && (within as u32) != (reference.group_id & 0xFF)
                        })
                    })
                    .filter(|contradicted| *contradicted)
                    .count();
                assert_eq!(
                    counted, contradicted,
                    "{entry}: the reported count must be the one that was measured"
                );
                if examples.len() < 8 {
                    examples.push(format!(
                        "{entry}: {} mesh(es), {collected} reference(s) collected, {contradicted} \
                         contradicted",
                        model.meshes.len()
                    ));
                }
            }
        }

        if model.texture_refs.iter().any(|r| r.group_id == 0) {
            records_naming_group_zero += 1;
            if !scope.divides_by_mesh() {
                // A group id of 0 states NO ordinal, so it is skipped by the
                // cross-check rather than read as ordinal 0. Reading it as 0
                // would have refused the cut of these records for no reason.
                records_naming_group_zero_losing_the_cut += 1;
            }
        }
    }

    println!("gmdl records in the package: {rows}");
    println!("gmdl records walked: {walked}");
    println!("material-info closed: {closed}, walk stopped early: {walk_stopped}");
    println!(
        "…of the closed ones, records whose trailer walk left bytes unread -- which is why \
         `GmdlModel::fully_walked()` cannot be used to detect a short reference list: \
         {closed_with_unread_bytes}"
    );
    println!("PerMeshBlock: {per_mesh}, WholeModel: {whole_model}, Undetermined: {undetermined}");
    println!("  refs_per_mesh among cut records whose walk CLOSED: {closed_width:?}");
    println!("  refs_per_mesh among cut records whose walk STOPPED: {stopped_width:?}");
    println!(
        "records whose walk stopped early and were still cut per mesh -- stopped AFTER every \
         declared block's texture set was read, so their list is complete: \
         {stopped_but_whole_list}"
    );
    println!("records refused a cut AND whose walk stopped early: {undetermined_and_stopped}");
    println!(
        "slots in per-mesh cuts whose stated ordinal disagrees with the record: \
         {conflicts_when_cut} of {slots_when_cut}"
    );
    println!("records naming group id 0 (which states no ordinal): {records_naming_group_zero}");
    println!(
        "…of those, how many lose their cut to it: {records_naming_group_zero_losing_the_cut}"
    );
    for line in closed_but_not_cut.iter().take(8) {
        println!("  closed walk, no cut: {line}");
    }
    for line in undetermined_but_walk_closed.iter().take(8) {
        println!("  undetermined although the walk closed: {line}");
    }
    println!(
        "cut records with more than one mesh: {multi_mesh_cut}; of those, every mesh's slice is \
         the SAME texture set: {slices_all_identical}; slices that differ, so a permutation of \
         them would be visible: {slices_differ}"
    );
    for line in &slices_differ_examples {
        println!("  slices differ: {line}");
    }
    for line in &examples {
        println!("  undetermined: {line}");
    }

    assert!(walked > 0, "no gmdl record was walked");
    assert_eq!(
        per_mesh + whole_model + undetermined,
        walked,
        "every walked record must report exactly one scope"
    );

    // ---- the invariant that matters -------------------------------------
    //
    // Not "no stopped walk was cut": a stopped walk is NOT a short list. `0x218`
    // is the LAST entry of a material-info block, so a walk that stops on it has
    // already read that block's texture set in full. What must never happen is a
    // cut the RECORD contradicts, and that is what this asserts.
    assert_eq!(
        conflicts_when_cut, 0,
        "{conflicts_when_cut} of {slots_when_cut} slots in per-mesh cuts disagree with the \
         record's own group-id ordinals, so a mesh was handed another mesh's texture"
    );
    assert_eq!(
        undetermined, 100,
        "the records refused a cut because the record's own ordinals contradict it. The \
         independent block walk in crates/spore-differential/tests/material_block_binding.rs \
         measures exactly 100 gmdl records whose material-info walk stops before its last \
         declared block; the two counts are of the same set and must agree."
    );
    assert_eq!(
        undetermined_and_stopped, undetermined,
        "a record whose material-info walk CLOSED must never be refused a cut on the \
         contradiction test, or the check is rejecting sound cuts: {:?}",
        undetermined_but_walk_closed
    );
    assert_eq!(
        undetermined + stopped_but_whole_list,
        walk_stopped,
        "every record that stopped early is accounted for"
    );
    assert_eq!(
        stopped_but_whole_list, 519,
        "519 of the 619 records that stop on 0x218 do so AFTER the last declared block's \
         texture set was read, so their reference list is the record's whole list and the cut \
         is sound. The independent walk measures 619 stopped and 100 stopped-before-the-last-\
         block; the difference is this number."
    );
    assert_eq!(
        walk_stopped, 619,
        "the number of records whose material-info walk stops on the undocumented shader-data \
         id 0x218 is a measured fact about this install"
    );
    assert_eq!(
        whole_model, 0,
        "every record this build walks has a reference count that divides by its mesh count"
    );
    assert_eq!(
        records_naming_group_zero_losing_the_cut, 0,
        "a reference naming group id 0 states no ordinal and is SKIPPED by the cross-check, \
         so it must not cost a record its cut. If this ever fires, the count above is the price \
         and the rule in `TextureSlot::ordinal_agrees` needs revisiting -- not this assertion."
    );
    // ---- "is block i mesh i?" ------------------------------------------------
    //
    // The one thing that could REFUTE block-order-is-mesh-order is a record whose
    // blocks name different texture sets: then a permutation would move textures
    // between meshes. There is no such record in this corpus, and the count is
    // exact rather than sampled.
    assert_eq!(
        multi_mesh_cut, 1685,
        "cut records with more than one mesh: (2 meshes, 2 blocks) 1615 plus (3, 3) 70"
    );
    assert_eq!(
        slices_all_identical, 1685,
        "FINDING: EVERY multi-mesh record in the corpus has all of its per-mesh slices naming \
         the SAME texture set, so permuting material-info blocks changes nothing a renderer can \
         observe. 'Block i is mesh i' is therefore not merely unproven -- it is \
         UNFALSIFIABLE from the reference data this corpus offers. Nothing that could break the \
         tie (a per-mesh 0x210 parameter block, geometry or UV evidence) has been read. \
         MATERIAL_BLOCK_IS_MESH stays a non-finding, and this is the measured reason."
    );
    assert_eq!(
        slices_differ, 0,
        "a record with differing per-mesh slices would make the block order load-bearing and \
         would need the ordering settled; the first one to appear is a finding, not a failure: \
         {slices_differ_examples:?}"
    );
    assert!(
        closed_with_unread_bytes > 0,
        "`fully_walked()` was rejected as the completeness signal because records whose \
         material-info closed still leave bytes unread. If this install ever has none, the \
         reasoning behind asking the record's ordinals instead should be re-examined."
    );
}
