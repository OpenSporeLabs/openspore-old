//! The documented asset, against real game data.
//!
//! **Ignored by default.** This test needs the git-ignored `SPORE/` tree, so it
//! is excluded from `cargo test -p spore-material` and runs only when asked for
//! on a machine that has the game installed:
//!
//! ```text
//! cargo test -p spore-material --test real_asset -- --ignored
//! ```
//!
//! The package path comes from `OPENSPORE_SPORE_DATA` when set, and otherwise
//! from the repository's own `SPORE/Data/` directory. If neither exists the test
//! still passes — by saying so — because a missing game install is not a
//! failure of this crate.
//!
//! # What it pins, and what it stopped pinning
//!
//! This test used to assert that the documented model's three texture
//! references resolve to **two** 512x512 DXT5 images with **two layers each**
//! and **one** typed `UnsupportedFourcc` refusal, on the stated grounds that
//! `0x2f4e681c:0x40632902:0x067a07f0` is a *1024x1024* `0x15` record.
//!
//! **All three halves of that were wrong, and the corpus corrected them.**
//! `spore-texture` established the `0x15` family's framing over 901 of 901
//! records and now decodes it, so the third reference resolves; the record is
//! **512x512**, not 1024x1024 — `Spore_Content.package` contains no 1024² raster
//! of either format; and the two `DXT5` records hold **one chain each**, because
//! the 8-byte block stride that produced a second layer was refuted. They carry
//! **real colour**: 92 / 121 / 73 and 184 / 238 / 198 distinct values in lanes
//! 0 / 1 / 2, and 4 436 distinct RGB triples in the first one's base mip.
//!
//! The test is updated rather than deleted, and the old expectations are recorded
//! as refuted in `docs/MATERIAL-ROLES.md` §8. A test asserting a specification
//! the real data refutes is exactly the "stale claim contradicting its own
//! decoder" defect, and it is worse than no test.
//!
//! What it pins now: three 512x512 images — two `colour-and-mask` `DXT5` and one
//! `tangent-space-normal` `0x15`, **one chain each** — with roles
//! `(colour-and-mask, colour-and-mask, normal)`, an even per-mesh cut, and 3 of 3
//! ordinal agreements. See also `tests/real_corpus_roles.rs`, which measures the
//! same asset over the whole package.

use spore_assets::ContentStore;
use spore_core::ResourceKey;
use spore_gmdl::GMDL_TYPE;
use spore_material::{claims, resolve_model_record, resolve_texture, Lane, TextureRole};
use spore_texture::RASTER_TYPE;

/// The documented asset.
const DOCUMENTED_ASSET: &str = "0x00e6bce5:0x40637e03:0x067a07f0";

/// The instance id shared by all three of its texture references.
const INSTANCE: u32 = 0x067A_07F0;

/// The group of the `0x15`-family reference: a tangent-space normal map, not a
/// refusal.
const NORMAL_MAP_GROUP: u32 = 0x4063_2902;

/// Where the content package lives, or `None` when it cannot be found.
fn package_path() -> Option<std::path::PathBuf> {
    if let Ok(path) = std::env::var("OPENSPORE_SPORE_DATA") {
        let path = std::path::PathBuf::from(path);
        if path.is_file() {
            return Some(path);
        }
        return None;
    }
    let path = std::path::Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../SPORE/Data/Spore_Content.package"
    ));
    path.is_file().then(|| path.to_owned())
}

