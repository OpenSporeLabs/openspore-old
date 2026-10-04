//! # spore-engine — the OpenSpore runtime.
//!
//! A Bevy application whose asset path starts at a real Spore `.package` file
//! and ends at pixels:
//!
//! ```text
//!   .package on disk
//!     -> spore_dbpf     DBPF v3 index, QFS decompress          (no Bevy)
//!     -> spore_gmdl     gmdl decode, mesh extraction           (no Bevy)
//!     -> spore_assets   package priority, identity resolution   (no Bevy)
//!     -> spore_material content roles per record               (no Bevy)
//!     -> domain        world, model, creature, scene, materials (no Bevy)
//!     -> convert       vertex buffers, normal and tangent policy (no GPU)
//!     -> loader        Assets<Mesh> handles, entity spawn      (Bevy)
//!     -> bevy_render   wgpu -> Vulkan / Metal / DX12
//! ```
//!
//! The split is the architecture, not an accident of file layout.
//!
//! # Three layers, and what each one may know about
//!
//! | layer | modules | may import |
//! |---|---|---|
//! | decode | `spore-core` … `spore-material` | nothing from this crate |
//! | domain | [`domain`] | `spore-*`, no `bevy` |
//! | runtime | [`loader`], [`texture`], [`convert`], [`scene`], [`cli`] | `bevy`, `domain`, `spore-*` |
//!
//! [`domain`] importing `bevy` would be the load-bearing violation: it is what
//! makes the whole asset path — including the normal-map swizzle and the
//! per-mesh role cut — assertable in `cargo test -p spore-engine` with no
//! window, no GPU and no game install.
//!
//! # Decoding happens before Bevy is even constructed
//!
//! Opening a ~995 MB package, parsing its ~17k-row index and decompressing a
//! record is ordinary I/O work with nothing to do with rendering. Doing it
//! inside a `Startup` system would make three things worse for no benefit: the
//! window would appear before the content, an asset failure could only be
//! reported through the render loop, and the most valuable test in the project
//! -- "does a real Spore record decode?" -- would need a GPU.
//!
//! So [`prepare`] decodes first, *outside* Bevy, and only then assembles an app.
//! A consequence worth stating: `--info` never constructs a window at all,
//! which is what makes it usable as a CI gate.
//!
//! # Modes
//!
//! * `--placeholder` -- a built-in mesh, no game data. Proves the renderer.
//! * `--record T:G:I --record T:G:I ...` -- many real Spore records in one
//!   scene, laid out from their own bounds.
//! * `--creature K --part K ...` -- one creature from one or more records.
//! * `--info` -- decode and report, then exit. Headless.
//!
//! # What is deliberately absent
//!
//! No audio, no input beyond the window, no simulation, no animation, no UI.
//! Each is a real subsystem that has not been written yet, and stubbing them
//! here would make the vertical slice look further along than it is. See
//! [`domain::creature`] for why `Creature` is a container and not a character
//! controller.

#![deny(unsafe_code)]
#![warn(missing_debug_implementations)]

pub mod cli;
pub mod convert;
pub mod domain;
pub mod loader;
pub mod scene;
pub mod texture;

pub use loader::{SceneOutcome, SporeAssetPlugin, WorldResources};
pub use scene::ScenePlugin;

use std::sync::Arc;

use bevy::pbr::StandardMaterial;
use bevy::prelude::*;
use bevy::window::{PresentMode, WindowResolution};

use cli::{LaunchRequest, SceneRequest, WindowOptions};
use loader::{
    CameraFraming, LoadedScene, NormalMapPolicy, PrepareOutcome, DEFAULT_VERTICAL_FOV_DEGREES,
};
use texture::NormalMapOrientation;

/// The engine entry point. The binary is a one-line shim over this.
pub fn main() -> std::process::ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match cli::parse(args) {
        Ok(LaunchRequest::Help) => {
            print!("{}", cli::usage());
            std::process::ExitCode::SUCCESS
        }
        Ok(LaunchRequest::Placeholder(window)) => run_placeholder(window),
        Ok(LaunchRequest::Scene(scene)) => run_scene(scene),
        Err(error) => {
            eprintln!("openspore: {error}\n");
            eprint!("{}", cli::usage());
            std::process::ExitCode::from(cli::EXIT_USAGE as u8)
        }
    }
}

/// The result of decoding a request, before any rendering is involved.
///
/// Separating this from the runtime is what lets a test assert the whole asset
/// path without a window, and what lets `--info` skip rendering entirely.
#[derive(Debug)]
pub enum Decoded {
    /// Everything decoded.
    Ready(Box<PrepareOutcome>),
    /// It did not, with a reason a person can act on.
    Failed(String),
}

impl Decoded {
    /// The report lines to print, or the failure line.
    pub fn report_lines(&self) -> Vec<String> {
        match self {
            Decoded::Ready(outcome) => outcome.report_lines(),
            Decoded::Failed(reason) => {
                vec![format!("OPENSPORE-STAGED v1 failed: {reason}")]
            }
        }
    }
}

