//! The material wiring, asserted with **no window, no GPU and no game install**.
//!
//! # Why this file exists
//!
//! "The creature rendered" is a claim about pixels. A pixel claim cannot be made
//! in CI, cannot be diffed in review, and cannot tell a correct normal swizzle
//! from a plausible wrong one — the whole failure mode this repository keeps
//! warning about. So the wiring is asserted here on **texel values and asset
//! collection contents**, and the pixels are left to a screenshot.
//!
//! What is checked:
//!
//! 1. a `TangentSpaceNormal` record lands in `StandardMaterial::normal_map_texture`;
//! 2. the uploaded image is `Rgba8Unorm`, never the sRGB variant;
//! 3. its **blue** lane carries the record's file byte 0, because that byte is
//!    Z — asserted from the numbers, not from the code path that produced them;
//! 4. a `LayeredAlphaMask` record produces one single-channel surface **per
//!    layer**, attached to the entity, and bound to no material slot at all;
//! 5. **no** texture reaches `base_color_texture`, because no raster record in
//!    this corpus decodes as base colour;
//! 6. the mesh carries a tangent frame, because Bevy 0.19 ignores a normal map
//!    entirely without one;
//! 7. two assets in one scene produce two object entities, and each mesh entity
//!    gets its own material.
//!
//! Point 7 is the multi-object claim: the previous revision of this engine held
//! exactly one decoded record and had nowhere to put a second.

use std::sync::Arc;

use bevy::asset::AssetApp;
use bevy::mesh::PrimitiveTopology;
use bevy::pbr::StandardMaterial;
use bevy::prelude::*;
use bevy::render::render_resource::TextureFormat;

use spore_assets::ModelFormat;
use spore_core::ResourceKey;
use spore_material::{ResolvedTexture, TexSetShape, TextureRole};

use spore_engine::convert::{tangents_of, to_bevy_mesh};
use spore_engine::domain::creature::Creature;
use spore_engine::domain::materials::{MaterialRoute, MaterialTexture, MeshSlots, ModelMaterials};
use spore_engine::domain::model::{
    DecodeStatus, ModelDefinition, ModelInstance, ModelPart, Placement,
};
use spore_engine::domain::scene::{Scene, SceneObject};
use spore_engine::loader::{
    LoadedScene, ModelPartMarker, NormalMapPolicy, SceneObjectMarker, NORMAL_MODE,
};
use spore_engine::scene::OnStage;
use spore_engine::texture::NormalMapOrientation;
use spore_engine::{ScenePlugin, SporeAssetPlugin};

const RASTER: u32 = 0x2F4E_681C;

const MASK_A: u32 = 0x4063_2900;
const MASK_B: u32 = 0x4063_2901;
const NORMAL: u32 = 0x4063_2902;

fn mesh(width: f32) -> spore_gmdl::Mesh {
    spore_gmdl::Mesh {
        positions: vec![
            [0.0, 0.0, 0.0],
            [width, 0.0, 0.0],
            [width, 0.0, width],
            [0.0, 0.0, width],
        ],
        normals: vec![[0.0, 1.0, 0.0]; 4],
        uvs: vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
        indices: vec![0, 1, 2, 0, 2, 3],
        topology: spore_gmdl::Topology::TriangleList,
        bounds_min: [0.0, 0.0, 0.0],
        bounds_max: [width, 0.0, width],
        radius: width,
    }
}

/// A synthetic normal map shaped like the measured creature record:
/// Z in byte 0 running 250 … 220, X and Y at 128 (the encoding of zero), and a
/// fourth lane this repository cannot interpret.
fn normal_texture() -> ResolvedTexture {
    let pixels: Vec<u8> = [250u8, 240, 230, 220]
        .into_iter()
        .flat_map(|z| [z, 128, 128, 7])
        .collect();
    resolved(
        NORMAL,
        TextureRole::TangentSpaceNormal,
        pixels.clone(),
        vec![pixels],
    )
}

/// A synthetic two-layer mask: the colour planes collapse to zero, each layer's
/// alpha plane carries distinct full-range values.
fn mask_texture(group: u32, first: u8) -> ResolvedTexture {
    let layer0: Vec<u8> = (0..4u8).flat_map(|i| [0u8, 0, 0, first + i]).collect();
    let layer1: Vec<u8> = (0..4u8).flat_map(|i| [0u8, 0, 0, 200 - i]).collect();
    resolved(
        group,
        TextureRole::LayeredAlphaMask,
        layer0.clone(),
        vec![layer0, layer1],
    )
}

