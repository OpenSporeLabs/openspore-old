//! The creator projection, asserted against entities.
//!
//! Everything here is hermetic: synthetic records, synthetic `<sporemodel>` XML,
//! `MinimalPlugins`, no GPU, no window and no game install. The one exception is
//! the last test, which is `#[ignore]`d and skips cleanly without `SPORE/`.
//!
//! # What is being pinned
//!
//! | claim | where |
//! |---|---|
//! | one entity per resolved block, parented as the document nests | `a_three_block_document_becomes_three_entities_with_its_parentage` |
//! | the **composed** world transform equals the projection's `world_*` | `the_composed_world_transform_is_the_projections_world_transform` |
//! | an unresolved block spawns nothing but still has a reported reason | `an_unresolved_block_spawns_nothing_and_is_still_reported` |
//! | save → reload → re-project is stable | `a_saved_document_reloads_and_reprojects_identically` |
//! | one camera, owned by `ScenePlugin` | `the_creator_mode_owns_no_camera` |
//! | readiness, so automation need not sleep | `the_scene_reports_stability_before_it_reports_itself_ready` |
//! | the vertical slice is unregressed | `the_documented_asset_preset_is_unchanged` |
//!
//! The parentage claim is the one worth reading twice. Each entity carries its
//! block's **local** transform and is a child of its authored parent's entity, so
//! Bevy's own `parent_world * child_local` composition produces the projection's
//! `world_position`. Writing `world_position` onto the child *and* parenting it
//! would apply the parent chain twice, and the outermost part is the only one that
//! would look wrong -- so the assertion is on the composed `GlobalTransform`, not
//! on the local `Transform`.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use spore_core::ResourceKey;
use spore_creator_core::{parse_sporemodel, persist, CreatorDocument, Family};
use spore_engine::cli::{self, CliError, CreatorSource, LaunchRequest};
use spore_engine::creator::{
    self, block_name, CreatorBlockMarker, CreatorDecoded, CreatorRootMarker, CreatorRuntimeState,
    DocumentSource, ROOT_NAME,
};
use spore_engine::loader::orientation_to_quat;
use spore_engine::scene::{MainCamera, ScenePlugin};
use spore_engine::{CreatorPlugin, StagedCreator};

use spore_morphology::resolve::{GROUP_PART, TYPE_GMSH, TYPE_PROP, TYPE_RIGBLOCK, TYPE_RW4};
use spore_morphology::{MemorySource, RecordSource};

/// The part group every fixture identity lives in.
const GROUP: u32 = 0x4062_6000;

/// One part identity, used by all three fixture blocks.
const INSTANCE: u32 = 0x0001_3530;

// ---------------------------------------------------------------------------
// The app under test
// ---------------------------------------------------------------------------

/// An app with the creator plugin, the world plugin, and no window.
///
/// `MinimalPlugins` deliberately has no `AssetPlugin`, so the asset collections
/// are initialised by hand -- the same thing `scene.rs`'s tests do.
/// `TransformPlugin` is added because it is the whole point: it is what composes a
/// child's world transform out of its parent's, and without it a `GlobalTransform`
/// stays at the local value and the central assertion below would pass for the
/// wrong reason. Neither needs a GPU or a window.
fn creator_app(stage: creator::CreatorStage, selected: Option<usize>, scale: f32) -> App {
    let state = stage.runtime_state(selected);
    let framing = stage.camera_framing();

    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        bevy::asset::AssetPlugin::default(),
        // `TransformPlugin` is what composes a child's world transform out of its
        // parent's. Without it a `GlobalTransform` keeps the local value, and the
        // central assertion below would pass for the wrong reason. It needs no GPU
        // and no window.
        bevy::transform::TransformPlugin,
    ));
    app.init_asset::<Mesh>();
    app.init_asset::<StandardMaterial>();
    // The world first, then the content: the same order `build_creator_app` uses.
    app.add_plugins(ScenePlugin);
    app.insert_resource(StagedCreator {
        stage,
        selected,
        scale,
    });
    app.insert_resource(state);
    if let Some(framing) = framing {
        app.insert_resource(framing);
    }
    app.add_plugins(CreatorPlugin);
    app.finish();
    app.cleanup();
    app
}

/// A fresh [`QueryState`] over an app's world.
///
/// Bevy 0.19's `QueryState::new` is the only constructor, so every query in this
/// file goes through here rather than through `World::query`, which is not a
/// public API. It also means the archetype set is updated before iteration, which
/// is what makes a query written *after* the spawn see the spawned entities.
fn query_of<Q: bevy::ecs::query::QueryData, F: bevy::ecs::query::QueryFilter>(
    app: &mut App,
) -> bevy::ecs::query::QueryState<Q, F> {
    let mut state = bevy::ecs::query::QueryState::<Q, F>::new(app.world_mut());
    state.update_archetypes(app.world_mut());
    state
}

/// The largest absolute element of two matrices' difference.
///
/// Written out rather than reaching for `max_element` because glam has no such
/// method on `Mat3`, and because a comparison with an explicit tolerance is a
/// claim about float arithmetic rather than an exact-equality assertion that would
/// only ever be satisfied by accident.
fn max_matrix_diff(left: Mat3, right: Mat3) -> f32 {
    left.to_cols_array()
        .iter()
        .zip(right.to_cols_array().iter())
        .map(|(a, b)| (a - b).abs())
        .fold(0.0f32, f32::max)
}

/// How many entities match a query.
fn count<Q: bevy::ecs::query::QueryData, F: bevy::ecs::query::QueryFilter>(app: &mut App) -> usize {
    let mut state = query_of::<Q, F>(app);
    let found = state.iter(app.world_mut()).count();
    drop(state);
    found
}

// ---------------------------------------------------------------------------
// The fixtures
// ---------------------------------------------------------------------------

/// One authored block, as a test fixture names it.
struct BlockSpec {
    /// The rigblock instance every hop starts at.
    instance: u32,
    /// `<position>`.
    position: [f32; 3],
    /// `<scale>`.
    scale: f32,
    /// `<orientation>`, row-major.
    orientation: [[f32; 3]; 3],
    /// `<childlist>`.
    children: &'static [usize],
}