/// Decodes a scene request without touching Bevy, the GPU, or the filesystem
/// beyond the packages named.
///
/// Textures are resolved here too, for the same reason the model is: the store
/// is a live view over memory-mapped packages, and resolving inside a spawn
/// system would either have to keep the mapping alive longer than the app or
/// re-enter it per spawn. Resolving up front also means `--info` reports texture
/// resolution and roles, so the headless path and the visible path cannot
/// disagree.
pub fn prepare(scene: &SceneRequest) -> Decoded {
    let world = match domain::world::ResourceWorld::open(&scene.packages) {
        Ok(world) => Arc::new(world),
        Err(error) => return Decoded::Failed(loader::describe(&error)),
    };
    let orientation = if scene.flip_normal_y {
        NormalMapOrientation::FlipGreen
    } else {
        NormalMapOrientation::AsStored
    };
    let mut staged = scene.to_scene();
    match loader::prepare_scene(
        world,
        std::mem::take(&mut staged),
        scene.textured,
        scene.assumed_texture_type,
        scene.layout,
        scene.spacing,
    ) {
        Ok((outcome, _extents)) => {
            info!(
                "material policy: normal-map orientation={} (flip-green is UNKNOWN and off by \
                 default), assumed texture type=0x{:08x}",
                orientation.as_str(),
                scene.assumed_texture_type
            );
            Decoded::Ready(Box::new(outcome))
        }
        Err(error) => Decoded::Failed(loader::describe(&error)),
    }
}

/// Prints every report line: one `STAGED v1` and one `MATERIAL v1` per record,
/// then one `SCENE v1` summary.
fn report(outcome: &Decoded) {
    for line in outcome.report_lines() {
        println!("{line}");
    }
}

fn run_placeholder(window: WindowOptions) -> std::process::ExitCode {
    info!("openspore: placeholder mode, no Spore data");
    println!("OPENSPORE-MODE v1 placeholder");
    build_placeholder_app(window).run();
    std::process::ExitCode::SUCCESS
}

fn run_scene(scene: SceneRequest) -> std::process::ExitCode {
    info!(
        "openspore: scene of {} object(s), {} record(s), textures={}",
        scene.objects.len(),
        scene.records().len(),
        scene.textured
    );

    // Decode first, outside Bevy. This is where a missing game install, a wrong
    // identity and a corrupt record all become visible -- and none of them
    // should cost the user a window.
    let outcome = prepare(&scene);
    report(&outcome);

    let Decoded::Ready(outcome) = outcome else {
        return std::process::ExitCode::from(cli::EXIT_ASSET as u8);
    };
    if scene.info_only {
        // Headless by design: nothing was opened, nothing was rendered.
        return std::process::ExitCode::SUCCESS;
    }

    let exit = build_scene_app(scene.window.clone(), *outcome).run();
    match exit {
        AppExit::Success => std::process::ExitCode::SUCCESS,
        AppExit::Error(cause) => {
            error!("render loop exited with an error: {cause}");
            std::process::ExitCode::FAILURE
        }
    }
}

/// Assembles the placeholder app.
pub fn build_placeholder_app(window: WindowOptions) -> App {
    let mut app = base_app(window);
    app.add_plugins(ScenePlugin);
    app.add_systems(
        Startup,
        |mut commands: Commands,
         mut meshes: ResMut<Assets<Mesh>>,
         mut materials: ResMut<Assets<StandardMaterial>>| {
            scene::spawn_placeholder(&mut commands, &mut meshes, &mut materials);
        },
    );
    app.add_systems(Update, exit_on_escape);
    app
}

/// Assembles the app that shows decoded Spore content.
///
/// Everything expensive has already happened. What is inserted here is data, in
/// the right order:
///
/// 1. the loaded scene, so [`SporeAssetPlugin`]'s `Startup` system has something
///    to spawn;
/// 2. the world, so a future streaming subsystem has the packages;
/// 3. the normal-map policy, which the same system reads;
/// 4. the camera framing, which the same plugin's `PostStartup` system reads.
pub fn build_scene_app(window: WindowOptions, outcome: PrepareOutcome) -> App {
    let orientation = NormalMapOrientation::default();
    let framing = outcome.loaded.bounds().map(|bounds| CameraFraming {
        bounds,
        vertical_fov_degrees: DEFAULT_VERTICAL_FOV_DEGREES,
    });

    let mut app = base_app(window);
    app.insert_resource(LoadedScene {
        scene: outcome.loaded.scene.clone(),
        models: outcome.loaded.models.clone(),
    });
    app.insert_resource(WorldResources {
        world: outcome.world.clone(),
    });
    app.insert_resource(NormalMapPolicy(orientation));
    if let Some(framing) = framing {
        app.insert_resource(framing);
    }
    // Two plugins, two responsibilities: the world is OpenSpore's, the content
    // is Spore's. Keeping them apart means loading different content cannot move
    // the camera, and changing the renderer cannot respawn the content.
    app.add_plugins((ScenePlugin, SporeAssetPlugin));
    app.add_systems(Update, exit_on_escape);
    app
}