fn resolved(
    group: u32,
    role: TextureRole,
    image_pixels: Vec<u8>,
    layers: Vec<Vec<u8>>,
) -> ResolvedTexture {
    let image = spore_texture::MipImage {
        width: 2,
        height: 2,
        pixels: image_pixels,
    };
    ResolvedTexture {
        key: ResourceKey::new(RASTER, group, 0x067A_07F0),
        kind: spore_material::TextureKind::Raster,
        statistics: spore_material::MipStatistics::of(&image),
        image: image.clone(),
        layers: layers
            .into_iter()
            .map(|pixels| spore_texture::MipImage {
                width: 2,
                height: 2,
                pixels,
            })
            .collect(),
        role,
        envelope: spore_texture::RasterEnvelope {
            version: 1,
            width: 2,
            height: 2,
            mip_count: 1,
            field_10: 8,
            fourcc: 0,
            field_18: 16,
            field_1c: 0,
        },
        mip_count: 1,
        layer_count: 0,
        pixels_decoded: true,
        claims: Vec::new(),
    }
}

/// A one-mesh model with the corpus's `(mask, mask, normal)` set.
fn texture_bearing_model(key: ResourceKey) -> Arc<ModelDefinition> {
    let textures = vec![
        MaterialTexture {
            reference: Some(spore_gmdl::GmdlTextureRef {
                instance_id: 0x067A_07F0,
                group_id: MASK_A,
            }),
            resolved: Some(mask_texture(MASK_A, 10)),
            refusal: None,
        },
        MaterialTexture {
            reference: Some(spore_gmdl::GmdlTextureRef {
                instance_id: 0x067A_07F0,
                group_id: MASK_B,
            }),
            resolved: Some(mask_texture(MASK_B, 20)),
            refusal: None,
        },
        MaterialTexture {
            reference: Some(spore_gmdl::GmdlTextureRef {
                instance_id: 0x067A_07F0,
                group_id: NORMAL,
            }),
            resolved: Some(normal_texture()),
            refusal: None,
        },
    ];
    let materials = ModelMaterials {
        scope: spore_material::BindingScope::PerMeshBlock { refs_per_mesh: 3 },
        meshes: vec![MeshSlots {
            mesh_index: 0,
            material_id: 0x407D_FDDB,
            slots: vec![
                spore_gmdl::GmdlTextureRef {
                    instance_id: 0x067A_07F0,
                    group_id: MASK_A,
                },
                spore_gmdl::GmdlTextureRef {
                    instance_id: 0x067A_07F0,
                    group_id: MASK_B,
                },
                spore_gmdl::GmdlTextureRef {
                    instance_id: 0x067A_07F0,
                    group_id: NORMAL,
                },
            ],
        }],
        route: MaterialRoute::Strict,
        strict_refusal: None,
        observed_shape: Some(TexSetShape::MaskMaskNormal),
        ordinal_agreement: Some(spore_material::OrdinalAgreement {
            compared: 3,
            agreed: 3,
        }),
        textures,
    };
    Arc::new(ModelDefinition {
        key,
        package_name: "mini".into(),
        format: ModelFormat::Gmdl,
        parts: vec![ModelPart {
            mesh_index: 0,
            mesh: mesh(1.0),
            material_id: 0x407D_FDDB,
        }],
        materials: Some(materials),
        paint: no_paint(),
        decode: DecodeStatus::Complete,
    })
}

/// The colour binding a fixture means by "we did not look for a colour": the
/// unavailable variant, never a neutral tint and never black.
fn no_paint() -> spore_engine::domain::paint::PaintBinding {
    spore_engine::domain::paint::PaintBinding::Unavailable {
        reason: spore_engine::domain::paint::PaintUnavailableReason::NoSporemodelRecords,
        fact: spore_core::evidence::Fact::unavailable("test fixture"),
    }
}

/// A one-mesh model whose textures were never requested.
fn bare_model(key: ResourceKey) -> Arc<ModelDefinition> {
    Arc::new(ModelDefinition {
        key,
        package_name: "mini".into(),
        format: ModelFormat::Gmdl,
        parts: vec![ModelPart {
            mesh_index: 0,
            mesh: mesh(1.0),
            material_id: 0,
        }],
        materials: None,
        paint: no_paint(),
        decode: DecodeStatus::Complete,
    })
}

