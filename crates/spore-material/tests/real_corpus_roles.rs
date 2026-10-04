//! Real-corpus verification of the role and binding model. **Every test here is
//! `#[ignore]`d.**
//!
//! These need the installed game data, which is git-ignored and about 5 GB:
//!
//! ```text
//! SPORE/Data/Spore_Content.package     995 MB
//! ```
//!
//! ```text
//! cargo test -p spore-material --release -- --ignored --nocapture
//! ```
//!
//! `--release` matters (a 995 MB package, memory-mapped and walked) and
//! `--nocapture` matters because each test prints its measurement: the numbers
//! are the deliverable and a bare `ok` hides them.
//!
//! # What these tests are for
//!
//! The claims this crate records were measured with the repository's **Python
//! oracles** (`tools/spore/gmdl`, `tools/spore/raster`, `tools/spore/dxt5`),
//! because those expose things `spore-gmdl` does not — the material-info block
//! boundaries and the 16 entry-header bytes. These tests re-measure what the
//! **Rust** crates can see, so the two implementations have to agree before a
//! number is trusted:
//!
//! | Test | Re-measures |
//! |---|---|
//! | [`raster_population`] | the fourcc census, the DXT5 layer split, the `0x15` framing identity |
//! | [`dxt5_records_carry_no_colour`] | the colour degeneracy, over every decodable DXT5 record |
//! | [`normal_map_signature_holds_corpus_wide`] | the normal-map role check, over every `0x15` record |
//! | [`every_gmdl_record_partitions_evenly`] | the per-mesh partition's divisibility, over every gmdl record |
//!
//! # What they deliberately do NOT check
//!
//! Nothing here re-derives the roles: a role must be assigned from a record's
//! own bytes at resolve time, which is what `resolve_texture` does. A census
//! that computed roles itself would be a second source of truth, which is the
//! defect this workspace has already paid for twice.

use spore_assets::{ContentStore, Package};
use spore_core::ResourceKey;
use spore_gmdl::GMDL_TYPE;
use spore_material::{
    classify, layered_mask_signature, normal_map_signature, partition_texture_refs, Lane,
    MipStatistics, TextureRole,
};
use spore_texture::{parse_envelope, RASTER_TYPE};

/// The package every measurement below is over, resolved against the workspace
/// root rather than the crate directory: a test's working directory is its own
/// crate, so a bare relative path is a coin flip.
const CONTENT: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../SPORE/Data/Spore_Content.package"
);

/// The documented asset, used for a byte-level spot check of the roles.
const DOCUMENTED_MODEL: ResourceKey = ResourceKey::new(GMDL_TYPE, 0x4063_7E03, 0x067A_07F0);

fn store() -> ContentStore {
    let mut store = ContentStore::new();
    store.push(Package::open("Spore_Content", CONTENT).expect("Spore_Content.package is present"));
    store
}

fn raster_keys(store: &ContentStore) -> Vec<ResourceKey> {
    store.identities_of_type(RASTER_TYPE)
}

// ---------------------------------------------------------------- 1. population

