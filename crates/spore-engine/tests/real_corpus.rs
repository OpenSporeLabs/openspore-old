//! Real-corpus checks. `#[ignore]`d: they need the git-ignored `SPORE/` tree,
//! read only record bytes, and write nothing.
//!
//! Every number the engine's material policy depends on is asserted here rather
//! than quoted, so a change to `spore-material` that moves a role cannot reach
//! the renderer without one of these failing.
//!
//! Reproduce with:
//!
//! ```bash
//! cargo test -p spore-engine -- --ignored --nocapture
//! ```

use std::path::PathBuf;

use spore_assets::{ContentStore, ModelStore, Package};
use spore_core::ResourceKey;
use spore_engine::domain::materials::MaterialRoute;
use spore_material::{BindingScope, TextureRole};

const CREATURE: ResourceKey = ResourceKey::new(0x00E6_BCE5, 0x4062_7100, 0x067C_79D2);
const DOCUMENTED: ResourceKey = ResourceKey::new(0x00E6_BCE5, 0x4063_7E03, 0x067A_07F0);

/// Absolute, because `cargo test` runs with the crate directory as CWD.
fn content_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../SPORE/Data/Spore_Content.package")
}

fn store() -> Option<ContentStore> {
    let path = content_path();
    if !path.exists() {
        eprintln!("skipping: {} is not installed", path.display());
        return None;
    }
    let mut store = ContentStore::new();
    store.push(Package::open("Spore_Content", &path).unwrap());
    Some(store)
}

fn raster() -> u32 {
    spore_core::record::type_id::RASTER
}

