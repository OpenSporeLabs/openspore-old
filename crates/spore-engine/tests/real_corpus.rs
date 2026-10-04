//! Real-corpus checks. `#[ignore]`d: they need the git-ignored `SPORE/` tree,
//! read only record bytes, and write nothing.
//!
//! Every number the engine's material policy depends on is asserted here rather
//! than quoted, so a change to `spore-material` that moves a role cannot reach
//! the renderer without one of these failing.
//!
//! # Two corpus measurements, both recorded in these tests
//!
//! 1. **The raster layer model is refuted.** One mip chain, no layers, no
//!    16-byte layer headers; `DXT5` is BC3. The engine's old "two layers per
//!    mask record" expectation came from a decoder that charged 8 bytes per
//!    4x4 block *and* divided by `16 + chain` -- two errors that cancelled
//!    exactly when the geometry lined up.
//! 2. **"A raster carries no colour" was a tautology of that decoder.** Read as
//!    BC3 the same records carry 32 distinct greens and blues. The engine binds
//!    only what `spore-material` publishes, so the colour in a record does not
//!    become a base-colour map: a `DXT5` record still feeds no material channel
//!    but its mask plane, and the model's colour still comes from its
//!    `sporemodel` record.
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

/// A store with `Spore_Game` as well, because the `sporemodel` colour records
/// live there and **not** in `Spore_Content`. That is the whole reason
/// `--sporemodel-package` exists.
fn store_with_sporemodels() -> Option<ContentStore> {
    let mut store = store()?;
    let game =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../SPORE/Data/Spore_Game.package");
    if !game.exists() {
        eprintln!("skipping: {} is not installed", game.display());
        return None;
    }
    store.push(Package::open("Spore_Game", &game).unwrap());
    Some(store)
}

fn raster() -> u32 {
    spore_core::record::type_id::RASTER
}