/// The raster population: fourcc census, layer split, `0x15` framing.
#[test]
#[ignore = "needs the git-ignored SPORE/ tree"]
fn raster_population() {
    let store = store();
    let mut counts = std::collections::BTreeMap::new();
    let mut dxt5_layers = std::collections::BTreeMap::new();
    let mut lum_identity_ok = 0usize;
    let mut lum_identity_bad = 0usize;
    let mut shapes = std::collections::BTreeMap::new();

    for key in raster_keys(&store) {
        let Ok(record) = store.read(&key) else {
            continue;
        };
        let Ok(envelope) = parse_envelope(&record) else {
            continue;
        };
        *counts.entry(envelope.fourcc).or_insert(0usize) += 1;
        *shapes
            .entry((envelope.fourcc, envelope.width, envelope.height))
            .or_insert(0usize) += 1;
        match envelope.fourcc {
            spore_texture::DXT5_FOURCC => {
                match spore_texture::layer_count_for(record.len(), &envelope) {
                    Ok(layers) => *dxt5_layers.entry(layers).or_insert(0usize) += 1,
                    Err(_) => *dxt5_layers.entry(0usize).or_insert(0usize) += 1,
                }
            }
            spore_texture::LUMINANCE_FOURCC => {
                let expected = spore_texture::luminance_record_size(
                    envelope.width,
                    envelope.height,
                    envelope.mip_count,
                );
                if expected == record.len() {
                    lum_identity_ok += 1;
                } else {
                    lum_identity_bad += 1;
                }
            }
            _ => {}
        }
    }

    println!("# raster records: {}", counts.values().sum::<usize>());
    println!("# fourcc census: {counts:#x?}");
    println!("# shapes: {shapes:#x?}");
    println!("# dxt5 derived layer counts: {dxt5_layers:?}");
    println!("# 0x15 framing identity: {lum_identity_ok} exact, {lum_identity_bad} not");

    assert_eq!(counts.get(&0x3554_5844), Some(&1747), "DXT5");
    assert_eq!(counts.get(&0x0000_0015), Some(&901), "the 0x15 family");
    assert_eq!(
        counts.get(&0x3154_5844),
        Some(&306),
        "DXT1, refused by name"
    );
    assert_eq!(lum_identity_bad, 0, "the 0x15 framing identity must hold");
    assert_eq!(lum_identity_ok, 901);
    // The derived DXT5 split: 1374 records divide into 2 layers and 373 do not
    // divide at all. The 373 are the 64^2 and 256^2 families, which are 12
    // bytes short of two layers -- a finding for `spore-texture`, recorded here
    // because it bounds what this crate can classify.
    assert_eq!(dxt5_layers.get(&2), Some(&1374));
    assert_eq!(dxt5_layers.get(&0), Some(&373));
}

// ------------------------------------------------------------------ 2. colour

/// No decodable DXT5 record in the corpus carries a colour.
#[test]
#[ignore = "needs the git-ignored SPORE/ tree"]
fn dxt5_records_carry_no_colour() {
    let store = store();
    let mut records = 0usize;
    let mut texels = 0usize;
    let mut green_max_span = 0u32;
    let mut blue_max_span = 0u32;
    let mut red_cardinality_max = 0usize;
    let mut alpha_min_distinct = usize::MAX;
    let mut all_layers_match = 0usize;
    let mut layers_differing = 0usize;

    for key in raster_keys(&store) {
        let Ok(record) = store.read(&key) else {
            continue;
        };
        let Ok(envelope) = parse_envelope(&record) else {
            continue;
        };
        if envelope.fourcc != spore_texture::DXT5_FOURCC {
            continue;
        }
        let Ok(image) = spore_texture::decode_raster(&record) else {
            continue;
        };
        records += 1;
        let mut layers: Vec<Vec<u8>> = Vec::new();
        for layer in &image.layers {
            let Some(base) = layer.first() else { continue };
            let stats = MipStatistics::of(base);
            texels += stats.texels;
            green_max_span = green_max_span.max(
                u32::from(stats.max[Lane::Green.index()])
                    - u32::from(stats.min[Lane::Green.index()]),
            );
            blue_max_span = blue_max_span.max(
                u32::from(stats.max[Lane::Blue.index()]) - u32::from(stats.min[Lane::Blue.index()]),
            );
            red_cardinality_max = red_cardinality_max.max(stats.distinct[Lane::Red.index()]);
            alpha_min_distinct = alpha_min_distinct.min(stats.distinct[Lane::Alpha.index()]);
            assert!(
                layered_mask_signature(&stats),
                "{key:?}: layer statistics do not match the mask signature"
            );
            layers.push(base.pixels.clone());
        }
        if layers.len() == 2 {
            if layers[0] == layers[1] {
                all_layers_match += 1;
            } else {
                layers_differing += 1;
            }
        }
    }

    println!("# decodable DXT5 records: {records}");
    println!("# base-mip texels examined: {texels}");
    println!("# widest green span: {green_max_span}");
    println!("# widest blue span: {blue_max_span}");
    println!("# largest red cardinality: {red_cardinality_max}");
    println!("# smallest alpha cardinality: {alpha_min_distinct}");
    println!("# records whose two layers are identical: {all_layers_match}");
    println!("# records whose two layers differ: {layers_differing}");

    // The measurement behind claims::DXT5_CARRIES_NO_COLOUR.
    assert_eq!(green_max_span, 0, "green is constant in every texel");
    assert_eq!(blue_max_span, 0, "blue is constant in every texel");
    assert!(red_cardinality_max <= 2, "red takes at most two values");
    assert!(alpha_min_distinct >= 2, "the alpha plane is never constant");
    // The measurement behind claims::DXT5_LAYERS_ARE_DISTINCT.
    assert_eq!(all_layers_match, 0, "no record's two layers are identical");
    assert_eq!(layers_differing, 1374);
}