#[test]
#[ignore = "needs SPORE/Data/Spore_Content.package; run with --ignored --nocapture"]
fn the_creature_record_carries_a_normal_map_and_the_even_cut_splits_it_across_two_meshes() {
    let Some(store) = store() else { return };

    // The whole engine path, not a shortcut: `spore-assets` decodes with the
    // RECOVERING parse, because this record's material-info tail names
    // shader-data id 0x218 and the strict parse refuses it.
    let loaded = ModelStore::new(&store).load(&CREATURE).unwrap();
    assert_eq!(loaded.meshes.len(), 2);
    assert_eq!(
        loaded
            .meshes
            .iter()
            .map(|m| m.indices.len() / 3)
            .sum::<usize>(),
        47016
    );
    assert!(loaded.stopped_at.is_some(), "the tail is not decoded");
    assert_eq!(loaded.gmdl.as_ref().unwrap().meshes.len(), 2);

    // `spore-material`'s strict entry point cannot answer for this record, and
    // says so by name.
    let strict = spore_material::resolve_model_record(&store, &CREATURE, raster());
    assert!(
        strict.is_err(),
        "the strict route accepts this record, so the recovering route is unnecessary"
    );
    eprintln!("strict route: {}", strict.unwrap_err());

    // The engine's route: the same public `spore-material` functions applied to
    // the recovering parse's model.
    let materials = spore_engine::domain::materials::ModelMaterials::resolve(
        &store,
        &CREATURE,
        &loaded,
        raster(),
    );
    assert_eq!(materials.route, MaterialRoute::Recovering);
    assert!(materials.strict_refusal.is_some());
    assert_eq!(materials.attempted(), 2, "the record names two references");
    assert_eq!(materials.resolved_count(), 2, "and both resolve");

    // The cut: two references over two meshes is one per mesh.
    assert_eq!(
        materials.scope,
        BindingScope::PerMeshBlock { refs_per_mesh: 1 }
    );
    assert!(materials.scope.is_inferred(), "there is no proven binding");
    assert_eq!(materials.meshes.len(), 2);
    assert_eq!(
        materials.roles(),
        vec![
            vec![TextureRole::LayeredAlphaMask],
            vec![TextureRole::TangentSpaceNormal],
        ]
    );

    // Record identity: group ids 0x40622900 (mask) and 0x40622901 (normal), both
    // at instance 0x067c79d2.
    assert_eq!(
        materials.meshes[0].slots[0],
        spore_gmdl::GmdlTextureRef {
            instance_id: 0x067C_79D2,
            group_id: 0x4062_2900
        },
    );
    assert_eq!(
        materials.meshes[1].slots[0],
        spore_gmdl::GmdlTextureRef {
            instance_id: 0x067C_79D2,
            group_id: 0x4062_2901
        },
    );

    // The content of each.
    let mask = materials.entry(0).unwrap().resolved.as_ref().unwrap();
    assert_eq!(mask.role, TextureRole::LayeredAlphaMask);
    assert_eq!(mask.layers.len(), 2, "two alpha planes");
    assert_eq!((mask.image.width, mask.image.height), (512, 512));
    assert_eq!(mask.mip_count, 10);
    let normal = materials.entry(1).unwrap().resolved.as_ref().unwrap();
    assert_eq!(normal.role, TextureRole::TangentSpaceNormal);
    assert_eq!(normal.layers.len(), 1);
    assert_eq!((normal.image.width, normal.image.height), (256, 256));

    // The evidence for "this is Z-first", read from the record's own statistics
    // rather than from the role.
    let means = [
        normal.statistics.mean(spore_material::Lane::Red).unwrap(),
        normal.statistics.mean(spore_material::Lane::Green).unwrap(),
        normal.statistics.mean(spore_material::Lane::Blue).unwrap(),
    ];
    eprintln!("creature normal-map lane means: {means:?}");
    assert!(
        means[0] >= spore_material::NORMAL_Z_MEAN_FLOOR,
        "lane 0 must be bright: {means:?}"
    );
    assert!(
        means[0] - means[1].max(means[2]) >= spore_material::NORMAL_Z_LEAD,
        "lane 0 must LEAD the lateral lanes: {means:?}"
    );

    // The swizzle, on the real first texel. File byte 0 is Z, so the output blue
    // byte must be the file's first byte and the output red byte must not be.
    let planes = materials.plans();
    assert_eq!(planes.len(), 2);
    assert!(planes[0].normal_map.is_none(), "mesh 0 is the mask");
    let normal_plan = planes[1]
        .normal_map
        .as_ref()
        .expect("mesh 1 is the normal map");
    assert_eq!(normal_plan.lanes, [1, 2, 0], "file byte 0 holds Z");
    assert!(normal_plan.lane_zero_is_z());
    let swizzled = spore_engine::domain::materials::swizzle_z_first_to_xyz(&normal.image.pixels);
    assert_eq!(swizzled.len(), normal.image.pixels.len());
    assert_eq!(swizzled[2], normal.image.pixels[0], "Z into blue");
    assert_eq!(swizzled[0], normal.image.pixels[1], "X into red");
    assert_eq!(swizzled[1], normal.image.pixels[2], "Y into green");
    eprintln!(
        "creature first texel, file order: {:?}",
        &normal.image.pixels[..4]
    );
    eprintln!("creature first texel, swizzled:    {:?}", &swizzled[..4]);

    // The disagreement this record carries. The even cut gives mesh 1's slot
    // derived ordinal 0 while the referenced record's group id low byte says 1.
    // Two independent statements, and they do not agree. Reported, not resolved.
    assert_eq!(planes[1].ordinal_conflicts, 1);
    assert_eq!(planes[0].ordinal_conflicts, 0);
}