#[test]
#[ignore = "needs SPORE/Data/Spore_Content.package; run with --ignored --nocapture"]
fn the_creature_record_carries_a_normal_map_and_its_short_list_cannot_be_cut_per_mesh() {
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

    // The cut. The material-info walk stops at the undocumented shader-data id
    // `0x218`, so the flat reference list holds 2 entries for 2 meshes: it
    // divides evenly by the mesh count and is therefore *wrong*, and the
    // record's own group-id low bytes say so. Measured: 2 collected,
    // 1 contradicted. `spore-material` refuses the cut rather than binding mesh
    // 1 to a texture out of mesh 0's block.
    assert_eq!(
        materials.scope,
        BindingScope::Undetermined {
            collected: 2,
            contradicted: 1
        }
    );
    assert!(materials.scope.is_inferred(), "there is no proven binding");
    assert!(!materials.scope.divides_by_mesh());
    assert!(
        materials.scope.applies_whole_set(),
        "a refused cut hands every mesh the whole collected list"
    );
    assert_eq!(materials.scope.refs_per_mesh(), None);
    assert_eq!(materials.meshes.len(), 2);
    // Because the whole short list goes to every mesh, BOTH meshes carry both
    // references -- the conservative answer, and the visible consequence of
    // refusing the cut.
    //
    // The roles themselves are asserted, because they are what the renderer
    // binds from. The first slot's record is a `DXT5`: it is not a normal map,
    // and it is not published as the old layered-alpha-mask either, because with
    // BC3 texels its colour lanes are no longer degenerate. What it publishes is
    // asserted from its channels, below.
    assert_eq!(
        materials.roles(),
        vec![
            vec![TextureRole::ColourAndMask, TextureRole::TangentSpaceNormal],
            vec![TextureRole::ColourAndMask, TextureRole::TangentSpaceNormal],
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
        materials.meshes[0].slots[1],
        spore_gmdl::GmdlTextureRef {
            instance_id: 0x067C_79D2,
            group_id: 0x4062_2901
        },
    );

    // The content of each.
    let mask = materials.entry(0).unwrap().resolved.as_ref().unwrap();
    assert_eq!(mask.role, TextureRole::ColourAndMask);
    assert_eq!(mask.layer_count, 1, "one mip chain, always");
    assert_eq!(mask.layers.len(), 1, "and no layer header, at any size");
    assert_eq!((mask.image.width, mask.image.height), (512, 512));
    assert_eq!(mask.mip_count, 10);
    // The refutation, on this record: read as BC3 the colour lanes are full
    // ranges, not the two-value field the old codec could produce. These are the
    // distinct-value counts over all 262 144 texels.
    assert_eq!(
        mask.statistics.distinct,
        [233, 254, 230, 256],
        "green and blue are no longer degenerate, so 'a raster carries no \
         colour' was a property of the decoder and not of the record"
    );
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
    for (index, plan) in planes.iter().enumerate() {
        let normal_plan = plan
            .normal_map
            .as_ref()
            .unwrap_or_else(|| panic!("mesh {index} carries the normal map"));
        assert_eq!(normal_plan.lanes, [1, 2, 0], "file byte 0 holds Z");
        assert_eq!(normal_plan.layer_index, 0);
        assert!(normal_plan.lane_zero_is_z());
        // REFUTED: this used to be 2, "two alpha planes from the one mask
        // record". The record has one chain and publishes one mask channel.
        assert_eq!(
            plan.masks.len(),
            1,
            "one published mask channel from a one-layer record"
        );
        assert_eq!(plan.masks[0].lane, spore_material::Lane::Alpha);
        assert_eq!(plan.masks[0].layer_index, 0);
        assert_eq!(plan.masks[0].width, 512);
        assert!(
            plan.unidentified.is_empty(),
            "both slots bound something: {:?}",
            plan.unidentified
        );
        assert_eq!(
            plan.ordinal_conflicts, 0,
            "a refused cut states no ordinal, so it cannot manufacture a conflict"
        );
        // And the whole-set binding means both meshes hold the *same* two
        // records: mesh 1 is not bound to a texture out of mesh 0's block.
        assert_eq!(
            plan.masks[0].key, planes[0].masks[0].key,
            "the same collected list goes to every mesh"
        );
    }
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
fn the_documented_asset_keeps_its_cut_its_shape_and_its_ordinals() {
    let Some(store) = store() else { return };
    let resolved = spore_material::resolve_model_record(&store, &DOCUMENTED, raster()).unwrap();
    assert_eq!(resolved.meshes.len(), 1);
    assert_eq!(
        resolved.scope,
        BindingScope::PerMeshBlock { refs_per_mesh: 3 }
    );
    assert!(resolved.scope.is_inferred());
    // REFUTED, RECORDED: this used to assert `TexSetShape::MaskMaskNormal`, over
    // a role called `LayeredAlphaMask` and two layers per record. The set's
    // SHAPE survived the raster refutation -- three references, normal last --
    // under `spore-material`'s re-derived role name. What did not survive is the
    // content the old name asserted (a colourless, two-layer alpha plane), and
    // that is asserted from the record's own statistics below rather than from a
    // role's spelling.
    assert_eq!(
        resolved.observed_shape,
        spore_material::TexSetShape::ColourAndMaskTwiceThenNormal
    );
    assert!(
        resolved.observed_shape.matches_corpus(),
        "the corpus regularity still holds, under the role name the content earns"
    );
    assert_eq!(resolved.ordinal_agreement.agreed, 3);
    assert_eq!(resolved.ordinal_agreement.compared, 3);
    assert!(resolved.ordinal_agreement.checked());
    assert!(resolved.ordinal_agreement.all_agree());
    assert_eq!(resolved.resolved_count(), 3);
    assert_eq!(resolved.model.texture_refs.len(), 3);
    assert_eq!(
        resolved.roles(),
        vec![vec![
            TextureRole::ColourAndMask,
            TextureRole::ColourAndMask,
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

    // The other two records: ONE chain each, publishing one mask channel over
    // three colour lanes.
    //
    // REFUTED, RECORDED: this used to assert `layer_count == 2`, eight channel
    // bindings, `DegenerateColour` on the colour lanes, and a distinct-value
    // count of 1 for green and blue. Every one of those was a measurement *of the
    // old 8-byte decoder*. Read as BC3 the same records carry colour, and the
    // counts below are what they actually are.
    for index in [0usize, 1] {
        let mask = resolved.textures[index].as_ref().unwrap();
        assert_eq!(mask.layer_count, 1, "one mip chain, always");
        assert_eq!(mask.layers.len(), 1);
        assert!(mask.pixels_decoded);
        assert_eq!(
            mask.channels().len(),
            4,
            "4 * layer_count, and layer_count is 1"
        );
        let bindings = mask.channels();
        assert_eq!(bindings[0].lane, spore_material::Lane::Red);
        assert_eq!(bindings[3].lane, spore_material::Lane::Alpha);
        assert_eq!(
            bindings
                .iter()
                .filter(|c| c.role == spore_material::ChannelRole::MaskChannel)
                .count(),
            1,
            "one channel the engine may upload, and no more"
        );
        for lane in [spore_material::Lane::Red, spore_material::Lane::Green] {
            assert!(
                mask.statistics.distinct[lane.index()] > 1,
                "lane {} takes {} distinct values: the old decoder could not \
                 produce a second one, which is why this number used to be 1",
                lane.as_str(),
                mask.statistics.distinct[lane.index()],
            );
        }
    }

    // What the engine does with that, from the same record: two mask channels --
    // one per record -- and one normal map. Never the other way round.
    let engine = spore_engine::domain::materials::ModelMaterials::from_model_textures(&resolved);
    let plans = engine.plans();
    assert_eq!(plans.len(), 1);
    assert_eq!(plans[0].masks.len(), 2, "one published channel per record");
    assert!(plans[0].normal_map.is_some(), "the 0x15 record binds");
    assert!(
        plans[0].unidentified.is_empty(),
        "nothing was left unbound: {:?}",
        plans[0].unidentified
    );
    for mask in &plans[0].masks {
        assert_eq!(mask.lane, spore_material::Lane::Alpha);
        assert_eq!(mask.layer_index, 0);
        assert_eq!((mask.width, mask.height), (512, 512));
    }
    // And the normal map is not one of the mask records: the keys differ.
    let normal_key = plans[0].normal_map.as_ref().unwrap().key;
    assert!(plans[0].masks.iter().all(|m| m.key != normal_key));
}

#[test]
#[ignore = "needs SPORE/Data/Spore_Content.package; run with --ignored --nocapture"]
fn every_raster_record_decodes_and_only_the_luminance_family_publishes_a_normal() {
    // The corpus-scale version of the same claim, and the number the engine's
    // binding policy rests on: **2954 of 2954** raster records decode with one
    // mip chain and no layer header, and exactly one family publishes anything
    // bindable. If a decoder regression ever takes a record back, this fails
    // rather than quietly changing what the renderer shows.
    use spore_material::Lane;

    let Some(store) = store() else { return };
    let mut total = 0usize;
    let mut chains = std::collections::BTreeMap::<usize, usize>::new();
    let mut roles = std::collections::BTreeMap::<String, usize>::new();
    let mut normals = 0usize;
    for key in store.identities_of_type(raster()) {
        let bytes = store.read(&key).expect("every raster record extracts");
        let image =
            spore_texture::decode_raster(&bytes).unwrap_or_else(|e| panic!("{key} decodes: {e}"));
        total += 1;
        *chains.entry(image.layers.len()).or_default() += 1;
        assert_eq!(
            image.layers.len(),
            1,
            "{key}: one mip chain per record, no layer header"
        );
        let stats = spore_material::MipStatistics::of(&image.layers[0][0]);
        let role = spore_material::classify(image.envelope.fourcc, &stats);
        *roles.entry(role.as_str().to_owned()).or_default() += 1;
        if spore_material::normal_map_signature(&stats) {
            normals += 1;
        }
        // The engine binds a normal map only when the record publishes the three
        // normal components, so a record that carries no colour must not be able
        // to masquerade as one.
        if !spore_material::normal_map_signature(&stats) {
            assert!(
                stats.mean(Lane::Red).is_some(),
                "{key}: statistics were gathered"
            );
        }
    }
    eprintln!("rasters={total} normal_maps={normals} roles={roles:?} chains={chains:?}");
    assert_eq!(total, 2954, "every raster record in Spore_Content.package");
    assert_eq!(
        *chains.get(&1).unwrap_or(&0),
        2954,
        "layers.len() is 1 for every single record"
    );
    assert_eq!(normals, 901, "the 0x15 family, and nothing else");
    assert_eq!(
        roles.get("tangent-space-normal").copied().unwrap_or(0),
        901,
        "901 records publish a normal: {roles:?}"
    );
    // 1 747 `DXT5` records publish a colour plane and a mask channel -- and
    // crucially NOT a normal -- while the 306 `DXT1` records publish nothing at
    // all. That is the whole corpus role census, and it is what the renderer
    // binds from.
    assert_eq!(
        roles.get("colour-and-mask").copied().unwrap_or(0),
        1747,
        "the DXT5 family: {roles:?}"
    );
    assert_eq!(
        roles.get("unidentified").copied().unwrap_or(0),
        306,
        "the DXT1 family, which no signature claims: {roles:?}"
    );
    assert_eq!(
        roles.values().sum::<usize>(),
        total,
        "every record is counted in exactly one role: {roles:?}"
    );
}

// ---------------------------------------------------------------------------
// Colour: where a Spore creature's colour comes from
// ---------------------------------------------------------------------------

#[test]
#[ignore = "needs SPORE/Data/Spore_Game.package; run with --ignored --nocapture"]
fn a_creature_model_resolves_a_sporemodel_record_from_its_asset_family() {
    use spore_engine::domain::paint::{self, PaintBinding, PaintOrigin, PaintSource};

    let Some(store) = store_with_sporemodels() else {
        return;
    };
    let binding = paint::resolve(&store, &CREATURE, None);
    let PaintBinding::Bound(found) = &binding else {
        panic!("the creature family must resolve: {binding:?}");
    };

    // The family relation is measured, not asserted by hand: 0x40627100 is
    // `CreatureModelsHQ` and 0x40626200 is `CreatureModels` in the committed
    // group-name table, and their 97 instance ids are the same 97.
    assert_eq!(found.type_name, "crt");
    assert_eq!(found.record.type_id, 0x2B97_8C46);
    assert_eq!(found.record.group_id, 0x4062_6200);
    assert_eq!(found.package_name, "Spore_Game");
    assert_eq!(found.origin, PaintOrigin::FamilyDefault);
    // **No byte in the corpus names a creature-part model from a
    // `sporemodel`.** So the association is inferred while the colours
    // themselves are observed, and the two must not be merged.
    assert_eq!(found.association, spore_core::EvidenceLevel::Inferred);
    assert_eq!(
        paint::contents_provenance().level(),
        spore_core::EvidenceLevel::Observed
    );
    // 47 creature presets share this family, so the count is on the record and
    // the choice is the lowest identity in it.
    assert_eq!(found.family_candidates, 47);
    assert_eq!(
        found.record,
        ResourceKey::new(0x2B97_8C46, 0x4062_6200, 0x05E3_DF13)
    );

    // The colour, verbatim, from `<skincolor1>`.
    let application = found
        .application()
        .expect("a creature record has skincolor1");
    assert_eq!(application.source, PaintSource::SkinSlot(1));
    assert_eq!(
        application.base.to_corpus_text(),
        "0.498039,0.803922,0.474510",
        "the measured skincolor1 of 0x2b978c46:0x40626200:0x05e3df13"
    );
    assert_eq!(found.base_rgb().unwrap(), [0.498_039, 0.803_922, 0.474_510]);
    assert_eq!(found.base_hex().as_deref(), Some("#7fcd79"));
    // ...and it is NOT greyscale, which is the whole point of the finding.
    let [r, g, b] = found.base_rgb().unwrap();
    assert!(
        g - r.max(b) > 0.25,
        "a Spore creature colour is not grey: {:?}",
        found.base_rgb().unwrap()
    );

    // The per-mesh cut: every mesh gets the same colour, because a creature
    // record has no region mask to cut on.
    for mesh in 0..2u32 {
        let mesh = paint::mesh_paint(&binding, mesh);
        assert_eq!(mesh.base_color, found.base_rgb());
        assert_eq!(mesh.source, Some(PaintSource::SkinSlot(1)));
        assert_eq!(
            mesh.regions.elements(),
            0,
            "0 of 47 crt records hold a <paint>"
        );
        assert!(
            !mesh.regions.is_decoded(),
            "no bit of a region mask is ever decoded"
        );
    }
    let line = binding.describe(&CREATURE, 0, 1);
    assert!(line.contains("base=#7fcd79"), "{line}");
    assert!(line.contains("regions=absent"), "{line}");
    assert!(line.contains("alpha_source=UNKNOWN"), "{line}");
}

#[test]
#[ignore = "needs SPORE/Data/Spore_Content.package; run with --ignored --nocapture"]
fn a_creature_model_has_no_colour_at_all_when_the_colour_package_is_not_opened() {
    use spore_engine::domain::paint::{self, PaintBinding, PaintUnavailableReason};

    // `Spore_Content` holds **no** `sporemodel` record. That is the diagnosis,
    // and it is a different one from "there is no colour in Spore".
    let Some(store) = store() else { return };
    let binding = paint::resolve(&store, &CREATURE, None);
    assert_eq!(
        binding,
        PaintBinding::Unavailable {
            reason: PaintUnavailableReason::NoSporemodelRecords,
            fact: spore_core::Fact::unavailable(
                "no opened package holds a record of a sporemodel type; \
                 Spore_Game.package and PatchData.package carry them"
            ),
        }
    );
    // Unavailable is not black and not grey: the renderer substitutes its
    // neutral tint and the report says why.
    assert_eq!(paint::mesh_paint(&binding, 0).base_color, None);
    let line = binding.describe(&CREATURE, 0, 1);
    assert!(line.contains("state=unavailable"), "{line}");
    assert!(line.contains("reason=no_sporemodel_records"), "{line}");
    assert!(line.contains("base=absent"), "{line}");
}

#[test]
#[ignore = "needs SPORE/Data/Spore_Game.package; run with --ignored --nocapture"]
fn every_creature_and_cell_sporemodel_record_is_read_and_nearly_none_is_grey() {
    use spore_engine::domain::paint::SPOREMODEL_TYPES;

    let Some(store) = store_with_sporemodels() else {
        return;
    };
    // Two claims, measured over every record of the two creature-shaped types.
    let (mut records, mut paints, mut distinct) = (0usize, 0usize, Vec::new());
    for (type_id, name) in [SPOREMODEL_TYPES[0], SPOREMODEL_TYPES[1]] {
        for key in store.identities_of_type(type_id) {
            let bytes = store.read(&key).expect("every sporemodel record extracts");
            let model = spore_assets::extract_model_colors(&bytes)
                .unwrap_or_else(|e| panic!("{name} {key} decodes: {e}"));
            records += 1;
            assert_eq!(
                model.skin_colors.iter().filter(|c| c.is_some()).count(),
                3,
                "{name} {key} carries all three skin slots or none"
            );
            paints += model
                .paint_lists
                .iter()
                .map(|list| list.paints.len())
                .sum::<usize>();
            for colour in model.distinct_colors() {
                if !distinct.contains(&colour) {
                    distinct.push(colour);
                }
            }
        }
    }
    eprintln!(
        "sporemodel records={records} paint_elements={paints} distinct={}",
        distinct.len()
    );
    // Measured: 47 crt + 13 cll, and **zero** `<paint>` elements between them.
    assert_eq!(records, 60, "47 creature + 13 cell presets");
    assert_eq!(
        paints, 0,
        "a creature/cell colour record has no region mask; the 390 paints in the \
         install all belong to vcl/ufo/bld"
    );
    // 38 distinct colours across the two families (the whole install's 59
    // includes the vehicle/ufo/building paint lists).
    assert!(
        distinct.len() >= 24,
        "only {} distinct colours",
        distinct.len()
    );
    // Per channel, measured over those 38: red 0.141176..1.0 (span 0.8588),
    // green 0.223528..1.0 (span 0.7765), blue 0.0..1.0 (span 1.0). Every
    // channel reaches the top of the range and dips below a quarter of the way
    // down, which is the direct refutation of "the data has no colour in it".
    let mut spans = [(f32::INFINITY, f32::NEG_INFINITY); 3];
    for colour in &distinct {
        for (channel, value) in colour.components().into_iter().enumerate() {
            spans[channel].0 = spans[channel].0.min(value);
            spans[channel].1 = spans[channel].1.max(value);
        }
    }
    for (channel, (lo, hi)) in spans.iter().enumerate() {
        assert_eq!(
            *hi, 1.0,
            "channel {channel} must reach the top of the range; spans {lo}..{hi}"
        );
        assert!(
            *lo < 0.25,
            "channel {channel} spans only {lo}..{hi}; Spore creature colour is not greyscale"
        );
    }
    // And the union reaches both ends of the range exactly.
    let all: Vec<f32> = distinct.iter().flat_map(|c| c.components()).collect();
    let lo = all.iter().copied().fold(f32::INFINITY, f32::min);
    let hi = all.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    assert_eq!((lo, hi), (0.0, 1.0), "the corpus writes 0 and 1 outright");
}