/// A `<sporemodel>` document from a list of blocks.
///
/// Written in the corpus's own grammar rather than built as a struct, because
/// `parse_sporemodel` is the path a real document takes and a struct literal would
/// test the projection against something the parser never produces.
fn document(blocks: &[BlockSpec]) -> CreatorDocument {
    let body: String = blocks
        .iter()
        .map(
            |BlockSpec {
                 instance,
                 position,
                 scale,
                 orientation: rows,
                 children,
             }| {
                let childlist = if children.is_empty() {
                    String::new()
                } else {
                    let ids: String = children
                        .iter()
                        .map(|child| format!("<childid>{child}</childid>"))
                        .collect();
                    format!("<childlist count=\"{}\">{ids}</childlist>", children.len())
                };
                format!(
                    "<blockref><blockid>0x{GROUP:08x}, 0x{instance:08x}</blockid><transform>\
                 <position>{},{},{}</position><scale>{scale}</scale><orientation>\
                 <row0>{},{},{}</row0><row1>{},{},{}</row1><row2>{},{},{}</row2>\
                 </orientation></transform>{childlist}</blockref>",
                    position[0],
                    position[1],
                    position[2],
                    rows[0][0],
                    rows[0][1],
                    rows[0][2],
                    rows[1][0],
                    rows[1][1],
                    rows[1][2],
                    rows[2][0],
                    rows[2][1],
                    rows[2][2],
                )
            },
        )
        .collect();
    let xml = format!(
        "<sporemodel><formatversion>16</formatversion><blocks count=\"{}\">{body}</blocks>\
         </sporemodel>",
        blocks.len()
    );
    parse_sporemodel(Family::Creature, &xml).expect("the fixture document parses")
}

/// The row-major matrix for a right-handed rotation of `degrees` about +Y.
fn rotation_y(degrees: f32) -> [[f32; 3]; 3] {
    let (sin, cos) = (degrees.to_radians().sin(), degrees.to_radians().cos());
    [[cos, 0.0, sin], [0.0, 1.0, 0.0], [-sin, 0.0, cos]]
}

const IDENTITY: [[f32; 3]; 3] = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];

/// The three-block forest every entity test uses.
///
/// ```text
///   0  pos (1,0,0)  scale 1  identity      child of: -
///   1  pos (0,2,0)  scale 2  rot +90 about Y  child of: 0
///   2  pos (0,0,3)  scale 1  identity      child of: 1
/// ```
///
/// Deliberately not symmetric: a tree of siblings would make a parentage bug
/// invisible, because every entity would parent to the root and still land in the
/// right place. Here block 2's world position depends on block 1's scale *and* its
/// rotation, so a dropped parent, a dropped scale and a dropped rotation each fail
/// a different assertion.
fn three_block_document() -> CreatorDocument {
    document(&[
        BlockSpec {
            instance: INSTANCE,
            position: [1.0, 0.0, 0.0],
            scale: 1.0,
            orientation: IDENTITY,
            children: &[1],
        },
        BlockSpec {
            instance: INSTANCE,
            position: [0.0, 2.0, 0.0],
            scale: 2.0,
            orientation: rotation_y(90.0),
            children: &[2],
        },
        BlockSpec {
            instance: INSTANCE,
            position: [0.0, 0.0, 3.0],
            scale: 1.0,
            orientation: IDENTITY,
            children: &[],
        },
    ])
}

/// A record source with the whole resolver chain for [`INSTANCE`], plus a second
/// part identity that resolves to nothing.
///
/// The byte layouts are the ones `spore-morphology`'s hermetic resolver tests use,
/// so the chain walked here is the chain walked there.
fn chain_source() -> MemorySource {
    let mut source = MemorySource::new();

    // rigblock: `k = 0`, so a 36-byte header and no cells.
    let mut rigblock = vec![0u8; 36];
    for axis in 0..6 {
        rigblock[4 * axis..4 * axis + 4].copy_from_slice(&1.0f32.to_le_bytes());
    }
    source.insert(ResourceKey::new(TYPE_RIGBLOCK, GROUP, INSTANCE), rigblock);

    // prop: one validated 16-byte pointer entry.
    let mut prop = Vec::new();
    prop.extend_from_slice(&INSTANCE.to_le_bytes());
    prop.extend_from_slice(&TYPE_RW4.to_le_bytes());
    prop.extend_from_slice(&GROUP_PART.to_le_bytes());
    prop.extend_from_slice(&0x00A0_0CE4u32.to_le_bytes());
    source.insert(ResourceKey::new(TYPE_PROP, GROUP, INSTANCE), prop);

    // An empty container: the part resolves and its skeleton is `None`, which is
    // the real corpus state of 256 of 1824 containers.
    source.insert(ResourceKey::new(TYPE_RW4, GROUP_PART, INSTANCE), Vec::new());
    source.insert(
        ResourceKey::new(TYPE_GMSH, GROUP_PART, INSTANCE),
        gmsh_record(),
    );
    source
}

/// A record source holding nothing, so every block stops at its first hop.
fn empty_source() -> MemorySource {
    MemorySource::new()
}

/// A minimal real `gmsh` record: 3 vertices, 1 triangle, 1 bone.
fn gmsh_record() -> Vec<u8> {
    let mut out = vec![0u8; 0x18];
    out[0x00..0x04].copy_from_slice(&0x0100_0000u32.to_le_bytes());
    out[0x04..0x08].copy_from_slice(&0x0100_0000u32.to_le_bytes());
    out[0x08..0x0C].copy_from_slice(&0x0000_0400u32.to_le_bytes());
    // Recipe 0x00010300 declares a 3-block chain, so three blocks follow.
    out[0x0C..0x10].copy_from_slice(&0x0001_0300u32.to_le_bytes());
    out[0x10..0x14].copy_from_slice(&3u32.to_le_bytes());
    // `nv` is a `u16` big-endian inside header word 0x14. Every other word of a
    // `gmsh` block is big-endian too, and only the 24-byte header is little-endian.
    out[0x14..0x16].copy_from_slice(&3u16.to_be_bytes());
    out[0x16] = 0x00;
    out[0x17] = 0x0C;
    for corner in [[0.0f32, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]] {
        for value in corner {
            out.extend_from_slice(&value.to_le_bytes());
        }
    }
    out.extend_from_slice(&0x0000_0001u32.to_be_bytes());
    out.extend_from_slice(&3u32.to_be_bytes());
    out.extend_from_slice(&2u16.to_be_bytes());
    for index in [0u16, 1, 2] {
        out.extend_from_slice(&index.to_le_bytes());
    }
    out.extend_from_slice(&0x1500_0608u32.to_be_bytes());
    out.extend_from_slice(&1u32.to_be_bytes());
    out.extend_from_slice(&4u16.to_be_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&0x0A00_0400u32.to_be_bytes());
    out.extend_from_slice(&3u32.to_be_bytes());
    out.extend_from_slice(&16u16.to_be_bytes());
    for _ in 0..3 {
        for value in [0.0f32, 0.0, 0.0, 1.0] {
            out.extend_from_slice(&value.to_le_bytes());
        }
    }
    out.extend_from_slice(&3u32.to_be_bytes());
    out.push(2);
    out.push(0);
    for index in 0..2u8 {
        out.push(index);
        out.extend_from_slice(&[0xFF, 0xFF, 0x00]);
    }
    out.extend_from_slice(&1u32.to_be_bytes());
    out.extend_from_slice(&[4, 0, 0, 0]);
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&3u32.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&4u32.to_le_bytes());
    out.extend_from_slice(&0u32.to_be_bytes());
    out.extend_from_slice(&31u16.to_be_bytes());
    out.extend_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
    out.extend_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
    out
}

