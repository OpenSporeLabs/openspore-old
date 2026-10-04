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
//! | [`raster_population`] | the fourcc census, the chain count, the `0x15` framing identity |
//! | [`dxt5_records_carry_real_colour`] | **the refutation**: what the four lanes of every decodable DXT5 record actually contain |
//! | [`dxt5_colour_lanes_are_correlated_and_the_fourth_is_not`] | the mutual-correlation and independence statistics behind `TextureRole::ColourAndMask` |
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
    classify, colour_and_mask_signature, normal_map_signature, partition_texture_refs, Lane,
    MipStatistics, TextureRole, COLOUR_LANE_CARDINALITY_FLOOR,
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

/// The raster population: fourcc census, chain count, `0x15` framing.
#[test]
#[ignore = "needs the git-ignored SPORE/ tree"]
fn raster_population() {
    let store = store();
    let mut counts = std::collections::BTreeMap::new();
    let mut dxt5_chains = std::collections::BTreeMap::new();
    let mut lum_identity_ok = 0usize;
    let mut lum_identity_bad = 0usize;
    let mut shapes = std::collections::BTreeMap::new();
    let mut dxt5_shapes = std::collections::BTreeMap::new();

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
                *dxt5_shapes
                    .entry((
                        envelope.width,
                        envelope.height,
                        envelope.mip_count,
                        record.len(),
                    ))
                    .or_insert(0usize) += 1;
                match spore_texture::layer_count_for(record.len(), &envelope) {
                    Ok(chains) => *dxt5_chains.entry(chains).or_insert(0usize) += 1,
                    Err(_) => *dxt5_chains.entry(0usize).or_insert(0usize) += 1,
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
    println!("# DXT5 (w, h, mips, record bytes) shapes: {dxt5_shapes:#x?}");
    println!("# DXT5 derived chain counts: {dxt5_chains:?}");
    println!("# 0x15 framing identity: {lum_identity_ok} exact, {lum_identity_bad} not");

    assert_eq!(counts.get(&0x3554_5844), Some(&1747), "DXT5");
    assert_eq!(counts.get(&0x0000_0015), Some(&901), "the 0x15 family");
    assert_eq!(
        counts.get(&0x3154_5844),
        Some(&306),
        "DXT1, which spore-texture also decodes"
    );
    assert_eq!(lum_identity_bad, 0, "the 0x15 framing identity must hold");
    assert_eq!(lum_identity_ok, 901);
    // **THE REFUTATION, in the population.** The previous version of this test
    // asserted 1374 records yielding 2 chains and 373 yielding none, and the 373
    // were "4 or 12 bytes short of two whole layers" — an artefact of charging 8
    // bytes per 4x4 block. Under the measured 16-byte stride every record
    // decodes and every one holds exactly one chain.
    assert_eq!(
        dxt5_chains.get(&1),
        Some(&1747),
        "every DXT5 record is one chain and none is refused"
    );
    assert_eq!(
        dxt5_chains.get(&2),
        None,
        "no record holds two chains: the two-layer reading is refuted"
    );
    assert_eq!(dxt5_chains.get(&0), None, "and none is refused either");
    // Three shapes, and each one's record length is the measured one. The
    // 64x64 shape is the one the old rule refused outright.
    assert_eq!(dxt5_shapes.len(), 3);
    assert_eq!(dxt5_shapes.get(&(512, 512, 10, 349_616)), Some(&1374));
    assert_eq!(dxt5_shapes.get(&(256, 256, 9, 87_468)), Some(&306));
    assert_eq!(dxt5_shapes.get(&(64, 64, 7, 5_540)), Some(&67));
}

// ------------------------------------------------------------------ 2. colour

/// **THE REFUTATION.** Every decodable `DXT5` record in the corpus carries real
/// colour in lanes 0–2.
///
/// This test used to be called `dxt5_records_carry_no_colour` and asserted the
/// exact opposite — a green span of 0, a blue span of 0 and at most two red
/// values across every record. Those numbers were a property of an 8-byte block
/// read: green and blue came out zero *by construction*, because R5G5B5 on a
/// 6-bit word cannot set bits 10..19. It measured the codec, not the data. The
/// test keeps its name changed, its purpose intact, and its old numbers in
/// [`claims::DXT5_CARRIES_NO_COLOUR`](spore_material::claims::DXT5_CARRIES_NO_COLOUR).
#[test]
#[ignore = "needs the git-ignored SPORE/ tree"]
fn dxt5_records_carry_real_colour() {
    let store = store();
    let mut records = 0usize;
    let mut texels = 0usize;
    let mut min_card = [usize::MAX; 4];
    let mut max_card = [0usize; 4];
    let mut global_min = [u8::MAX; 4];
    let mut global_max = [0u8; 4];
    let mut widest_span = [0u32; 4];
    let mut signature_ok = 0usize;
    let mut signature_declined = Vec::new();
    let mut constant_fourth_plane = 0usize;
    let mut fourth_plane_distinct: Vec<usize> = Vec::new();
    let mut role_census: std::collections::BTreeMap<&str, usize> =
        std::collections::BTreeMap::new();

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
        assert_eq!(
            image.layers.len(),
            1,
            "{key:?}: one chain, or the framing identity did not close"
        );
        let Some(base) = image.layers.first().and_then(|chain| chain.first()) else {
            continue;
        };
        records += 1;
        texels += base.pixels.len() / 4;
        let stats = MipStatistics::of(base);
        for lane in 0..4 {
            min_card[lane] = min_card[lane].min(stats.distinct[lane]);
            max_card[lane] = max_card[lane].max(stats.distinct[lane]);
            global_min[lane] = global_min[lane].min(stats.min[lane]);
            global_max[lane] = global_max[lane].max(stats.max[lane]);
            widest_span[lane] =
                widest_span[lane].max(u32::from(stats.max[lane]) - u32::from(stats.min[lane]));
        }
        fourth_plane_distinct.push(stats.distinct[Lane::Alpha.index()]);
        if stats.is_constant(Lane::Alpha) {
            constant_fourth_plane += 1;
        }
        if colour_and_mask_signature(&stats) {
            signature_ok += 1;
        } else {
            signature_declined.push(format!("{key:?} cards={:?}", stats.distinct));
        }
        *role_census
            .entry(classify(envelope.fourcc, &stats).as_str())
            .or_insert(0usize) += 1;
    }
    fourth_plane_distinct.sort_unstable();

    println!("# decodable DXT5 records: {records}");
    println!("# base-mip texels examined: {texels}");
    println!("# min distinct values per lane (r, g, b, a): {min_card:?}");
    println!("# max distinct values per lane (r, g, b, a): {max_card:?}");
    println!("# global per-lane min: {global_min:?}");
    println!("# global per-lane max: {global_max:?}");
    println!("# widest per-lane span: {widest_span:?}");
    println!(
        "# fourth-plane distinct values, min/max: {} / {}",
        fourth_plane_distinct.first().copied().unwrap_or(0),
        fourth_plane_distinct.last().copied().unwrap_or(0)
    );
    println!("# records whose fourth plane is CONSTANT: {constant_fourth_plane}");
    println!("# records the colour signature admits: {signature_ok}");
    println!(
        "# records the colour signature declines: {}",
        signature_declined.len()
    );
    println!("# role census: {role_census:?}");

    // The measurement behind claims::COLOUR_AND_MASK_CONTENT and the refutation
    // of claims::DXT5_CARRIES_NO_COLOUR.
    assert_eq!(records, 1747, "every DXT5 record in the package decodes");
    assert!(
        min_card[0] >= COLOUR_LANE_CARDINALITY_FLOOR,
        "lane 0 never drops to the floor: the smallest is {min_card:?}"
    );
    assert!(min_card[1] >= COLOUR_LANE_CARDINALITY_FLOOR);
    assert!(min_card[2] >= COLOUR_LANE_CARDINALITY_FLOOR);
    assert_eq!(global_max, [255, 255, 255, 255]);
    assert_eq!(global_min, [0, 0, 0, 0]);
    // **The specific numbers the claims and the doc quote.** If these move, the
    // constants beside them (`COLOUR_LANE_CARDINALITY_FLOOR`) have to be
    // re-derived rather than left to look approximately right.
    assert_eq!(
        min_card,
        [32, 64, 32, 1],
        "the per-lane minima the doc and the claims cite"
    );
    assert_eq!(max_card, [235, 255, 235, 256]);
    assert_eq!(
        widest_span[1], 255,
        "green spans the full range somewhere: it is not a constant lane"
    );
    // The signature admits every single record: no corpus record has a colour
    // lane at or below the floor, so there is no "degenerate colour field" class
    // left in this corpus.
    assert_eq!(signature_ok, records, "the role check must not decline");
    assert!(
        signature_declined.is_empty(),
        "{:?}",
        &signature_declined[..signature_declined.len().min(5)]
    );
    // The fourth plane: 46 records hold it constant, which is why no signature
    // condition mentions it.
    assert_eq!(
        constant_fourth_plane, 46,
        "records whose fourth plane never varies -- the case a 'mask must vary' \
         condition would have refused"
    );
    assert_eq!(
        fourth_plane_distinct.first().copied(),
        Some(1),
        "and the smallest fourth-plane cardinality is 1, i.e. all-zero"
    );
    // Every record classifies as colour-and-mask from its own bytes.
    assert_eq!(
        role_census.get(TextureRole::ColourAndMask.as_str()),
        Some(&records)
    );
}