// -------------------------------------------------------------- 3. normal map

/// The normal-map role check holds for every `0x15` record in the corpus.
#[test]
#[ignore = "needs the git-ignored SPORE/ tree"]
fn normal_map_signature_holds_corpus_wide() {
    let store = store();
    let mut records = 0usize;
    let mut texels = 0usize;
    let mut lane0_min_mean = f64::MAX;
    let mut lane0_max_mean = f64::MIN;
    let mut lane1_max_dev = 0.0f64;
    let mut lane2_max_dev = 0.0f64;
    let mut flat_normal_records = 0usize;
    let mut unidentified = 0usize;

    for key in raster_keys(&store) {
        let Ok(record) = store.read(&key) else {
            continue;
        };
        let Ok(envelope) = parse_envelope(&record) else {
            continue;
        };
        if envelope.fourcc != spore_texture::LUMINANCE_FOURCC {
            continue;
        }
        let Ok(image) = spore_texture::decode_raster(&record) else {
            continue;
        };
        let Some(base) = image.layers.first().and_then(|layer| layer.first()) else {
            continue;
        };
        let stats = MipStatistics::of(base);
        records += 1;
        texels += stats.texels;
        let (Some(z), Some(x), Some(y)) = (
            stats.mean(Lane::Red),
            stats.mean(Lane::Green),
            stats.mean(Lane::Blue),
        ) else {
            continue;
        };
        lane0_min_mean = lane0_min_mean.min(z);
        lane0_max_mean = lane0_max_mean.max(z);
        lane1_max_dev = lane1_max_dev.max((x - 127.5).abs());
        lane2_max_dev = lane2_max_dev.max((y - 127.5).abs());
        if stats.distinct[Lane::Red.index()] == 1
            && stats.distinct[Lane::Green.index()] == 1
            && stats.distinct[Lane::Blue.index()] == 1
        {
            flat_normal_records += 1;
        }
        let role = classify(envelope.fourcc, &stats);
        if role != TextureRole::TangentSpaceNormal {
            unidentified += 1;
        }
    }

    println!("# 0x15 records: {records}, base-mip texels: {texels}");
    println!("# lane 0 (Z) mean range: {lane0_min_mean:.2} .. {lane0_max_mean:.2}");
    println!("# lane 1 (X) max deviation from 127.5: {lane1_max_dev:.2}");
    println!("# lane 2 (Y) max deviation from 127.5: {lane2_max_dev:.2}");
    println!("# records whose lanes 0-2 are constant: {flat_normal_records}");
    println!("# records the role check declined: {unidentified}");

    // The claim text quotes these bounds, so they are asserted rather than
    // merely printed.
    assert_eq!(records, 901);
    assert_eq!(unidentified, 0, "every 0x15 record satisfies the signature");
    assert!(
        lane0_min_mean >= 224.0,
        "the floor in role.rs is not below the corpus"
    );
    assert!(
        lane1_max_dev <= 16.0,
        "the tolerance in role.rs is not below the corpus"
    );
    assert!(lane2_max_dev <= 16.0);
    // And the margins the constants claim, measured.
    println!(
        "# smallest lane-0 lead over lane 1: {:.2}",
        lane0_min_mean - (127.5 + lane1_max_dev)
    );
    assert_eq!(
        flat_normal_records, 7,
        "the seven records holding the constant (255, 128, 128)"
    );
}

// ----------------------------------------------------------------- 4. the cut