/// Projects a document with the engine's own entry point.
fn project(document: &CreatorDocument, source: &dyn RecordSource) -> creator::CreatorStage {
    creator::project_document(
        document.clone(),
        &DocumentSource::for_record(ResourceKey::new(Family::Creature.type_id(), 1, 2))
            .expect("creature is a known family"),
        source,
        0,
    )
    .expect("the document and its composition agree")
}

// ---------------------------------------------------------------------------
// 1. Three blocks, three entities, the right parentage
// ---------------------------------------------------------------------------

#[test]
fn a_three_block_document_becomes_three_entities_with_its_parentage() {
    let stage = project(&three_block_document(), &chain_source());
    assert_eq!(stage.block_count(), 3);
    assert_eq!(stage.projection.counts().resolved, 3);

    let mut app = creator_app(stage, None, 1.0);
    app.update();

    let mut state = query_of::<&CreatorBlockMarker, ()>(&mut app);
    let mut markers: Vec<(u32, Option<u32>, u32)> = state
        .iter(app.world_mut())
        .map(|marker| (marker.block, marker.parent, marker.ancestor_depth))
        .collect();
    drop(state);
    markers.sort_unstable();
    assert_eq!(
        markers,
        vec![(0, None, 0), (1, Some(0), 1), (2, Some(1), 2)],
        "one entity per resolved block, parented exactly as the document nests"
    );
}

#[test]
fn the_parent_relationship_is_maintained_on_both_sides_of_the_hierarchy() {
    // Bevy's invariant: `ChildOf` on the child alone is not enough, and its own
    // warning says a one-sided relationship "will cause inconsistent behaviors".
    // This asserts the engine used `add_child` -- that BOTH sides name the same
    // entities -- rather than asserting that it "should have".
    let stage = project(&three_block_document(), &chain_source());
    let mut app = creator_app(stage, None, 1.0);
    app.update();

    // Which entity is which block, by name. Reading identities off the projection
    // rather than off entity indices keeps the assertions below about the graph.
    let mut state = query_of::<(Entity, &CreatorBlockMarker), ()>(&mut app);
    let mut entity_of: Vec<(u32, Entity)> = state
        .iter(app.world_mut())
        .map(|(entity, marker)| (marker.block, entity))
        .collect();
    drop(state);
    entity_of.sort_by_key(|(block, _)| *block);
    let table = entity_of;
    let entity_of = |block: u32| table[block as usize].1;
    let block_of = |entity: Entity| {
        table
            .iter()
            .find(|(_, candidate)| *candidate == entity)
            .map(|(block, _)| *block)
    };

    // The parent's side. Only a parent carries `Children`, so the query finds two of
    // the three block entities -- and that is part of the claim: block 2 is a leaf.
    let mut state = query_of::<(&CreatorBlockMarker, &Children), ()>(&mut app);
    let mut rows: Vec<(u32, Vec<Entity>)> = state
        .iter(app.world_mut())
        .map(|(marker, children)| (marker.block, children.to_vec()))
        .collect();
    drop(state);
    rows.sort_by_key(|(block, _)| *block);
    assert_eq!(
        rows.iter().map(|(block, _)| *block).collect::<Vec<_>>(),
        vec![0, 1],
        "only blocks 0 and 1 are parents; block 2 is the leaf"
    );
    assert_eq!(
        rows[0].1,
        vec![entity_of(1)],
        "block 0 parents exactly block 1"
    );
    assert_eq!(
        rows[1].1,
        vec![entity_of(2)],
        "block 1 parents exactly block 2"
    );

    // The child's side, on the entities themselves.
    let mut state = query_of::<(&CreatorBlockMarker, &ChildOf), ()>(&mut app);
    let mut parents: Vec<(u32, Entity)> = state
        .iter(app.world_mut())
        .map(|(marker, child_of)| (marker.block, child_of.parent()))
        .collect();
    drop(state);
    parents.sort_by_key(|(block, _)| *block);
    assert_eq!(
        parents.len(),
        3,
        "every block entity is a child of something"
    );
    assert_eq!(
        block_of(parents[1].1),
        Some(0),
        "block 1's `ChildOf` names block 0's entity"
    );
    assert_eq!(
        block_of(parents[2].1),
        Some(1),
        "block 2's `ChildOf` names block 1's entity"
    );

    // And block 0 hangs off the one creature root, which is not a block at all.
    let mut state = query_of::<Entity, With<CreatorRootMarker>>(&mut app);
    let root = state.single(app.world_mut()).expect("one root");
    drop(state);
    assert_eq!(
        block_of(parents[0].1),
        None,
        "block 0 is a root, so its parent is the creature root rather than a block"
    );
    assert_eq!(parents[0].1, root);
    let mut state = query_of::<(&CreatorRootMarker, &Children), ()>(&mut app);
    let root_children: Vec<Entity> = state.single(app.world_mut()).expect("one root").1.to_vec();
    drop(state);
    assert_eq!(root_children, vec![entity_of(0)]);
}

#[test]
fn every_block_entity_is_named_so_an_agent_can_address_it() {
    // `world.find_entities_by_name` returned 0 against 241 live entities before
    // this path set any names. A `Name` is not reflectable in Bevy 0.19, so this
    // is purely about addressability.
    let stage = project(&three_block_document(), &chain_source());
    let mut app = creator_app(stage, None, 1.0);
    app.update();

    let mut state = query_of::<&Name, ()>(&mut app);
    let names: Vec<String> = state
        .iter(app.world_mut())
        .map(|name| name.to_string())
        .collect();
    drop(state);
    assert!(names.contains(&ROOT_NAME.to_owned()), "{names:?}");
    for index in 0..3 {
        assert!(
            names.contains(&block_name(index)),
            "block {index} must be addressable: {names:?}"
        );
    }
}

#[test]
fn the_creator_mode_owns_no_camera() {
    // One owner per entity kind. `ScenePlugin` owns the camera; `CreatorPlugin`
    // frames it. Two owners is how a framing bug once wore a framing bug's clothes.
    let stage = project(&three_block_document(), &chain_source());
    let mut app = creator_app(stage, None, 1.0);
    app.update();
    app.update();

    assert_eq!(
        count::<Entity, With<Camera3d>>(&mut app),
        1,
        "exactly one camera"
    );
    assert_eq!(count::<Entity, With<MainCamera>>(&mut app), 1);
}