/// A headless app with both plugins, the asset collections they write into, and
/// nothing that opens a window.
///
/// `MinimalPlugins` has no `AssetPlugin`, so the collections are initialised by
/// hand; and `DefaultPlugins` is not used at all because it builds a winit
/// `EventLoop`, which panics off the main thread. "The app assembles" is
/// therefore not the claim — **the wiring is**, and it is asserted below.
fn headless_app(scene: LoadedScene) -> App {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, bevy::asset::AssetPlugin::default()));
    app.init_asset::<Mesh>();
    app.init_asset::<StandardMaterial>();
    app.init_asset::<Image>();
    app.insert_resource(scene);
    app.insert_resource(NormalMapPolicy(NormalMapOrientation::AsStored));
    app.add_plugins((ScenePlugin, SporeAssetPlugin));
    app.finish();
    app
}

fn scene_of(objects: Vec<SceneObject>, models: Vec<Arc<ModelDefinition>>) -> LoadedScene {
    LoadedScene {
        scene: Scene { objects },
        models,
    }
}

#[test]
fn the_normal_map_lands_in_the_normal_slot_with_z_in_blue_and_no_srgb() {
    let key = ResourceKey::new(0x00E6_BCE5, 0x4063_7E03, 0x067A_07F0);
    let scene = scene_of(
        vec![SceneObject::Model(ModelInstance::new(
            key,
            Placement::default(),
        ))],
        vec![texture_bearing_model(key)],
    );
    let mut app = headless_app(scene);
    app.update();

    let mut parts = bevy::ecs::query::QueryState::<
        (Entity, &ModelPartMarker, &MeshMaterial3d<StandardMaterial>),
        With<OnStage>,
    >::new(&mut *app.world_mut());
    parts.update_archetypes(app.world());
    let world = app.world();
    let images = world.resource::<Assets<Image>>();
    let materials = world.resource::<Assets<StandardMaterial>>();

    let (entity, marker, handle) = parts
        .iter(world)
        .next()
        .expect("one mesh entity must be on stage");
    assert_eq!(marker.key, key);
    assert_eq!(marker.mesh_index, 0);
    assert_eq!(marker.material_id, 0x407D_FDDB);

    let material = materials
        .get(&handle.0)
        .expect("the material must be in the collection");

    // 5. Nothing is applied as base colour.
    assert!(
        material.base_color_texture.is_none(),
        "no raster record in this corpus decodes as base colour"
    );
    assert!(material.metallic_roughness_texture.is_none());

    // 1. The normal map IS in the normal slot.
    let normal_handle = material
        .normal_map_texture
        .clone()
        .expect("a tangent-space-normal record must bind as the normal map");

    // 2. Never sRGB.
    let image = images
        .get(&normal_handle)
        .expect("the image must be uploaded");
    assert_eq!(
        image.texture_descriptor.format,
        TextureFormat::Rgba8Unorm,
        "an sRGB decode applies a transfer curve to a linear direction"
    );

    // 3. The texel values. File byte 0 is Z; Z must be in blue.
    let bytes = image.data.as_ref().expect("base mip data");
    assert_eq!(bytes.len(), 4 * 2 * 2);
    for (index, texel) in bytes.chunks_exact(4).enumerate() {
        let file_z = [250u8, 240, 230, 220][index];
        assert_eq!(
            texel[2], file_z,
            "Z (file byte 0) must land in BLUE; a wrong swizzle looks almost right"
        );
        assert_eq!(texel[0], 128, "X must land in red");
        assert_eq!(texel[1], 128, "Y must land in green");
    }

    // 6. The mesh carries a tangent frame, or Bevy ignores the map entirely.
    let mesh_handle = world.get::<Mesh3d>(entity).expect("a mesh").0.clone();
    let mesh = world
        .resource::<Assets<Mesh>>()
        .get(&mesh_handle)
        .expect("the mesh must be in the collection");
    let tangents = tangents_of(mesh).expect("a normal map needs a tangent frame");
    assert_eq!(tangents.len(), 4);
    assert!(
        tangents[0][0].abs() > 0.9,
        "T should run along +X: {tangents:?}"
    );
}