#[test]
#[ignore = "needs SPORE/Data/Spore_Content.package; run with --ignored --nocapture"]
fn the_two_resolution_routes_agree_field_for_field_on_a_record_the_strict_one_accepts() {
    // The engine takes the strict route when it can. `spore-material` derives
    // the shape check and the ordinal cross-check in `resolve_model_record` and
    // does NOT expose them for the recovering route, so the engine can only
    // report those two on the strict route -- `observed_shape: None`,
    // `ordinal_agreement: None`. That difference is deliberate and is asserted
    // here so it cannot be mistaken for agreement.
    let Some(store) = store() else { return };
    let loaded = ModelStore::new(&store).load(&DOCUMENTED).unwrap();
    assert!(
        loaded.stopped_at.is_none(),
        "this record decodes completely"
    );

    let strict = spore_material::resolve_model_record(&store, &DOCUMENTED, raster())
        .expect("the documented asset parses strictly");
    let from_strict = spore_engine::domain::materials::ModelMaterials::from_model_textures(&strict);
    let engine = spore_engine::domain::materials::ModelMaterials::resolve(
        &store,
        &DOCUMENTED,
        &loaded,
        raster(),
    );

    assert_eq!(engine.route, MaterialRoute::Strict);
    assert_eq!(engine.strict_refusal, None);
    // The parts that must be identical.
    assert_eq!(engine.textures.len(), from_strict.textures.len());
    assert_eq!(engine.meshes, from_strict.meshes);
    assert_eq!(engine.scope, from_strict.scope);
    assert_eq!(engine.roles(), from_strict.roles());
    assert_eq!(engine.plans(), from_strict.plans());
    assert_eq!(engine.observed_shape, from_strict.observed_shape);
    assert_eq!(engine.ordinal_agreement, from_strict.ordinal_agreement);
    // And the roles themselves, field by field.
    for (a, b) in engine.textures.iter().zip(from_strict.textures.iter()) {
        assert_eq!(a.reference, b.reference);
        assert_eq!(
            a.resolved.as_ref().map(|t| t.role),
            b.resolved.as_ref().map(|t| t.role)
        );
        assert_eq!(a.resolved, b.resolved);
        assert_eq!(a.refusal, b.refusal);
    }
}

#[test]
#[ignore = "needs SPORE/Data/Spore_Content.package; run with --ignored --nocapture"]
fn the_documented_asset_is_mask_mask_normal_with_all_ordinals_agreeing() {
    let Some(store) = store() else { return };
    let resolved = spore_material::resolve_model_record(&store, &DOCUMENTED, raster()).unwrap();
    assert_eq!(resolved.meshes.len(), 1);
    assert_eq!(
        resolved.scope,
        BindingScope::PerMeshBlock { refs_per_mesh: 3 }
    );
    assert!(resolved.scope.is_inferred());
    assert_eq!(
        resolved.observed_shape,
        spore_material::TexSetShape::MaskMaskNormal
    );
    assert_eq!(resolved.ordinal_agreement.agreed, 3);
    assert_eq!(resolved.ordinal_agreement.compared, 3);
    assert_eq!(resolved.resolved_count(), 3);
    assert_eq!(
        resolved.roles(),
        vec![vec![
            TextureRole::LayeredAlphaMask,
            TextureRole::LayeredAlphaMask,
            TextureRole::TangentSpaceNormal,
        ]]
    );
    assert!(resolved.failures().is_empty());

    // The normal map's own channel bindings, which are what the engine's
    // swizzle is indexed from.
    let normal = resolved.textures[2].as_ref().unwrap();
    let roles: Vec<_> = normal.channels().iter().map(|c| c.role).collect();
    assert_eq!(
        roles,
        vec![
            spore_material::ChannelRole::NormalZ,
            spore_material::ChannelRole::NormalX,
            spore_material::ChannelRole::NormalY,
            spore_material::ChannelRole::UnidentifiedLane,
        ]
    );
    // The masks carry two alpha planes each and no colour: their colour planes
    // are `DegenerateColour` under the byte-pinned codec.
    for index in [0usize, 1] {
        let mask = resolved.textures[index].as_ref().unwrap();
        assert_eq!(mask.layer_count, 2);
        assert_eq!(
            mask.channels()
                .iter()
                .map(|c| c.role)
                .collect::<Vec<_>>()
                .len(),
            8
        );
        assert!(mask
            .channels()
            .iter()
            .all(|c| c.role == spore_material::ChannelRole::DegenerateColour
                || c.role == spore_material::ChannelRole::MaskChannel));
        assert_eq!(
            mask.statistics.distinct[spore_material::Lane::Green.index()],
            1,
            "the green span is 0 across the corpus"
        );
        assert_eq!(
            mask.statistics.distinct[spore_material::Lane::Blue.index()],
            1,
            "the blue span is 0 across the corpus"
        );
    }
}