#[test]
fn the_camera_is_framed_away_from_its_default_position() {
    let stage = project(&three_block_document(), &chain_source());
    let mut app = creator_app(stage, None, 1.0);
    app.update();
    app.update();

    let mut state = query_of::<&mut Transform, With<MainCamera>>(&mut app);
    let transform = *state.single(app.world_mut()).expect("one camera");
    drop(state);
    // `ScenePlugin` parks it at (0, 6, 34) looking at the origin; framing moves it
    // to the projection's own centre at its own distance.
    assert!(
        (transform.translation - Vec3::new(0.0, 6.0, 34.0)).length() > 1e-3,
        "the camera was never framed: {transform:?}"
    );
}

// ---------------------------------------------------------------------------
// 2. The world transform is the projection's world transform
// ---------------------------------------------------------------------------

#[test]
fn the_composed_world_transform_is_the_projections_world_transform() {
    let stage = project(&three_block_document(), &chain_source());
    let projected: Vec<(Vec3, bevy::math::Quat, f32)> = stage
        .projection
        .blocks()
        .iter()
        .map(|block| {
            (
                Vec3::new(
                    block.world_position.x,
                    block.world_position.y,
                    block.world_position.z,
                ),
                orientation_to_quat(block.world_orientation),
                block.local_scale,
            )
        })
        .collect();

    let mut app = creator_app(stage, None, 1.0);
    app.update();

    let mut state = query_of::<(&CreatorBlockMarker, &GlobalTransform), ()>(&mut app);
    let mut seen: Vec<(u32, Vec3, Quat)> = state
        .iter(app.world_mut())
        .map(|(marker, global)| (marker.block, global.translation(), global.rotation()))
        .collect();
    drop(state);
    seen.sort_by_key(|(block, _, _)| *block);
    assert_eq!(seen.len(), 3);

    for (block, translation, rotation) in seen {
        let (position, wanted, _) = projected[block as usize];
        assert!(
            (translation - position).length() < 1e-4,
            "block {block} landed at {translation:?}, not the projection's {position:?}. \
             A local transform with no parentage would also pass a per-block check, so \
             this is asserted on the composed global transform."
        );
        // Comparing the matrices the quaternions produce is sign-agnostic, which is
        // right: q and -q are the same rotation.
        assert!(
            max_matrix_diff(Mat3::from_quat(rotation), Mat3::from_quat(wanted)) < 1e-4,
            "block {block} is rotated {rotation:?}, not by the projection's orientation"
        );
    }
}

#[test]
fn a_childs_world_position_folds_in_its_parents_scale_and_rotation() {
    // Worked out by hand from the fixture, so the number cannot come from the code
    // under test:
    //   block 1: parent 0 at (1,0,0) scale 1  ->  (1,0,0) + R0*(1*(0,2,0)) = (1,2,0)
    //   block 2: parent 1 at (1,2,0) scale 2, rotated +90 about Y
    //            ->  (1,2,0) + R_y(90)*(2*(0,0,3)) = (1,2,0) + (6,0,0) = (7,2,0)
    let stage = project(&three_block_document(), &chain_source());
    let expected = [
        Vec3::new(1.0, 0.0, 0.0),
        Vec3::new(1.0, 2.0, 0.0),
        Vec3::new(7.0, 2.0, 0.0),
    ];
    let wanted: Vec<[f32; 3]> = stage
        .projection
        .blocks()
        .iter()
        .map(|block| block.world_position.to_array())
        .collect();
    for (index, want) in expected.iter().enumerate() {
        let got = wanted[index];
        let got = Vec3::new(got[0], got[1], got[2]);
        assert!(
            (*want - got).length() < 1e-4,
            "block {index} composed to {got:?}, not {want:?}"
        );
    }
}

#[test]
fn a_world_scale_multiplies_the_whole_creature_without_touching_the_projection() {
    let stage = project(&three_block_document(), &chain_source());
    let plain = stage
        .projection
        .blocks()
        .iter()
        .map(|block| block.world_position)
        .collect::<Vec<_>>();
    let mut app = creator_app(stage, None, 3.0);
    app.update();

    let mut state = query_of::<(&CreatorBlockMarker, &GlobalTransform), ()>(&mut app);
    let mut scaled: Vec<(u32, Vec3)> = state
        .iter(app.world_mut())
        .map(|(marker, global)| (marker.block, global.translation()))
        .collect();
    drop(state);
    scaled.sort_by_key(|(block, _)| *block);
    for (block, translation) in scaled {
        let base = plain[block as usize];
        let want = Vec3::new(base.x * 3.0, base.y * 3.0, base.z * 3.0);
        assert!(
            (translation - want).length() < 1e-3,
            "block {block} is at {translation:?}, not the projection scaled by 3: {want:?}"
        );
    }
}

