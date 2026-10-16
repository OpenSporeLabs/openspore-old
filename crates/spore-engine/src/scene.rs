//! The scene: camera, light, ground and the placeholder.
//!
//! Kept apart from the asset bridge so the "is the renderer alive" question and
//! the "can it read a Spore record" question have separate answers. Swapping the
//! contents of the world must not require touching the camera.
//!
//! # One owner per entity kind
//!
//! This plugin owns the **camera, the lights and the ground**. It never spawns
//! Spore content, and [`crate::loader::SporeAssetPlugin`] never spawns a camera.
//! The split is not tidiness: when both plugins spawned a camera, Bevy warned
//! "Camera order ambiguities detected", the model rendered from whichever camera
//! it picked, and the framing query found "multiple entities" — a duplicate
//! spawn wearing a framing bug's clothes.

use bevy::app::Plugin;
use bevy::pbr::StandardMaterial;
use bevy::prelude::*;

use crate::domain::assembly::{ModuleKind, Primitive};

/// A soft sky colour behind everything.
pub const BACKDROP: Color = Color::srgb(0.05, 0.06, 0.09);

/// The colour of the reference floor grid.
pub const GROUND: Color = Color::srgb(0.16, 0.17, 0.20);

/// The colour used for a loaded asset that has no usable texture.
///
/// A neutral grey, deliberately: no raster record in this corpus decodes as a
/// base-colour map, so any hue would be an invention. See
/// [`crate::texture::UNTEXURED_TINT`], which is the same decision expressed at
/// the material layer.
pub const ASSET_TINT: Color = Color::srgb(0.62, 0.62, 0.62);

/// The colour used for the placeholder mesh.
pub const PLACEHOLDER_TINT: Color = Color::srgb(0.30, 0.68, 0.86);

/// Marker for the camera, so a scene can be retargeted without a name string.
#[derive(Debug, Clone, Copy, Component, Default)]
pub struct MainCamera;

/// Marker for an asset entity currently on show.
#[derive(Debug, Clone, Copy, Component, Default)]
pub struct OnStage;

/// The debug ground plane, so a caller can remove it.
///
/// It exists because a floating model with nothing under it gives no depth cue.
/// The **Creature Creator removes it**, for two reasons and the first is a bug:
///
/// 1. It is **opaque**, 400x400, and it sits at `y = -0.01`. A creature whose
///    centre is below that -- and `0x135ff098` centres at `y = -0.529` -- is
///    completely buried by it: ten entities resolve, ten report as visible, the
///    camera frames them correctly, and the screen shows nothing but grey floor.
///    "Everything resolved and nothing is on screen" is the exact signature of an
///    occluder, and the entity counts all say the model is fine.
/// 2. The reference Creator has a landscape backdrop and no grid, so keeping it
///    would also be wrong on visual grounds.
///
/// Marked rather than found by position, because "the entity with the largest
/// `Plane3d` mesh" is a heuristic that a scene with two floors would break.
#[derive(Debug, Clone, Copy, Component, Default)]
pub struct GroundPlane;

/// Everything that is *not* Spore content: the world the content sits in.
///
/// # Why this is a plugin and not a function
///
/// The world and the content change for different reasons and at different
/// times. The camera, the lights and the ground are OpenSpore's own choices and
/// change when the renderer changes; the content changes when a different
/// package is loaded. Splitting them means a content change cannot accidentally
/// move the camera, and a renderer change cannot accidentally respawn the
/// content -- and each can be tested with the other absent.
///
/// It is deliberately the *smallest* plugin that is still a real boundary. There
/// is one world and no scene graph; the scene description lives in
/// [`crate::domain::scene`] and is turned into entities by
/// [`crate::loader::SporeAssetPlugin`], which is where the content ownership
/// already was.
#[derive(Debug, Clone, Copy, Default)]
pub struct ScenePlugin;

impl Plugin for ScenePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_world);
    }
}

/// Spawns the camera, the key light, the ground and the backdrop.
fn spawn_world(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    spawn_stage(&mut commands, &mut meshes, &mut materials);
}