/// Every gmdl record the Rust parser walks partitions evenly by mesh count.
///
/// The load-bearing arithmetic behind [`spore_material::BindingScope`]: the cut
/// is only sound if the mesh count divides the flat reference count. Measured
/// with the Python oracle over 1819 records (2381 blocks, 6684 references);
/// this re-measures the divisibility with the Rust reader, which walks more
/// records, so the two are complementary rather than identical.
#[test]
#[ignore = "needs the git-ignored SPORE/ tree"]
fn every_gmdl_record_partitions_evenly() {
    let store = store();
    let mut parsed = 0usize;
    let mut even = 0usize;
    let mut odd = Vec::new();
    let mut no_meshes = 0usize;
    let mut refs_total = 0usize;
    let mut per_mesh = std::collections::BTreeMap::new();
    let mut ref_groups = std::collections::BTreeSet::new();
    let mut zero_group_refs = 0usize;
    let mut zero_group_models = std::collections::BTreeSet::new();

    for key in store.identities_of_type(GMDL_TYPE) {
        let Ok(record) = store.read(&key) else {
            continue;
        };
        let Ok(model) = spore_gmdl::parse(&record) else {
            continue;
        };
        parsed += 1;
        refs_total += model.texture_refs.len();
        for reference in &model.texture_refs {
            ref_groups.insert(reference.group_id & 0xFFFFFF00);
            if reference.group_id == 0 {
                zero_group_refs += 1;
                zero_group_models.insert(key);
            }
        }
        if model.meshes.is_empty() {
            no_meshes += 1;
            continue;
        }
        if partition_texture_refs(&model).is_some() {
            even += 1;
            *per_mesh
                .entry(model.texture_refs.len() / model.meshes.len())
                .or_insert(0usize) += 1;
        } else {
            odd.push(key);
        }
    }

    println!("# gmdl records parsed: {parsed}");
    println!("# partitioned evenly: {even}");
    println!("# partitioned unevenly: {}", odd.len());
    println!("# records with no meshes: {no_meshes}");
    println!("# texture references total: {refs_total}");
    println!("# references per mesh histogram: {per_mesh:?}");
    println!("# referenced group families: {ref_groups:#x?}");
    println!(
        "# references naming group id 0: {zero_group_refs} across {} models",
        zero_group_models.len()
    );

    assert_eq!(odd.len(), 0, "every record must divide evenly: {odd:?}");
    assert_eq!(
        even,
        parsed - no_meshes,
        "every record with a mesh is accounted for"
    );
    assert_eq!(no_meshes, 1, "the single meshCount-0 record in the corpus");
    assert_eq!(
        parsed, 3590,
        "the Rust reader walks more records than the Python oracle"
    );
    // The corpus's two shapes, as THIS reader sees them: 2 references per mesh
    // for the two-entry blocks and 3 for the three-entry ones. The absolute
    // counts are higher than the Python oracle's 459/1922 because the Rust
    // reader accepts 3590 of the 4209 gmdl records where the oracle walks 1819
    // -- so this is a wider sample of the same regularity, not a rival figure.
    assert_eq!(per_mesh.get(&2).copied().unwrap_or(0), 642);
    assert_eq!(per_mesh.get(&3).copied().unwrap_or(0), 2947);
    assert_eq!(per_mesh.len(), 2, "only two shapes occur");
    assert_eq!(refs_total, 15_390, "every reference counted once");
    assert_eq!(
        zero_group_refs, 150,
        "references naming group id 0, which no raster record carries"
    );
    assert_eq!(
        zero_group_models.len(),
        132,
        "and the models that carry them"
    );
    // SIX group families, not four. The Python oracle's narrower sample (1819 of
    // 3590 records) saw only 0x4063/64/65/66; the Rust reader also finds
    // 0x40612900 and 0x406b2900. `docs/MATERIAL-ROLES.md` records the wider set,
    // because a wider sample of the same regularity beats a narrower one.
    assert_eq!(
        ref_groups.len(),
        7,
        "six families plus the degenerate group-0 set"
    );
    for family in [
        0x4061_2900u32,
        0x4063_2900,
        0x4064_2900,
        0x4065_2900,
        0x4066_2900,
        0x406b_2900,
    ] {
        assert!(ref_groups.contains(&family), "0x{family:08x} is missing");
    }
    // The seventh "family" is 150 references whose group id is literally 0 --
    // not a naming family but an absence of one. Those are counted separately
    // below rather than folded into the shapes.
    assert!(ref_groups.contains(&0), "the group-0 population is missing");
}