#[test]
fn a_mask_record_produces_one_single_channel_surface_per_layer_and_binds_nothing() {
    let key = ResourceKey::new(0x00E6_BCE5, 0x4063_7E03, 0x067A_07F0);
    let scene = scene_of(
        vec![SceneObject::Model(ModelInstance::new(
            key,
            Placement::default(),
        ))],
        vec![texture_bearing_model(key)],
    );
    let mut app = headless_app(scene);
    app.update();

    // Every query is built before the world is borrowed, because
    // `QueryState::new` needs `&mut World` and `iter` needs `&World`.
    let mut parts = bevy::ecs::query::QueryState::<(Entity, &ModelPartMarker), With<OnStage>>::new(
        &mut *app.world_mut(),
    );
    let mut bindings = bevy::ecs::query::QueryState::<
        &spore_engine::loader::MaskChannels,
        With<OnStage>,
    >::new(&mut *app.world_mut());
    let mut owned =
        bevy::ecs::query::QueryState::<Entity, With<spore_engine::loader::MaskChannels>>::new(
            &mut *app.world_mut(),
        );
    parts.update_archetypes(app.world());
    bindings.update_archetypes(app.world());
    owned.update_archetypes(app.world());

    let world = app.world();
    let images = world.resource::<Assets<Image>>();
    let (entity, _) = parts.iter(world).next().expect("one mesh entity");

    // 4. Four planes: two records x two layers. `MaskChannels` is the only place
    //    they are named, and it names them positionally. A per-plane component
    //    would have left one of the four, because `insert` replaces.
    let mut planes: Vec<(ResourceKey, usize, Handle<Image>)> = bindings
        .iter(world)
        .flat_map(|channels| channels.planes.iter())
        .map(|binding| (binding.key, binding.layer_index, binding.handle.clone()))
        .collect();
    assert_eq!(planes.len(), 4, "two records x two layers");
    planes.sort_by_key(|(key, layer, _)| (key.group_id, *layer));
    assert_eq!(
        planes
            .iter()
            .map(|(key, layer, _)| (key.group_id, *layer))
            .collect::<Vec<_>>(),
        vec![(MASK_A, 0), (MASK_A, 1), (MASK_B, 0), (MASK_B, 1)]
    );
    for (_, _, handle) in &planes {
        let image = images.get(handle).expect("every plane is uploaded");
        assert_eq!(
            image.texture_descriptor.format,
            TextureFormat::R8Unorm,
            "one channel, one value; RGB would invent a colour"
        );
    }

    // The planes carry the ALPHA lane of each layer, nothing else. MASK_A's
    // layer 0 starts at 10 and layer 1 at 200.
    let first = images.get(&planes[0].2).unwrap();
    let second = images.get(&planes[1].2).unwrap();
    assert_eq!(first.data.as_ref().unwrap().as_slice(), &[10, 11, 12, 13]);
    assert_eq!(
        second.data.as_ref().unwrap().as_slice(),
        &[200, 199, 198, 197]
    );

    // And the binding is on the mesh entity that owns it.
    let owners: Vec<Entity> = owned.iter(world).collect();
    assert_eq!(owners, vec![entity]);
}