#[test]
fn a_general_orientation_reaches_the_transform_as_a_matrix_not_a_y_rotation() {
    // Block 1's orientation is a +90 degree turn about Y. Collapsing it to "some Y
    // rotation" would be indistinguishable here, so this asserts the arms of
    // `Orientation` agree *through the projection*, which is the property the
    // creator path depends on: a `General` must not silently become a different
    // rotation than the matrix it came from.
    let document = document(&[
        BlockSpec {
            instance: INSTANCE,
            position: [0.0, 0.0, 0.0],
            scale: 1.0,
            orientation: rotation_y(37.0),
            children: &[],
        },
        BlockSpec {
            instance: INSTANCE,
            position: [0.0, 0.0, 0.0],
            scale: 1.0,
            orientation: rotation_y(90.0),
            children: &[],
        },
    ]);
    let stage = project(&document, &chain_source());
    let mut app = creator_app(stage, None, 1.0);
    app.update();

    let mut state = query_of::<(&CreatorBlockMarker, &Transform), ()>(&mut app);
    let mut locals: Vec<(u32, Quat)> = state
        .iter(app.world_mut())
        .map(|(marker, transform)| (marker.block, transform.rotation))
        .collect();
    drop(state);
    locals.sort_by_key(|(block, _)| *block);

    // Two different authored angles, so a "one rotation for the whole document"
    // bug cannot pass.
    for (block, expected_rows) in [(0u32, rotation_y(37.0)), (1u32, rotation_y(90.0))] {
        let want = orientation_to_quat(spore_core::Orientation::General(spore_core::Matrix3 {
            row0: expected_rows[0],
            row1: expected_rows[1],
            row2: expected_rows[2],
        }));
        let got = locals[block as usize].1;
        assert!(
            max_matrix_diff(Mat3::from_quat(got), Mat3::from_quat(want)) < 1e-4,
            "block {block} lost its authored orientation: {got:?} vs {want:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// 3. Unresolved blocks are visible in the state and invisible in the scene
// ---------------------------------------------------------------------------

#[test]
fn an_unresolved_block_spawns_nothing_and_is_still_reported() {
    // Four blocks, one of which names an identity no package holds.
    let document = document(&[
        BlockSpec {
            instance: INSTANCE,
            position: [0.0, 0.0, 0.0],
            scale: 1.0,
            orientation: IDENTITY,
            children: &[],
        },
        BlockSpec {
            instance: INSTANCE,
            position: [1.0, 0.0, 0.0],
            scale: 1.0,
            orientation: IDENTITY,
            children: &[],
        },
        BlockSpec {
            instance: 0xDEAD_BEEF,
            position: [2.0, 0.0, 0.0],
            scale: 1.0,
            orientation: IDENTITY,
            children: &[],
        },
        BlockSpec {
            instance: INSTANCE,
            position: [3.0, 0.0, 0.0],
            scale: 1.0,
            orientation: IDENTITY,
            children: &[],
        },
    ]);
    let stage = project(&document, &chain_source());
    let counts = stage.projection.counts();
    assert_eq!(counts.resolved, 3);
    assert_eq!(counts.unresolved, 1);

    let state = stage.runtime_state(None);
    assert_eq!(state.block_count, 4);
    assert_eq!(state.unresolved_count, 1);
    // The row exists, and it carries a stage and a reason: not a bare absence.
    let missing = &state.blocks[2];
    assert!(!missing.resolved);
    assert!(!missing.visible);
    assert_eq!(
        missing.stop_stage.as_deref(),
        Some("rigblock"),
        "the block named an identity with no rigblock record, so that is where it \
         stopped: {missing:?}"
    );
    assert_eq!(missing.stop_reason.as_deref(), Some("no_such_record"));
    assert!(
        missing.block_id.is_some(),
        "it did have a block id to look up"
    );
    assert!(missing.container.is_none());

    let mut app = creator_app(stage, None, 1.0);
    app.update();

    // Three entities, not four, and never a fourth carrying an empty mesh.
    assert_eq!(count::<Entity, With<CreatorBlockMarker>>(&mut app), 3);

    // And the state the app publishes says three are on screen, not four.
    let state = app.world().resource::<CreatorRuntimeState>();
    assert_eq!(state.visible_block_count, 3);
    assert_eq!(state.unresolved_count, 1);
    assert_eq!(state.blocks.len(), 4, "the failure keeps its row");
}

#[test]
fn a_report_line_names_every_block_that_did_not_reach_the_screen() {
    let document = document(&[
        BlockSpec {
            instance: INSTANCE,
            position: [0.0, 0.0, 0.0],
            scale: 1.0,
            orientation: IDENTITY,
            children: &[],
        },
        BlockSpec {
            instance: 0xDEAD_BEEF,
            position: [0.0, 0.0, 0.0],
            scale: 1.0,
            orientation: IDENTITY,
            children: &[],
        },
    ]);
    let stage = project(&document, &chain_source());
    let lines = stage.report_lines();
    assert_eq!(lines.len(), 2, "one summary plus one failure: {lines:?}");
    assert!(lines[0].contains("blocks=2"), "{lines:?}");
    assert!(lines[0].contains("resolved=1"), "{lines:?}");
    assert!(lines[0].contains("unresolved=1"), "{lines:?}");
    assert!(lines[1].contains("block=1"), "{lines:?}");
    assert!(lines[1].contains("resolved=false"), "{lines:?}");
    assert!(lines[1].contains("reason=no_such_record"), "{lines:?}");
}

#[test]
fn a_creator_that_resolved_nothing_reports_zero_visible_rather_than_nothing_at_all() {
    let document = document(&[BlockSpec {
        instance: 0xDEAD_BEEF,
        position: [0.0, 0.0, 0.0],
        scale: 1.0,
        orientation: IDENTITY,
        children: &[],
    }]);
    let stage = project(&document, &empty_source());
    let lines = stage.report_lines();
    assert!(lines[0].contains("visible=0"), "{lines:?}");
    assert!(lines[0].contains("blocks=1"), "{lines:?}");

    let mut app = creator_app(stage, None, 1.0);
    app.update();
    assert_eq!(count::<Entity, With<CreatorBlockMarker>>(&mut app), 0);
}

// ---------------------------------------------------------------------------
// 4. Save -> reload -> re-project
// ---------------------------------------------------------------------------

#[test]
fn a_saved_document_reloads_and_reprojects_identically() {
    let original = three_block_document();
    let source = chain_source();

    // Round-trip the authoring data through the canonical interchange format.
    let text = persist::save(&original).expect("saves");
    assert!(text.starts_with("{\"record\":\"metadata\""), "{text}");
    assert!(
        text.contains("\"schema\":\"spore-creator-doc/1\""),
        "{text}"
    );
    let reloaded = persist::load(&text).expect("loads back");
    assert_eq!(reloaded.blocks.len(), original.blocks.len());
    assert_eq!(reloaded.roots(), original.roots());
    assert_eq!(reloaded.format_version, original.format_version);

    let before = project(&original, &source);
    let after = project(&reloaded, &source);
    assert_projections_agree(&before.projection, &after.projection);

    // And the engine's own save path writes exactly that format's bytes.
    let path = temp_path("creator-roundtrip.sporedoc");
    creator::save_document(&path, &original).expect("the engine can save");
    let from_disk = std::fs::read_to_string(&path).expect("the file is there");
    assert_eq!(from_disk, text, "two saves of one document are one file");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn a_document_reloaded_from_a_file_stages_with_no_family_claimed() {
    let document = three_block_document();
    let path = temp_path("creator-family.sporedoc");
    creator::save_document(&path, &document).expect("saves");

    let stage = creator::project_document(
        persist::load(&std::fs::read_to_string(&path).unwrap()).unwrap(),
        &DocumentSource::SavedFile(path.clone()),
        &chain_source(),
        0,
    )
    .expect("projects");

    // `spore-creator-doc/1` carries no type word, so the family is unknown rather
    // than defaulted to `creature`, and the report says so.
    assert_eq!(stage.source.family(), None);
    let state = stage.runtime_state(None);
    assert_eq!(state.document_family, "unknown");
    assert_eq!(state.document_source, "saved-file");
    assert!(
        state.document.ends_with("creator-family.sporedoc"),
        "{}",
        state.document
    );
    assert_eq!(state.block_count, 3);
    let lines = stage.report_lines();
    assert!(lines[0].contains("family=unknown"), "{lines:?}");
    let _ = std::fs::remove_file(&path);
}

/// Asserts two projections agree block for block.
///
/// `Projection` deliberately has no `PartialEq` -- it owns decoded geometry, and an
/// equality that compared 110 037 floats would be a test nobody would read -- so
/// the fields that carry meaning are compared here instead.
fn assert_projections_agree(
    left: &spore_morphology::Projection,
    right: &spore_morphology::Projection,
) {
    assert_eq!(left.blocks().len(), right.blocks().len());
    for (a, b) in left.blocks().iter().zip(right.blocks()) {
        assert_eq!(a.block, b.block);
        assert_eq!(a.block_id, b.block_id);
        assert_eq!(a.local_position.to_array(), b.local_position.to_array());
        assert_eq!(a.local_scale.to_bits(), b.local_scale.to_bits());
        assert_eq!(a.parent, b.parent);
        assert_eq!(a.ancestor_depth, b.ancestor_depth);
        assert_eq!(a.world_position.to_array(), b.world_position.to_array());
        assert_eq!(
            a.world_orientation.as_matrix().to_array(),
            b.world_orientation.as_matrix().to_array(),
            "a reload that changed an orientation would move a part silently"
        );
        assert_eq!(a.is_resolved(), b.is_resolved());
        assert_eq!(
            a.unresolved.as_ref().map(|u| (u.stage, u.reason)),
            b.unresolved.as_ref().map(|u| (u.stage, u.reason)),
        );
    }
    assert_eq!(left.counts(), right.counts());
    assert_eq!(left.total_triangles(), right.total_triangles());
    assert_eq!(left.total_vertices(), right.total_vertices());
    assert_eq!(left.total_bones(), right.total_bones());
    assert_eq!(left.max_depth(), right.max_depth());
}

// ---------------------------------------------------------------------------
// 5. Readiness, so automation does not sleep and hope
// ---------------------------------------------------------------------------

#[test]
fn the_scene_reports_stability_before_it_reports_itself_ready() {
    let stage = project(&three_block_document(), &chain_source());
    let mut app = creator_app(stage, None, 1.0);

    // Before anything runs: not stable, frame 0, and the counts are already there.
    {
        let state = app.world().resource::<CreatorRuntimeState>();
        assert!(!state.scene_stable);
        assert_eq!(state.frame, 0);
        assert_eq!(state.block_count, 3);
        assert_eq!(
            state.visible_block_count, 0,
            "nothing is on screen before the first frame, and the state must not \
             claim otherwise"
        );
    }

    app.update();
    {
        let state = app.world().resource::<CreatorRuntimeState>();
        assert!(
            state.scene_stable,
            "the spawn has happened by the first Update"
        );
        assert_eq!(state.frame, 1);
        assert_eq!(state.visible_block_count, 3);
    }
    app.update();
    assert_eq!(app.world().resource::<CreatorRuntimeState>().frame, 2);
}

#[test]
fn a_whole_creature_shares_one_material_per_tint() {
    // Three blocks, three entities, but TWO `StandardMaterial` assets: the tint is
    // a function of "is this the selection" and nothing else, so N parts must not
    // allocate N materials. Measured on the asset store rather than trusted.
    let stage = project(&three_block_document(), &chain_source());
    let mut app = creator_app(stage, Some(0), 1.0);
    app.update();

    let mut state =
        query_of::<&MeshMaterial3d<StandardMaterial>, With<CreatorBlockMarker>>(&mut app);
    let handles: Vec<AssetId<StandardMaterial>> = state
        .iter(app.world_mut())
        .map(|handle| handle.id())
        .collect();
    drop(state);
    assert_eq!(handles.len(), 3);
    let mut distinct = handles.clone();
    distinct.sort();
    distinct.dedup();
    assert_eq!(
        distinct.len(),
        2,
        "one material for the selection and one for the rest, not one per block: \
         {handles:?}"
    );
}

#[test]
fn a_selection_is_readable_from_the_state_and_lives_on_exactly_one_entity() {
    let stage = project(&three_block_document(), &chain_source());
    let mut app = creator_app(stage, Some(1), 1.0);
    app.update();

    assert_eq!(
        app.world().resource::<CreatorRuntimeState>().selected_block,
        Some(1)
    );
    let mut state = query_of::<&CreatorBlockMarker, With<Mesh3d>>(&mut app);
    let selected: Vec<u32> = state
        .iter(app.world_mut())
        .filter(|marker| marker.selected)
        .map(|marker| marker.block)
        .collect();
    drop(state);
    assert_eq!(selected, vec![1]);
}

// ---------------------------------------------------------------------------
// 6. The command line
// ---------------------------------------------------------------------------

#[test]
fn the_creator_mode_is_its_own_launch_request() {
    let LaunchRequest::Creator(request) = cli::parse([
        "--creator",
        "0x2b978c46:0x40626200:0x05e3df13",
        "--package",
        "/game.package",
        "--package",
        "/graphics.package",
        "--select-block",
        "4",
        "--creator-save",
        "/tmp/out.sporedoc",
        "--scale",
        "2.5",
        "--info",
    ])
    .expect("parses") else {
        panic!("expected a creator request")
    };
    assert_eq!(
        request.document,
        CreatorSource::Record(ResourceKey::new(0x2B97_8C46, 0x4062_6200, 0x05E3_DF13))
    );
    assert_eq!(request.packages.len(), 2);
    assert_eq!(request.select, Some(4));
    assert_eq!(request.save_to, Some(PathBuf::from("/tmp/out.sporedoc")));
    assert_eq!(request.scale, 2.5);
    assert!(request.info_only);
    assert_eq!(request.window.title, "OpenSpore - creator");
}

#[test]
fn a_creator_document_can_be_read_from_a_saved_file() {
    let LaunchRequest::Creator(request) = cli::parse([
        "--creator-doc",
        "/tmp/out.sporedoc",
        "--package",
        "/game.package",
    ])
    .expect("parses") else {
        panic!("expected a creator request")
    };
    assert_eq!(
        request.document,
        CreatorSource::SavedFile(PathBuf::from("/tmp/out.sporedoc"))
    );
    assert_eq!(request.save_to, None);
    assert_eq!(request.select, None);
}

#[test]
fn the_creator_mode_refuses_to_share_a_run_with_a_record() {
    // Two staging paths with two completeness vocabularies on one stage would
    // leave a caller unable to tell which report it was reading.
    assert_eq!(
        cli::parse(["--creator", "0x2b978c46:0x40626200:1", "--record", "1:2:3"]).unwrap_err(),
        CliError::CreatorModeConflict {
            flag: "--creator",
            other: "--record"
        }
    );
    assert!(matches!(
        cli::parse(["--creator", "0x2b978c46:0x40626200:1", "--placeholder"]).unwrap_err(),
        CliError::CreatorModeConflict {
            other: "--placeholder",
            ..
        }
    ));
    assert!(matches!(
        cli::parse([
            "--creator",
            "0x2b978c46:0x40626200:1",
            "--creator-doc",
            "/tmp/x.sporedoc"
        ])
        .unwrap_err(),
        CliError::CreatorModeConflict {
            flag: "--creator-doc",
            ..
        }
    ));
}

#[test]
fn a_creator_flag_with_no_document_is_a_usage_error_not_a_default() {
    for (flag, value) in [
        ("--creator-save", "/tmp/out.sporedoc"),
        ("--select-block", "0"),
    ] {
        assert_eq!(
            cli::parse([flag, value]).unwrap_err(),
            CliError::OrphanCreatorFlag {
                flag: flag.to_owned(),
                needs: "--creator <T:G:I> or --creator-doc <PATH>",
            },
            "{flag}"
        );
    }
}

#[test]
fn a_creator_with_no_package_names_the_two_packages_a_part_spans() {
    let request =
        cli::parse(["--creator", "0x2b978c46:0x40626200:0x05e3df13", "--info"]).expect("parses");
    let LaunchRequest::Creator(request) = request else {
        panic!("expected a creator request")
    };
    let spore_engine::CreatorDecoded::Failed(reason) = spore_engine::prepare_creator(&request)
    else {
        panic!("no packages means no parts, not a crash")
    };
    assert!(reason.contains("--package"), "{reason}");
    assert!(reason.contains("Spore_Game.package"), "{reason}");
    assert!(reason.contains("Spore_Graphics.package"), "{reason}");
}

#[test]
fn a_creator_record_of_an_unknown_type_is_a_data_error_naming_the_type_word() {
    let request = match cli::parse(["--creator", "0xdeadbeef:0x40626200:0x05e3df13"]).unwrap() {
        LaunchRequest::Creator(request) => request,
        other => panic!("expected a creator request, got {other:?}"),
    };
    let spore_engine::CreatorDecoded::Failed(reason) = spore_engine::prepare_creator(&request)
    else {
        panic!("an unknown family must be refused, not re-typed")
    };
    assert!(reason.contains("0xdeadbeef"), "{reason}");
    assert!(reason.contains("authoring families"), "{reason}");
}

#[test]
fn usage_documents_the_creator_flags_and_their_exit_codes() {
    let text = cli::usage();
    for flag in [
        "--creator",
        "--creator-doc",
        "--creator-save",
        "--select-block",
    ] {
        assert!(text.contains(flag), "usage must document {flag}: {text}");
    }
    assert!(text.contains("spore-creator-doc/1"), "{text}");
    assert!(
        text.contains("Spore_Graphics.package"),
        "why two packages are needed must be stated, not just offered: {text}"
    );
    assert!(
        text.contains(&cli::EXIT_ASSET.to_string()),
        "the exit codes must stay in the text: {text}"
    );
}

// ---------------------------------------------------------------------------
// 7. The vertical slice is unregressed
// ---------------------------------------------------------------------------

#[test]
fn the_documented_asset_preset_is_unchanged() {
    // The preset, the parse result and the window title are all a recorded
    // contract; the creator mode must not have moved any of them.
    let LaunchRequest::Scene(request) = cli::parse([
        "--preset",
        "documented-asset",
        "--package",
        "/content.package",
        "--info",
    ])
    .expect("parses") else {
        panic!("a preset still stages a scene, not a creator")
    };
    assert_eq!(
        request.records(),
        vec![ResourceKey::new(
            cli::PRESET_DOCUMENTED_ASSET.0,
            cli::PRESET_DOCUMENTED_ASSET.1,
            cli::PRESET_DOCUMENTED_ASSET.2
        )]
    );
    assert_eq!(
        cli::PRESET_DOCUMENTED_ASSET,
        (0x00E6_BCE5, 0x4063_7E03, 0x067A_07F0)
    );
    assert_eq!(request.packages, vec![PathBuf::from("/content.package")]);
    assert!(request.info_only);
    assert_eq!(request.window.title, "OpenSpore - 1 object(s)");
    assert_eq!(request.to_scene().len(), 1);
}

#[test]
fn the_record_scene_path_still_reports_a_missing_asset_and_names_the_fix() {
    // Same failure, same exit code, same message shape as before the creator mode
    // existed: the asset path is untouched.
    let request = match cli::parse(["--preset", "documented-asset"]).unwrap() {
        LaunchRequest::Scene(request) => request,
        other => panic!("expected a scene request, got {other:?}"),
    };
    let spore_engine::Decoded::Failed(reason) = spore_engine::prepare(&request) else {
        panic!("no packages means no record, not a crash")
    };
    assert!(reason.contains("--package"), "{reason}");
    assert_eq!(cli::EXIT_ASSET, 3);
    assert_eq!(cli::EXIT_USAGE, 2);
}

#[test]
fn a_creator_launch_request_never_reaches_the_record_preparer() {
    // Two entry points, two contracts. `prepare` is for records; a creator request
    // handed to it would resolve a `<sporemodel>` triple as a model record and
    // report "not found", which is a true sentence about the wrong question.
    let request = match cli::parse(["--creator", "0x2b978c46:0x40626200:1"]).unwrap() {
        LaunchRequest::Creator(request) => request,
        other => panic!("expected a creator request, got {other:?}"),
    };
    assert!(matches!(
        spore_engine::prepare_creator(&request),
        CreatorDecoded::Failed(_)
    ));
}

// ---------------------------------------------------------------------------
// 8. The real corpus
// ---------------------------------------------------------------------------

/// Absolute, because `cargo test` runs with the crate directory as CWD.
fn install_root() -> Option<PathBuf> {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
}

/// The six authoring record type ids, and their families.
const FAMILIES: [(u32, Family); 6] = [
    (0x2b97_8c46, Family::Creature),
    (0x3d97_a8e4, Family::Cell),
    (0x2468_2294, Family::Vehicle),
    (0x476a_98c7, Family::Ufo),
    (0x2399_be55, Family::Building),
    (0x438f_6347, Family::Flora),
];

/// The three packages a creator part spans, or `None` with a note.
///
/// The rigblock bounds live in `Spore_Game.package`, the `prop` pointer and the
/// `0x40606000` pair in `Spore_Graphics.package`, and the tail of the chain can
/// reach into content. Naming only the first two makes every part stop at the part
/// pair, which looks like a resolver bug and is not one.
fn install_packages() -> Option<Vec<PathBuf>> {
    let game = install("Spore_Game.package")?;
    let graphics = install("Spore_Graphics.package")?;
    let content = install("Spore_Content.package")?;
    Some(vec![game, graphics, content])
}

fn install(path: &str) -> Option<PathBuf> {
    let candidate = install_root()?.join("SPORE/Data").join(path);
    candidate.exists().then_some(candidate)
}

/// Every creator document identity in the install, across all six families.
fn creator_identities(packages: &[PathBuf]) -> Vec<ResourceKey> {
    let store = spore_engine::domain::world::ResourceWorld::open(packages).expect("opens");
    let mut identities: Vec<ResourceKey> = Vec::new();
    for (type_id, _) in FAMILIES {
        identities.extend(store.store().identities_of_type(type_id));
    }
    identities
}

/// Decodes one real creator document through the engine's own entry point.
fn decode(packages: &[PathBuf], identity: ResourceKey) -> creator::CreatorStage {
    let request = cli::CreatorRequest {
        packages: packages.to_vec(),
        document: CreatorSource::Record(identity),
        save_to: None,
        select: None,
        scale: 1.0,
        window: cli::WindowOptions::default(),
        info_only: true,
    };
    match spore_engine::prepare_creator(&request) {
        CreatorDecoded::Ready(stage) => *stage,
        CreatorDecoded::Failed(reason) => panic!("{identity} should decode: {reason}"),
    }
}

#[test]
#[ignore = "needs SPORE/Data/Spore_Game.package; run with --ignored --nocapture"]
fn a_real_creator_document_projects_to_one_entity_per_resolved_block() {
    let Some(packages) = install_packages() else {
        return;
    };

    // Scan every creator document and remember the two that matter differently:
    // the one with the most blocks, and the deepest one. They are not the same
    // document -- conflating "largest" with "deepest" is how a forest assertion
    // ends up asserting depth 1 on a flat list.
    let mut census: Vec<(usize, usize, ResourceKey)> = Vec::new();
    for identity in creator_identities(&packages) {
        let stage = decode(&packages, identity);
        census.push((stage.block_count(), stage.projection.max_depth(), identity));
    }
    assert_eq!(census.len(), 68, "the corpus is 68 creator documents");

    let widest = census.iter().max_by_key(|(blocks, _, _)| *blocks).unwrap();
    let deepest = census.iter().max_by_key(|(_, depth, _)| *depth).unwrap();
    assert!(
        widest.0 >= 20,
        "the largest document should exercise a real forest, got {} blocks",
        widest.0
    );
    assert_eq!(
        deepest.1, 19,
        "the authored forest is 19 deep at its maximum; a shallower answer means a \
         parent chain was dropped somewhere between the document and the projection"
    );

    for (label, &(blocks, depth, identity)) in [("widest", widest), ("deepest", deepest)] {
        let stage = decode(&packages, identity);
        let counts = stage.projection.counts();
        assert_eq!(
            counts.total(),
            blocks,
            "{label}: one projected entry per authored block"
        );
        assert_eq!(
            stage.without_geometry_count(),
            0,
            "{label}: real gmsh records decode"
        );
        assert_eq!(stage.visible_block_count(), counts.resolved);
        assert_eq!(stage.projection.max_depth(), depth);

        let mut app = creator_app(stage, None, 1.0);
        app.update();

        assert_eq!(
            count::<Entity, With<CreatorBlockMarker>>(&mut app),
            counts.resolved,
            "{label} {identity}: one entity per RESOLVED block, and never one per \
             block that failed"
        );

        let state = app.world().resource::<CreatorRuntimeState>();
        assert_eq!(state.block_count as usize, blocks, "{label}");
        assert_eq!(
            state.visible_block_count as usize, counts.resolved,
            "{label}"
        );
        assert_eq!(state.max_depth as usize, depth, "{label}");
        assert!(state.scene_stable);
        assert!(
            state.total_triangles > 0,
            "{label}: real geometry, not a fixture's"
        );
        assert_eq!(state.blocks.len(), blocks, "{label}: every block has a row");
        assert_eq!(
            state.unresolved_count as usize, counts.unresolved,
            "{label}: the failures are counted, not dropped"
        );
    }
}

#[test]
#[ignore = "needs SPORE/Data/Spore_Game.package; run with --ignored --nocapture"]
fn the_corpus_entity_count_equals_the_corpus_resolved_count() {
    let Some(packages) = install_packages() else {
        return;
    };
    let store = spore_engine::domain::world::ResourceWorld::open(&packages).expect("opens");
    let records = creator::StoreSource::new(store.store());
    let source = store.store();

    let mut total_blocks = 0usize;
    let mut total_resolved = 0usize;
    let mut documents = 0usize;
    for (type_id, _) in FAMILIES {
        for identity in source.identities_of_type(type_id) {
            let Ok(bytes) = source.read(&identity) else {
                continue;
            };
            let Ok(text) = std::str::from_utf8(&bytes) else {
                continue;
            };
            let family = match Family::from_type_id(identity.type_id) {
                Some(family) => family,
                None => continue,
            };
            let Ok(document) = parse_sporemodel(family, text) else {
                continue;
            };
            documents += 1;
            let stage = project(&document, &records);
            let counts = stage.projection.counts();
            total_blocks += counts.total();
            total_resolved += counts.resolved;

            // The same statement, per document, against actual entities.
            let mut app = creator_app(stage, None, 1.0);
            app.update();
            assert_eq!(
                count::<Entity, With<CreatorBlockMarker>>(&mut app),
                counts.resolved,
                "{identity}: entities must equal resolved blocks, never block count"
            );
        }
    }

    assert_eq!(documents, 68, "every creator document parses");
    assert_eq!(total_blocks, 680, "680 blockrefs across the 68 documents");
    assert_eq!(
        total_resolved, 678,
        "678 blocks resolve to a part; the other 2 must be reported, not skipped"
    );

    // 678 entities and 680 blocks is only honest if the two gaps are named. This is
    // the `--info` text an operator reads, so it is asserted here rather than
    // described in a document: 68 summary lines, exactly 2 block lines, and every
    // one of them carrying a stage and a reason.
    let mut summary_lines = 0usize;
    let mut failure_lines = 0usize;
    let mut reasons = Vec::new();
    for identity in creator_identities(&packages) {
        let stage = decode(&packages, identity);
        for line in stage.report_lines() {
            if line.starts_with("OPENSPORE-CREATOR-BLOCK ") {
                failure_lines += 1;
                assert!(line.contains("reason="), "{line}");
                assert!(line.contains("stage="), "{line}");
                reasons.push(line);
            } else {
                assert!(line.starts_with("OPENSPORE-CREATOR v1 "), "{line}");
                assert!(line.contains("blocks="), "{line}");
                assert!(line.contains("unresolved="), "{line}");
                assert!(line.contains("visible="), "{line}");
                summary_lines += 1;
            }
        }
    }
    assert_eq!(summary_lines, 68, "one summary line per document");
    assert_eq!(
        failure_lines, 2,
        "exactly the two corpus failures, each on its own line: {reasons:?}"
    );
}

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

fn temp_path(name: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!("openspore-{}-{name}", std::process::id()));
    path
}