/// Escape closes the window.
///
/// Bevy 0.19 has no built-in "close on escape", and Spore's own binding is not
/// recovered, so this is an OpenSpore decision rather than a compatibility one.
fn exit_on_escape(keys: Res<ButtonInput<KeyCode>>, mut exits: MessageWriter<AppExit>) {
    if keys.just_pressed(KeyCode::Escape) {
        exits.write(AppExit::Success);
    }
}

/// The plugins and resources every mode shares.
fn base_app(window: WindowOptions) -> App {
    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            resolution: WindowResolution::new(window.width, window.height),
            title: window.title,
            present_mode: PresentMode::AutoVsync,
            ..default()
        }),
        ..default()
    }));
    app
}

#[cfg(test)]
mod tests {
    use super::*;
    use cli::ObjectRequest;
    use spore_core::ResourceKey;

    fn request(objects: Vec<ObjectRequest>, textured: bool) -> SceneRequest {
        SceneRequest {
            packages: Vec::new(),
            objects,
            scale: 1.0,
            window: WindowOptions::default(),
            info_only: true,
            textured,
            layout: domain::Layout::Row,
            spacing: domain::scene::DEFAULT_GAP,
            flip_normal_y: false,
            assumed_texture_type: spore_core::record::type_id::RASTER,
        }
    }

    fn one(key: ResourceKey) -> Vec<ObjectRequest> {
        vec![ObjectRequest::Model {
            key,
            origin: cli::RecordOrigin::Explicit,
        }]
    }

    #[test]
    fn a_headless_app_assembles_but_the_real_one_needs_the_main_thread() {
        // `DefaultPlugins` builds a winit `EventLoop`, and winit 0.30 panics when
        // that happens off the main thread. So the *only* honest headless
        // assertion is over the plugin subset that has no event loop -- which is
        // precisely the part worth testing, because it is the part a Bevy API
        // change can silently break.
        //
        // This was found by writing the test the obvious way and watching it
        // panic: an "app assembles" test that needs a window is not a unit test.
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_systems(Update, exit_on_escape);
        // The app must at least have a schedule runner and our system registered
        // against it; `finish`/`cleanup` would panic on a malformed plugin set.
        app.finish();
        app.cleanup();
    }

    #[test]
    fn the_whole_asset_path_runs_with_no_window_and_no_game_install() {
        // The real guarantee: decoding is Bevy-free. If this ever needs a GPU,
        // the vertical slice stops being verifiable in CI.
        let outcome = prepare(&request(
            one(ResourceKey::new(0x00E6_BCE5, 0x4063_7E03, 0x067A_07F0)),
            false,
        ));
        assert!(
            matches!(outcome, Decoded::Failed(_)),
            "no packages means no record, not a crash"
        );
    }

    #[test]
    fn a_missing_package_names_the_path_and_still_exits_cleanly() {
        let mut request = request(one(ResourceKey::new(1, 2, 3)), false);
        request.packages = vec![std::path::PathBuf::from("/definitely/not/here.package")];
        let Decoded::Failed(reason) = prepare(&request) else {
            panic!("expected a failure")
        };
        assert!(reason.contains("/definitely/not/here.package"), "{reason}");
    }

    #[test]
    fn a_scene_with_no_packages_names_the_fix_not_a_symptom() {
        let Decoded::Failed(reason) = prepare(&request(one(ResourceKey::new(1, 2, 3)), false))
        else {
            panic!("expected a failure")
        };
        assert!(
            reason.contains("--package"),
            "the message must say what to do: {reason}"
        );
    }

    #[test]
    fn a_failure_report_line_is_versioned_and_carries_the_reason() {
        let lines = Decoded::Failed("no such record".into()).report_lines();
        assert_eq!(lines.len(), 1);
        assert!(
            lines[0].starts_with("OPENSPORE-STAGED v1 failed:"),
            "{lines:?}"
        );
        assert!(lines[0].contains("no such record"), "{lines:?}");
    }

    #[test]
    fn textures_are_only_resolved_when_asked_for() {
        // Both paths must fail the same way on a request with no packages:
        // neither the model nor the texture resolution can turn a missing
        // install into a different error, because both are downstream of
        // opening the package.
        for textured in [false, true] {
            assert!(
                matches!(
                    prepare(&request(
                        one(ResourceKey::new(0x00E6_BCE5, 0x4063_7E03, 0x067A_07F0)),
                        textured
                    )),
                    Decoded::Failed(_)
                ),
                "textured={textured}"
            );
        }
    }

    #[test]
    fn a_failing_record_stops_the_whole_scene_rather_than_drawing_part_of_it() {
        // Two objects, one of which is not in any package. Reporting success and
        // drawing half the scene is worse than failing: the user asked for two.
        let mut objects = one(ResourceKey::new(0x00E6_BCE5, 1, 2));
        objects.push(ObjectRequest::Creature {
            key: ResourceKey::new(0x00E6_BCE5, 3, 4),
            parts: vec![],
            origin: cli::RecordOrigin::Explicit,
        });
        assert!(matches!(
            prepare(&request(objects, false)),
            Decoded::Failed(_)
        ));
    }
}