#[test]
#[ignore = "needs the git-ignored SPORE/ tree; run with --ignored"]
fn the_documented_asset_resolves_three_images_with_measured_roles() {
    let Some(path) = package_path() else {
        eprintln!(
            "skipping: set OPENSPORE_SPORE_DATA to Spore_Content.package \
             (a missing game install is not a failure of this crate)"
        );
        return;
    };
    let mut store = ContentStore::new();
    store.push(spore_assets::Package::open("Spore_Content", &path).expect("opens"));

    let key: ResourceKey = DOCUMENTED_ASSET.parse().expect("a T:G:I spec");
    assert_eq!(key.type_id, GMDL_TYPE);
    let resolved = resolve_model_record(&store, &key, RASTER_TYPE).expect("the model decodes");

    assert_eq!(
        resolved.textures.len(),
        3,
        "the record lists three references"
    );
    assert_eq!(
        resolved.resolved_count(),
        3,
        "all three references resolve: the 0x15 family decodes"
    );

    for (index, entry) in resolved.textures.iter().enumerate().take(2) {
        let texture = entry.as_ref().unwrap_or_else(|error| {
            panic!("reference {index} of the documented asset must resolve: {error}")
        });
        assert_eq!(texture.envelope.fourcc, spore_material::REQUIRED_FOURCC);
        assert_eq!(
            (texture.image.width, texture.image.height),
            (512, 512),
            "the real records are 512x512"
        );
        assert_eq!(texture.mip_count, 10, "the real records carry ten mips");
        assert_eq!(
            texture.layer_count, 1,
            "ONE chain. The old expectation of 2 came from charging 8 bytes per \
             4x4 block, which split one base mip into two half-pictures."
        );
        assert_eq!(texture.layers.len(), 1);
        assert_eq!(texture.layer(1), None, "there is no second chain");
        assert_eq!(
            texture.channels().len(),
            4,
            "one chain of four lanes; the old 'eight' was two records x two layers"
        );
        assert_eq!(texture.pixel_len(), 512 * 512 * 4);

        // **The colour is real.** This is the refutation, asserted on the asset
        // this repository documents end to end. Every lane takes many values and
        // green and blue are not the constants the old codec produced.
        assert_eq!(texture.role, TextureRole::ColourAndMask);
        assert!(
            texture.statistics.distinct[Lane::Green.index()] > 1,
            "green is not a constant lane: {}",
            texture.statistics.distinct[Lane::Green.index()]
        );
        assert!(
            texture.statistics.distinct[Lane::Blue.index()] > 1,
            "blue is not a constant lane"
        );
        assert!(texture
            .mask_plane_varies()
            .expect("a decoded record has an answer"));
        // And the four channel spellings, none of which may say "degenerate".
        let channels = texture.channels();
        assert!(channels[..3]
            .iter()
            .all(|c| c.role == spore_material::ChannelRole::ColourPlane));
        assert!(channels.iter().all(|c| !c.role.purpose_established()));
    }

    // The third reference is the `0x15` family: a tangent-space normal map,
    // Z stored first. This is the record the old version of this test called
    // 1024x1024 and refused. It is 512x512 and it decodes.
    let normal = resolved.textures[2]
        .as_ref()
        .unwrap_or_else(|error| panic!("the 0x15 reference must resolve: {error}"));
    assert_eq!(
        normal.key,
        ResourceKey::new(RASTER_TYPE, NORMAL_MAP_GROUP, INSTANCE)
    );
    assert_eq!(normal.envelope.fourcc, 0x0000_0015);
    assert_eq!(
        (normal.envelope.width, normal.envelope.height),
        (512, 512),
        "512x512: this package contains no 1024^2 raster of either format"
    );
    assert_eq!(normal.mip_count, 10);
    assert_eq!(
        normal.layer_count, 1,
        "the 0x15 family has one chain and no layer headers"
    );
    assert_eq!(normal.role, TextureRole::TangentSpaceNormal);
    assert_eq!(
        normal.channels()[0].role,
        spore_material::ChannelRole::NormalZ,
        "file byte 0 is Z"
    );
    assert_eq!(
        normal.mask_plane_varies(),
        None,
        "the 0x15 family's fourth lane is a different question, and it is unknown"
    );
    assert!(normal.pixels_decoded());

    // And the model's own view of it: three roles in slot order, an even cut,
    // and two independent statements of every ordinal in agreement.
    assert_eq!(
        resolved.roles(),
        vec![vec![
            TextureRole::ColourAndMask,
            TextureRole::ColourAndMask,
            TextureRole::TangentSpaceNormal,
        ]]
    );
    assert_eq!(
        resolved.scope,
        spore_material::BindingScope::PerMeshBlock { refs_per_mesh: 3 }
    );
    assert_eq!(
        resolved.observed_shape,
        spore_material::TexSetShape::ColourAndMaskTwiceThenNormal
    );
    assert_eq!(
        resolved.observed_shape.as_str(),
        "(colour-and-mask, colour-and-mask, normal)"
    );
    assert!(resolved.ordinal_agreement.all_agree());
    assert_eq!(resolved.ordinal_agreement.compared, 3);

    // One mesh, unnamed, with SIX non-findings. A slot carries no role at all: a
    // role is a property of the resolved record, and this is measured per record
    // -- see `ResolvedTexture::role` and the role_census below.
    //
    // **This count was wrong here and is now enumerated.** It used to say 5,
    // which was a floor written before the third `BindingScope` arm existed:
    // `per_mesh_cut_contradicted_by_record_ordinals` became a sixth non-finding on
    // the `PerMeshBlock` path — "no slot contradicted the cut" — and a bare
    // number cannot notice a subject being added. The subject list is the
    // assertion now; the count follows from it.
    assert_eq!(resolved.meshes.len(), 1);
    let material = &resolved.meshes[0];
    assert_eq!(material.material_id, 0x407D_FDDB);
    assert_eq!(material.name, None, "no registry names this id");
    assert_eq!(material.slots.len(), 3);
    let material_unknowns: Vec<&str> = material
        .unknowns()
        .map(|claim| claim.subject.as_str())
        .collect();
    assert_eq!(
        material_unknowns,
        vec![
            claims::MATERIAL_NAME,
            claims::MATERIAL_ID_MEANING,
            claims::SAMPLER_ROLE,
            claims::MATERIAL_TEXTURE_OWNERSHIP,
            claims::MATERIAL_BLOCK_IS_MESH,
            claims::CUT_CONTRADICTED_BY_RECORD,
        ],
        "six non-findings, enumerated: a floor could not notice the sixth"
    );
    // The sixth is the *non-finding* arm: the record was asked whether the cut was
    // its block boundaries and it agreed, which is not the same as nobody asking.
    let contradiction = material
        .claim(claims::CUT_CONTRADICTED_BY_RECORD)
        .expect("subject");
    assert!(contradiction.fact.is_unavailable());
    assert_eq!(
        contradiction.fact.reason(),
        Some(claims::NO_CUT_CONTRADICTION)
    );
    let material_name = material.claim(claims::MATERIAL_NAME).expect("subject");
    assert!(material_name.fact.is_unavailable());
    assert_eq!(
        material_name.fact.reason(),
        Some(claims::NO_MATERIAL_REGISTRY)
    );

    // The refutations are recorded as values on a real record, not only in the
    // table: a caller reading one `ResolvedTexture` learns what changed.
    let refutation = resolved.roles()[0][0];
    assert_eq!(refutation, TextureRole::ColourAndMask);
    let texture = resolved.textures[0].as_ref().expect("resolves");
    assert!(
        texture
            .claim(claims::DXT5_CARRIES_NO_COLOUR)
            .expect("subject")
            .fact
            .is_available(),
        "'these records carry no colour' is recorded as a REFUTATION, which carries \
         a value; a non-finding would be indistinguishable from never having looked"
    );

    // And the same record read through the single-reference entry point behaves
    // identically: the type word is still a parameter.
    let texture = resolve_texture(
        &store,
        spore_gmdl::GmdlTextureRef {
            instance_id: INSTANCE,
            group_id: 0x4063_2900,
        },
        RASTER_TYPE,
    )
    .expect("the first reference resolves");
    assert_eq!(texture.image.width, 512);
    assert!(
        texture
            .claim(claims::ASSUMED_RECORD_TYPE)
            .expect("subject")
            .fact
            .is_available(),
        "the assumption is recorded, not buried"
    );
}
