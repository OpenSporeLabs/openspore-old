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
//!
//!   and, for a creator document, one more branch that converges on the same
//!   two runtime modules:
//!
//!     -> spore_creator_core  <sporemodel> XML -> CreatorDocument  (no Bevy)
//!     -> spore_morphology    block -> part -> Projection          (no Bevy)
//!     -> creator             one entity per resolved block        (Bevy)
//!     -> loader/convert      the same mesh conversion, reused
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
//! * `--creator T:G:I` -- one `<sporemodel>` document, staged as a composition:
//!   one entity per resolved block, parented as the document nests, with every
//!   unresolved block reported by name.
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
pub mod creator;
pub mod domain;
pub mod editor;
pub mod loader;
pub mod scene;
pub mod texture;

#[cfg(feature = "brp")]
pub mod brp;

pub use creator::{CreatorDecoded, CreatorPlugin, StagedCreator};
pub use editor::{CreatorSession, SessionError};
pub use loader::{SceneOutcome, SporeAssetPlugin, WorldResources};
pub use scene::ScenePlugin;

use std::sync::Arc;

use bevy::pbr::StandardMaterial;
use bevy::prelude::*;
use bevy::window::{PresentMode, WindowResolution};

use cli::{CreatorRequest, LaunchRequest, SceneRequest, WindowOptions};
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
        Ok(LaunchRequest::Creator(creator)) => run_creator(creator),
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
    // `--sporemodel-package` companions are appended at the LOWEST priority:
    // they exist so `Spore_Game.package` can be searched for `sporemodel`
    // records, and a package named only for that must never shadow a record the
    // caller asked for by `--package`.
    let world = match domain::world::ResourceWorld::open(&scene.all_packages()) {
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
        scene.sporemodel,
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

/// Stages one creator document: decode outside Bevy, then either report and exit
/// or open a window on the projection.
///
/// The ordering is the same as [`run_scene`]'s and for the same reason: decoding
/// is I/O with nothing to do with rendering, so a missing package, an unknown
/// record type and a document that will not parse all become visible **before** a
/// window costs anything. `--info` therefore never constructs an `App` at all.
fn run_creator(request: CreatorRequest) -> std::process::ExitCode {
    info!(
        "openspore: creator mode, document={}, scale={}",
        request.document.as_str(),
        request.scale
    );

    let decoded = prepare_creator(&request);
    for line in decoded.report_lines() {
        println!("{line}");
    }
    let CreatorDecoded::Ready(stage) = decoded else {
        return std::process::ExitCode::from(cli::EXIT_ASSET as u8);
    };

    // The save happens before the `--info` exit, because a headless save is the
    // whole reason the flag exists: it is how "project, save, reload, project
    // again" gets run against real data.
    if let Some(path) = &request.save_to {
        if let Err(error) = creator::save_document(path, &stage.document) {
            eprintln!("openspore: {error}");
            return std::process::ExitCode::from(cli::EXIT_ASSET as u8);
        }
        println!("OPENSPORE-CREATOR-SAVE v1 path={}", path.display());
    }

    if request.info_only {
        return std::process::ExitCode::SUCCESS;
    }

    // The editor is the same decode and the same projection, with a UI and a
    // session bolted on. The `--info` path is above this and never reaches it, so
    // the headless gate stays headless: `DefaultPlugins` builds a winit
    // `EventLoop`, which panics off the main thread, so an "the editor assembles"
    // claim that runs in `cargo test` would be a claim about nothing.
    let exit = if request.editor {
        build_creator_editor_app(request.window.clone(), &request)
    } else {
        build_creator_app(request.window.clone(), *stage, request)
    }
    .run();
    match exit {
        AppExit::Success => std::process::ExitCode::SUCCESS,
        AppExit::Error(cause) => {
            error!("render loop exited with an error: {cause}");
            std::process::ExitCode::FAILURE
        }
    }
}

/// Decodes a creator request without touching Bevy, the GPU, or a window.
///
/// The twin of [`prepare`], and it obeys the same rule: the packages are opened
/// here, the document is parsed, resolved and projected here, and the app is only
/// assembled from data that already exists.
pub fn prepare_creator(request: &CreatorRequest) -> CreatorDecoded {
    let source = match request.document.to_document_source() {
        Ok(source) => source,
        Err(reason) => return CreatorDecoded::Failed(reason),
    };
    // The replacement goes in here, before resolution, so the new part walks the
    // same `blockref -> rigblock -> prop -> part` chain an authored part would and
    // every number in the runtime state describes the part that is actually there.
    match creator::load_creator_replacing(&source, &request.packages, request.replace) {
        Ok(stage) => CreatorDecoded::Ready(Box::new(stage)),
        Err(error) => CreatorDecoded::Failed(error.to_string()),
    }
}

/// Assembles the app that shows one projected creator document.
///
/// Three resources and two plugins, in this order:
///
/// 1. the projection, which [`CreatorPlugin`]'s `Startup` system spawns from;
/// 2. the runtime state, so the very first `Update` already has a complete row to
///    report -- and so an agent that polls before the spawn sees `scene_stable:
///    false` rather than a missing resource;
/// 3. the camera framing, which the `PostStartup` system reads, and only when the
///    projection actually resolved something.
///
/// [`ScenePlugin`] still owns the camera. This function spawns none.
pub fn build_creator_app(
    window: WindowOptions,
    stage: creator::CreatorStage,
    request: CreatorRequest,
) -> App {
    let selected = request.select;
    let state = stage.runtime_state(selected);
    let framing = stage.camera_framing();

    let mut app = base_app(window);
    app.insert_resource(StagedCreator {
        stage,
        selected,
        scale: request.scale,
    });
    app.insert_resource(state);
    if let Some(framing) = framing {
        app.insert_resource(framing);
    }
    app.add_plugins((ScenePlugin, CreatorPlugin));
    app.add_systems(Update, exit_on_escape);
    app
}

/// Assembles the **Creature Creator**: the editor screen over a staged document.
///
/// The viewer ([`build_creator_app`]) and the editor differ in one resource --
/// [`CreatorEditorState`] holds a [`CreatorSession`], which owns a *mutable*
/// document -- and one plugin. Everything else, including the decode that
/// happens before this function is ever called, is shared, so the two cannot
/// disagree about what the document is.
///
/// [`CreatorRuntimeState`] is inserted as its own resource here rather than
/// living inside [`CreatorEditorState`], because it is the thing BRP reads and a
/// BRP reader must not have to know about the editor's private state to ask "how
/// many blocks are on screen".
///
/// # Framing
///
/// The framing comes from the stage *before* the session takes it, because the
/// session clones the document and the stage is what holds the projection. Read
/// it here and insert it as a resource; the editor plugin does not re-frame,
/// since framing once per load is correct and framing every frame would fight the
/// user's orbiting.
pub fn build_creator_editor_app(window: WindowOptions, request: &CreatorRequest) -> App {
    use editor::plugin::{CreatorEditorPlugin, CreatorEditorState, LastPick};
    use editor::ui::spawn::CreatorUiView;

    let source = request
        .document
        .to_document_source()
        .unwrap_or_else(|reason| panic!("openspore: the editor cannot open a document: {reason}"));

    // The session opens its OWN package mapping rather than adopting the stage
    // the report lines were printed from. That is deliberate: a session has to
    // hold a live mapping to re-resolve an edited document, and the only honest
    // way to get one is to own it. Adopting the stage would have meant either
    // resolving the document twice on every launch, or keeping a reference into
    // a `ResourceWorld` that the stage does not own.
    let session = CreatorSession::open(&source, &request.packages, request.scale)
        .unwrap_or_else(|error| panic!("openspore: the editor cannot open its document: {error}"));

    let framing = session.stage().camera_framing();
    let mut runtime = session.stage().runtime_state(request.select);

    let mut session = session;
    if let Err(error) = session.select(request.select) {
        // A selection the document refuses is a typo in the command line, and
        // saying so is better than opening an editor with the wrong part lit.
        panic!(
            "openspore: the editor cannot select block {:?}: {error}",
            request.select
        );
    }
    runtime.document = session.stage().document_identity();

    let mut app = base_app(window);
    app.insert_resource(CreatorEditorState::new(session, CreatorUiView::default()));
    app.insert_resource(LastPick::default());
    app.insert_resource(editor::plugin::CreatorSelectionCommand::default());
    app.insert_resource(runtime);
    if let Some(framing) = framing {
        app.insert_resource(framing);
    }
    app.add_plugins((ScenePlugin, CreatorEditorPlugin));
    app.add_systems(Update, exit_on_escape);
    app
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
    // Development-only, and the one caller of `bevy_remote` in the workspace.
    // Compiled out entirely unless `--features brp`; see `brp`.
    #[cfg(feature = "brp")]
    brp::add(&mut app);
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
            sporemodel_packages: Vec::new(),
            objects,
            scale: 1.0,
            window: WindowOptions::default(),
            info_only: true,
            textured,
            layout: domain::Layout::Row,
            spacing: domain::scene::DEFAULT_GAP,
            flip_normal_y: false,
            sporemodel: None,
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