// ------------------------------------------------------------------ 5. the asset

/// The documented asset, resolved: the roles the engine will actually see.
#[test]
#[ignore = "needs the git-ignored SPORE/ tree"]
fn the_documented_assets_roles() {
    let store = store();
    let resolved = spore_material::resolve_model_record(&store, &DOCUMENTED_MODEL, RASTER_TYPE)
        .expect("the documented asset resolves");

    println!("# meshes: {}", resolved.meshes.len());
    println!("# scope: {:?}", resolved.scope);
    println!("# observed shape: {:?}", resolved.observed_shape);
    println!("# ordinal agreement: {:?}", resolved.ordinal_agreement);
    println!("# role census: {:?}", resolved.role_census());
    for texture in &resolved.textures {
        let texture = texture.as_ref().expect("every reference resolves");
        println!(
            "# {:?} {}x{} mips={} layers={} role={:?}",
            texture.key,
            texture.envelope.width,
            texture.envelope.height,
            texture.mip_count,
            texture.layer_count,
            texture.role
        );
    }

    assert_eq!(resolved.resolved_count(), 3, "all three references resolve");
    assert_eq!(
        resolved.roles(),
        vec![vec![
            TextureRole::LayeredAlphaMask,
            TextureRole::LayeredAlphaMask,
            TextureRole::TangentSpaceNormal,
        ]]
    );
    assert!(resolved.ordinal_agreement.all_agree());
    // The `0x15` record of this asset is 512x512 with 10 mips, NOT 1024x1024,
    // and it DECODES. An earlier version of this crate's documentation claimed
    // both the wrong size and a refusal; the bytes win and the claim is gone.
    let normal = resolved.textures[2].as_ref().expect("third resolves");
    assert_eq!(normal.envelope.width, 512);
    assert_eq!(normal.envelope.height, 512);
    assert_eq!(normal.mip_count, 10);
    assert!(normal.pixels_decoded());
    assert!(normal_map_signature(&normal.statistics));
}
// --------------------------------------------------------------- 6. group zero

/// 150 references in the corpus name group id **zero**, and no raster record
/// carries that group.
///
/// A separate model family (`0x40616201`) does this, and it is worth its own
/// test because it is the one place where the corpus's texture-reference
/// population contains an identity that cannot exist. It is an *absence of a
/// name*, not a missing file: `spore-material` reports it as
/// [`spore_material::MaterialError::Lookup`], which is the honest variant.
#[test]
#[ignore = "needs the git-ignored SPORE/ tree"]
fn group_zero_references_exist_and_resolve_to_nothing() {
    let store = store();
    let mut references = 0usize;
    let mut models = std::collections::BTreeSet::new();
    let mut resolvable = 0usize;

    for key in store.identities_of_type(GMDL_TYPE) {
        let Ok(record) = store.read(&key) else {
            continue;
        };
        let Ok(model) = spore_gmdl::parse(&record) else {
            continue;
        };
        for reference in &model.texture_refs {
            if reference.group_id != 0 {
                continue;
            }
            references += 1;
            models.insert(key);
            if store.contains(&ResourceKey::new(RASTER_TYPE, 0, reference.instance_id)) {
                resolvable += 1;
            }
        }
    }

    println!(
        "# group-0 references: {references} across {} models",
        models.len()
    );
    println!("# of which a raster record exists at (raster, 0, instance): {resolvable}");

    assert_eq!(references, 150);
    assert_eq!(models.len(), 132);
    assert_eq!(
        resolvable, 0,
        "a group id of zero is an absence of a name, not a record that went missing"
    );
    // Every one of them is in the same group, so this is one model family's
    // convention rather than scattered damage.
    let families: std::collections::BTreeSet<u32> = models.iter().map(|key| key.group_id).collect();
    println!("# the models' own groups: {families:#x?}");
    assert!(
        families.contains(&0x4061_6201),
        "the population that carries them is 0x40616201"
    );
}