/// What makes the three colour lanes one colour image rather than three
/// independent channels — and what makes the fourth plane something else.
///
/// `TextureRole::ColourAndMask` names two content classes. This test measures
/// both, and it is deliberately **not** used as a gate: 467 of 1 747 records fall
/// below a 0.30 mean colour correlation, so any correlation threshold would be a
/// statistic of this corpus rather than a property of the format. The numbers go
/// into the claims; the gate stays on the cardinality floor.
#[test]
#[ignore = "needs the git-ignored SPORE/ tree"]
fn dxt5_colour_lanes_are_correlated_and_the_fourth_is_not() {
    let store = store();
    let mut colour: Vec<f64> = Vec::new();
    let mut alpha: Vec<f64> = Vec::new();
    let mut undefined_colour = 0usize;
    let mut undefined_alpha = 0usize;
    let mut records = 0usize;

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
        let Some(base) = image.layers.first().and_then(|chain| chain.first()) else {
            continue;
        };
        records += 1;
        let stats = MipStatistics::of(base);
        match stats.mean_colour_correlation() {
            Some(value) => colour.push(value),
            // A constant colour lane makes the correlation undefined, which is an
            // absence and is counted rather than recorded as 0.0.
            None => undefined_colour += 1,
        }
        match stats.mean_alpha_correlation_abs() {
            Some(value) => alpha.push(value),
            None => undefined_alpha += 1,
        }
    }
    colour.sort_by(|a, b| a.partial_cmp(b).unwrap());
    alpha.sort_by(|a, b| a.partial_cmp(b).unwrap());

    let describe = |values: &[f64]| {
        format!(
            "min {:.4} p1 {:.4} median {:.4} max {:.4}",
            values.first().copied().unwrap_or(f64::NAN),
            values.get(values.len() / 100).copied().unwrap_or(f64::NAN),
            values.get(values.len() / 2).copied().unwrap_or(f64::NAN),
            values.last().copied().unwrap_or(f64::NAN),
        )
    };
    println!("# records: {records}");
    println!("# mean colour-pair correlation  {}", describe(&colour));
    println!("# mean |colour-to-alpha| corr   {}", describe(&alpha));
    println!("# records where the colour correlation is undefined: {undefined_colour}");
    println!("# records where the alpha correlation is undefined: {undefined_alpha}");
    let below_030 = colour.iter().filter(|v| **v < 0.30).count();
    println!("# records below a 0.30 mean colour correlation: {below_030}");

    assert_eq!(records, 1747);
    // The numbers `MASK_PLANE_INDEPENDENT_OF_COLOUR` and the doc cite. Compared
    // with a tolerance rather than exactly: the inputs are exact integers and the
    // correlation is computed in `f64`, so the last bits are arithmetic order
    // rather than a fact about the corpus.
    const TOLERANCE: f64 = 5e-5;
    let near = |got: Option<f64>, want: f64, what: &str| {
        let got = got.unwrap_or_else(|| panic!("{what} was undefined"));
        assert!(
            (got - want).abs() < TOLERANCE,
            "{what}: measured {got:.6}, the claims and the doc cite {want}"
        );
    };
    near(colour.first().copied(), -0.0779, "colour min");
    near(
        colour.get(colour.len() / 100).copied(),
        -0.0165,
        "colour p1",
    );
    near(
        colour.get(colour.len() / 2).copied(),
        0.5103,
        "colour median",
    );
    near(colour.last().copied(), 0.7124, "colour max");
    near(alpha.first().copied(), 0.0039, "alpha min");
    near(alpha.get(alpha.len() / 2).copied(), 0.0673, "alpha median");
    near(alpha.last().copied(), 0.5389, "alpha max");
    assert_eq!(
        below_030, 467,
        "the count that makes a correlation threshold a statistic rather than a \
         property -- which is why the signature gates on cardinality instead"
    );
    // Undefined means undefined: the 46 constant-fourth-plane records make the
    // colour-to-alpha correlation meaningless, and they are counted, not zeroed.
    assert_eq!(undefined_alpha, 46, "one per constant-fourth-plane record");
    assert_eq!(undefined_colour, 0, "no record has a constant colour lane");
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
            TextureRole::ColourAndMask,
            TextureRole::ColourAndMask,
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
    assert_eq!(normal.layer_count, 1, "one chain, as for every record");
    assert!(normal.pixels_decoded());
    assert!(normal_map_signature(&normal.statistics));

    // The two `DXT5` records, and the measurements the claims cite about them.
    // These are the exact numbers `docs/MATERIAL-ROLES.md` §4 publishes, so a
    // change to any of them means the doc and the claim table are stale.
    for (index, expected_cards, expected_alpha_max) in [
        (0usize, [92usize, 121, 73, 21], 73u8),
        (1, [184, 238, 198, 32], 73),
    ] {
        let texture = resolved.textures[index].as_ref().expect("resolves");
        assert_eq!(texture.envelope.width, 512);
        assert_eq!(texture.layer_count, 1);
        assert_eq!(
            texture.statistics.distinct, expected_cards,
            "slot {index}: the per-lane cardinalities the doc cites"
        );
        assert_eq!(
            texture.statistics.max[Lane::Alpha.index()],
            expected_alpha_max,
            "slot {index}: the fourth plane tops out at 73"
        );
        assert_eq!(texture.role, TextureRole::ColourAndMask);
        assert!(colour_and_mask_signature(&texture.statistics));
        assert_eq!(texture.mask_plane_varies(), Some(true));
        assert_eq!(texture.channels().len(), 4);
    }
    // And the first record's 4436 distinct RGB triples: the single most direct
    // statement that "DXT5 carries no colour" was a property of a codec.
    let first = resolved.textures[0].as_ref().expect("resolves");
    let mut triples = std::collections::BTreeSet::new();
    for texel in first.image.pixels.chunks_exact(4) {
        triples.insert([texel[0], texel[1], texel[2]]);
    }
    println!(
        "# distinct RGB triples in 0x2f4e681c:0x40632900:0x067a07f0: {}",
        triples.len()
    );
    assert_eq!(
        triples.len(),
        4436,
        "the figure the doc quotes as the refutation of 'no colour'"
    );
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