#[test]
fn two_assets_in_one_scene_produce_two_objects_and_two_materials() {
    let textured = ResourceKey::new(0x00E6_BCE5, 0x4063_7E03, 0x067A_07F0);
    let bare = ResourceKey::new(0x00E6_BCE5, 0x4062_7100, 0x067C_79D2);
    let offset = Placement {
        translation: [5.0, 0.0, 0.0],
        ..Placement::default()
    };
    let scene = scene_of(
        vec![
            SceneObject::Model(ModelInstance::new(textured, Placement::default())),
            // A creature: the shape an engine needs for a multi-record one.
            SceneObject::Creature(Creature::single(ModelInstance::new(bare, offset))),
        ],
        vec![texture_bearing_model(textured), bare_model(bare)],
    );
    let mut app = headless_app(scene);
    app.update();

    let mut objects =
        bevy::ecs::query::QueryState::<Entity, With<SceneObjectMarker>>::new(&mut *app.world_mut());
    let mut parts = bevy::ecs::query::QueryState::<
        (Entity, &ModelPartMarker, &MeshMaterial3d<StandardMaterial>),
        With<OnStage>,
    >::new(&mut *app.world_mut());
    let mut distinct = bevy::ecs::query::QueryState::<
        Entity,
        (With<MeshMaterial3d<StandardMaterial>>, With<OnStage>),
    >::new(&mut *app.world_mut());
    let mut parents =
        bevy::ecs::query::QueryState::<&ChildOf, With<Mesh3d>>::new(&mut *app.world_mut());
    objects.update_archetypes(app.world());
    parts.update_archetypes(app.world());
    distinct.update_archetypes(app.world());
    parents.update_archetypes(app.world());

    let world = app.world();
    let materials = world.resource::<Assets<StandardMaterial>>();

    let mut kinds: Vec<&'static str> = objects
        .iter(world)
        .map(|entity| world.get::<SceneObjectMarker>(entity).unwrap().kind)
        .collect();
    kinds.sort_unstable();
    assert_eq!(kinds, vec!["creature", "model"]);

    let found: Vec<(ResourceKey, bool)> = parts
        .iter(world)
        .map(|(_, marker, handle)| {
            (
                marker.key,
                materials
                    .get(&handle.0)
                    .expect("a material")
                    .normal_map_texture
                    .is_some(),
            )
        })
        .collect();
    assert_eq!(found.len(), 2);
    assert!(
        found.contains(&(textured, true)),
        "the textured asset binds a normal map"
    );
    assert!(
        found.contains(&(bare, false)),
        "an asset whose textures were never requested binds none"
    );

    // Two leaf entities, two distinct materials, and the placement lives on the
    // object parent rather than on the leaves.
    let entities: Vec<Entity> = distinct.iter(world).collect();
    assert_eq!(entities.len(), 2);
    let parent_of: Vec<Entity> = parents
        .iter(world)
        .map(|child_of| child_of.parent())
        .collect();
    assert_eq!(parent_of.len(), 2);
    for parent in parent_of {
        assert!(
            world.get::<SceneObjectMarker>(parent).is_some(),
            "and that parent is a scene object"
        );
    }
}

#[test]
fn an_asset_with_no_meshes_is_reported_rather_than_spawned_as_nothing() {
    // A record that decoded no geometry must not produce an entity that looks
    // like a failed load. `LoadedScene::bounds` is `None`, the outcome says
    // `parts = 0`, and nothing is spawned.
    let key = ResourceKey::new(0x00E6_BCE5, 0x4063_7E03, 0x067A_07F0);
    let mut model = (*texture_bearing_model(key)).clone();
    model.parts.clear();
    let scene = scene_of(
        vec![SceneObject::Model(ModelInstance::new(
            key,
            Placement::default(),
        ))],
        vec![Arc::new(model)],
    );
    let mut app = headless_app(scene);
    app.update();
    let mut staged =
        bevy::ecs::query::QueryState::<Entity, With<OnStage>>::new(&mut *app.world_mut());
    staged.update_archetypes(app.world());
    assert_eq!(staged.iter(app.world()).count(), 0);
    drop(staged);
    let outcome = app.world().resource::<spore_engine::loader::SceneOutcome>();
    assert_eq!(outcome.parts, 0);
    assert_eq!(outcome.triangles, 0);
    assert_eq!(outcome.bounds, None);
}

#[test]
fn the_staged_mesh_carries_uvs_normals_and_tangents_so_the_map_has_a_frame() {
    // The staging path itself, with no app: a mesh bound for a normal map has to
    // arrive at the GPU with positions, UVs, normals AND tangents. Bevy 0.19
    // compiles `apply_normal_mapping` only under `VERTEX_TANGENTS`.
    let buffers = spore_engine::convert::to_buffers(&mesh(1.0));
    let bevy = to_bevy_mesh(&buffers, NORMAL_MODE);
    assert!(bevy.attribute(Mesh::ATTRIBUTE_POSITION).is_some());
    assert!(bevy.attribute(Mesh::ATTRIBUTE_UV_0).is_some());
    assert!(bevy.attribute(Mesh::ATTRIBUTE_NORMAL).is_some());
    let tangents = tangents_of(&bevy).expect("tangents");
    assert_eq!(tangents.len(), 4);
    assert_eq!(bevy.primitive_topology(), PrimitiveTopology::TriangleList);
}