/// The world spawn itself, callable directly for tests and for the placeholder.
pub fn spawn_stage(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) {
    commands.insert_resource(ClearColor(Color::BLACK));

    // A 3D camera framed so a model of roughly unit-to-10-unit size lands in
    // view. Spore geometry is authored in its own units, so the distance is a
    // starting point that `crate::loader::frame_camera_on_the_scene` then
    // corrects in `PostStartup` from the scene's measured extent.
    commands.spawn((
        Camera3d::default(),
        MainCamera,
        Transform::from_xyz(0.0, 6.0, 34.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    commands.spawn((
        DirectionalLight {
            illuminance: 9_000.0,
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_xyz(8.0, 14.0, 8.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    // A dim fill from the opposite side so an unlit-looking back face still has
    // some shape. Two lights is the smallest rig that reads as a scene.
    commands.spawn((
        DirectionalLight {
            illuminance: 2_500.0,
            shadow_maps_enabled: false,
            contact_shadows_enabled: false,
            ..default()
        },
        Transform::from_xyz(-10.0, 4.0, -6.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(400.0, 400.0))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: GROUND,
            perceptual_roughness: 0.95,
            metallic: 0.0,
            ..default()
        })),
        Transform::from_xyz(0.0, -0.01, 0.0),
        GroundPlane,
    ));
}

/// Overall size of the placeholder creature, chosen so it reads well at the
/// placeholder camera's fixed distance. This is a **presentation** choice and
/// lives here, not in [`crate::domain::assembly`]: the assembly is authored at
/// its natural scale and the renderer decides how big to show it.
pub const PLACEHOLDER_SCALE: f32 = 1.8;

/// The colour of a placeholder module, chosen from the module's *kind*.
///
/// The domain says what a module is for ([`ModuleKind`]); the renderer says what
/// it looks like. That split is the boundary rule, and it is why there is no RGB
/// in the assembly.
fn module_color(kind: ModuleKind) -> Color {
    match kind {
        ModuleKind::Body | ModuleKind::Limb | ModuleKind::Tail => PLACEHOLDER_TINT,
        ModuleKind::Head => Color::srgb(0.36, 0.72, 0.88),
        ModuleKind::Snout => Color::srgb(0.22, 0.52, 0.68),
        ModuleKind::Eye => Color::srgb(0.04, 0.05, 0.07),
    }
}

/// The Bevy mesh for one placeholder primitive.
fn primitive_mesh(primitive: Primitive) -> Mesh {
    match primitive {
        Primitive::Sphere { radius } => Sphere::new(radius).mesh().uv(24, 12),
        Primitive::Capsule { radius, length } => Capsule3d::new(radius, length).mesh().build(),
        Primitive::Cylinder { radius, height } => Cylinder::new(radius, height).mesh().build(),
        Primitive::Cuboid { half_extents } => Cuboid::new(
            half_extents[0] * 2.0,
            half_extents[1] * 2.0,
            half_extents[2] * 2.0,
        )
        .mesh()
        .build(),
    }
}

/// Spawns the built-in placeholder **assembly**, used by `--placeholder`.
///
/// It exists so "the window opened and a modular 3D scene rendered" can be
/// established with no game install at all. If that cannot be demonstrated,
/// every later failure is ambiguous between "the renderer is broken" and "the
/// asset is wrong", and the vertical slice stops being debuggable.
///
/// The creature's structure and placement come from
/// [`crate::domain::assembly::quadruped`], which is Bevy-free; this function is
/// the whole of the conversion. Modules are spawned as children of one root so
/// that the assembly is one hierarchy, and the root carries [`OnStage`] so
/// [`clear_stage`] removes the creature and its parts together.
pub fn spawn_placeholder_creature(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) -> Entity {
    let assembly = crate::domain::assembly::quadruped();
    let placements = assembly
        .placements()
        .expect("the built-in preset is well-formed by construction");

    let root = commands
        .spawn((
            Transform::from_scale(Vec3::splat(PLACEHOLDER_SCALE)),
            Visibility::default(),
            OnStage,
        ))
        .id();

    for (module, placement) in assembly.modules.iter().zip(placements) {
        let child = commands
            .spawn((
                Mesh3d(meshes.add(primitive_mesh(module.primitive))),
                MeshMaterial3d(materials.add(StandardMaterial {
                    base_color: module_color(module.kind),
                    perceptual_roughness: 0.55,
                    metallic: 0.0,
                    ..default()
                })),
                Transform {
                    translation: Vec3::from_array(placement.translation),
                    rotation: crate::loader::orientation_to_quat(placement.orientation),
                    scale: Vec3::splat(placement.scale),
                },
            ))
            .id();
        commands.entity(root).add_child(child);
    }

    root
}

/// Despawns whatever was on stage, so a scene change never stacks up entities.
pub fn clear_stage(commands: &mut Commands, existing: &mut Query<Entity, With<OnStage>>) {
    for entity in existing.iter() {
        commands.entity(entity).despawn();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn world_with_stage() -> App {
        let mut app = App::new();
        // `MinimalPlugins` deliberately has no `AssetPlugin`, so the asset
        // collections this world writes into have to be initialised by hand.
        // That is also the point: the world spawn needs no asset *server*, no
        // file loader and no window.
        app.add_plugins((MinimalPlugins, bevy::asset::AssetPlugin::default()));
        app.init_asset::<Mesh>();
        app.init_asset::<StandardMaterial>();
        app.add_systems(Startup, spawn_world);
        app.finish();
        app.cleanup();
        app
    }

    #[test]
    fn the_world_owns_exactly_one_camera() {
        // The regression this split exists for: two plugins spawning a camera
        // produced "Camera order ambiguities detected" and a framing query that
        // found "multiple entities".
        let mut app = world_with_stage();
        app.update();
        let mut cameras = bevy::ecs::query::QueryState::<&Camera3d>::new(&mut *app.world_mut());
        assert_eq!(cameras.iter(app.world()).count(), 1);
        drop(cameras);
        let mut markers =
            bevy::ecs::query::QueryState::<Entity, With<MainCamera>>::new(&mut *app.world_mut());
        markers.update_archetypes(app.world());
        assert_eq!(markers.iter(app.world()).count(), 1);
    }

    #[test]
    fn the_world_owns_exactly_two_lights() {
        let mut app = world_with_stage();
        app.update();
        let mut lights =
            bevy::ecs::query::QueryState::<&DirectionalLight>::new(&mut *app.world_mut());
        assert_eq!(lights.iter(app.world()).count(), 2);
    }

    #[test]
    fn the_world_owns_a_ground_but_no_asset() {
        let mut app = world_with_stage();
        app.update();
        let mut staged =
            bevy::ecs::query::QueryState::<Entity, With<OnStage>>::new(&mut *app.world_mut());
        assert_eq!(
            staged.iter(app.world()).count(),
            0,
            "ScenePlugin must never spawn Spore content"
        );
        drop(staged);
        let mut ground =
            bevy::ecs::query::QueryState::<Entity, With<Mesh3d>>::new(&mut *app.world_mut());
        assert_eq!(ground.iter(app.world()).count(), 1, "the ground plane");
    }

    #[test]
    fn the_untinted_colours_are_neutral_grey_not_a_hue() {
        // No raster record in this corpus decodes as base colour, so a hue here
        // would be an invention.
        let linear = ASSET_TINT.to_linear();
        assert!(
            (linear.red - linear.green).abs() < 0.01 && (linear.green - linear.blue).abs() < 0.01,
            "not neutral: {ASSET_TINT:?}"
        );
        assert!(
            linear.red > 0.2 && linear.red < 0.9,
            "and not black: {ASSET_TINT:?}"
        );
    }

    #[test]
    fn the_placeholder_is_a_modular_assembly_under_one_staged_root() {
        let mut app = world_with_stage();
        // Spawn the placeholder *in addition to* the world, as the real app does.
        app.add_systems(
            Startup,
            |mut commands: Commands,
             mut meshes: ResMut<Assets<Mesh>>,
             mut materials: ResMut<Assets<StandardMaterial>>| {
                spawn_placeholder_creature(&mut commands, &mut meshes, &mut materials);
            },
        );
        app.update();

        let expected = crate::domain::assembly::quadruped().len();
        assert!(
            expected > 1,
            "the preset is meant to be more than one module"
        );

        let mut staged =
            bevy::ecs::query::QueryState::<Entity, With<OnStage>>::new(&mut *app.world_mut());
        assert_eq!(
            staged.iter(app.world()).count(),
            1,
            "exactly one staged root, so clear_stage removes the whole assembly"
        );
        drop(staged);

        let mut meshes_n = bevy::ecs::query::QueryState::<
            Entity,
            (With<Mesh3d>, Without<GroundPlane>),
        >::new(&mut *app.world_mut());
        assert_eq!(
            meshes_n.iter(app.world()).count(),
            expected,
            "one mesh entity per module"
        );
    }
}
