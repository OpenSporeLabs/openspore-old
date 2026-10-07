//! # `CreatorEditorPlugin` — the assembly point.
//!
//! Everything else in [`editor`](crate::editor) is a piece; this is the thing
//! that puts them in the same app and, more importantly, the thing that keeps
//! them in the same **order**.
//!
//! ## The one rule this module exists to enforce
//!
//! A click in the viewport does not move an entity. It calls
//! [`CreatorSession::select`](session::CreatorSession::select), which changes
//! the session's selection, and the selection reaches the screen because the
//! spawn system reads it. Nothing else. So the UI, the picker and a future
//! script all drive selection through one call, and there is no path on which the
//! picture and the document disagree about what is selected.
//!
//! ## Frame order, and why each step is where it is
//!
//! | system | schedule | why there |
//! |---|---|---|
//! | [`spawn_creator_screen`] | `Startup` | the UI tree is static furniture; it is spawned once |
//! | [`spawn_or_respawn_creature`] | `Update`, **first** | `Commands::spawn` is deferred, so a respawn's entities do not exist until the next frame |
//! | [`read_viewport_click`] | `Update` | after the creature is on screen, or a click would pick against the previous document |
//! | [`refresh_view_model`] | `Update` | last: it reads the selection that `read_viewport_click` may have just changed |
//!
//! [`spawn_or_respawn_creature`] is deliberately **not** in `PostStartup` even
//! though nothing is deferred about it by then: it has to run again after every
//! edit, and a system that lives in `PostStartup` cannot. The camera framing
//! stays in `PostStartup` because framing once per load is correct and framing
//! every frame would fight the user's orbiting.
//!
//! ## The revision counter is the respawn trigger
//!
//! The session bumps [`CreatorSession::revision`] after every accepted edit and
//! **not** after a refused one. The spawn system compares that against the
//! revision it last built from. So "does the scene match the document" is one
//! integer comparison, a refused edit costs nothing, and there is no structural
//! guess about whether the world is stale.

use bevy::prelude::*;
use bevy::ui::ui_transform::UiGlobalTransform;

use crate::creator::{
    spawn_projection_skin, CreatorRootMarker, CreatorRuntimeState, DocumentSource,
};
use crate::editor::picking::{self, PickRay};
use crate::editor::session::CreatorSession;
use crate::editor::skin::SkinPalette;
use crate::editor::ui;
use crate::editor::ui::spawn::{spawn_creator_ui, CreatorUiPart, CreatorUiView};
use crate::editor::viewport::{CreatorViewport, CreatorViewportCamera};
use crate::scene::{GroundPlane, MainCamera, OnStage};
use std::num::NonZeroUsize;

/// Everything the editor needs to open, owned by the plugin.
#[derive(Debug)]
pub struct CreatorEditorRequest {
    /// Where the document comes from.
    pub source: DocumentSource,
    /// The packages to resolve against, opened once for the session's life.
    pub packages: Vec<std::path::PathBuf>,
    /// The world scale.
    pub scale: f32,
    /// Which block starts selected, if any.
    pub select: Option<usize>,
}

impl Plugin for CreatorEditorPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_creator_screen)
            .add_systems(Startup, spawn_viewport_camera)
            .add_systems(
                Startup,
                |mut commands: Commands, mut fonts: ResMut<Assets<Font>>| {
                    let coverage = ui::font::resolve(&mut fonts);
                    commands.insert_resource(coverage);
                },
            )
            .add_systems(
                PostStartup,
                (
                    claim_ui_camera,
                    remove_debug_ground,
                    frame_editor_camera,
                    // Independent of the framing; what it needs is to run AFTER
                    // `spawn_creator_screen`, which `PostStartup` guarantees and
                    // `Startup` would not (`Commands::spawn` is deferred).
                    crate::editor::backdrop::install_backdrop,
                    ui::font::apply,
                ),
            )
            .add_systems(Update, crate::editor::backdrop::publish_backdrop_geometry)
            // The bake is its own chain, and deliberately NOT part of the
            // editor's `Update` tuple: it is a forty-part decode plus a read-back,
            // and interleaving it with the per-frame selection and wheel systems
            // would put a second of work between a click and its result.
            // Chained, and the chain is load-bearing twice over. `create_atlas`
            // must run before `start_bake` because `start_bake` takes
            // `ResMut<AtlasHandle>` and a `Startup` system that runs first against a
            // resource that does not exist yet is a panic, not a deferral --
            // `ResMut` validation happens when the system runs. And
            // `prepare_palette` must precede `start_bake` because the bake reads
            // the catalogue it builds.
            .add_systems(
                Startup,
                (
                    create_atlas,
                    crate::editor::thumbnail_bake::prepare_palette,
                    crate::editor::thumbnail_bake::start_bake,
                )
                    .chain(),
            )
            .add_systems(
                Update,
                (
                    crate::editor::thumbnail_bake::request_capture,
                    crate::editor::thumbnail_bake::clear_staging,
                    crate::editor::thumbnail_bake::publish_bake_state,
                ),
            )
            .init_resource::<crate::editor::thumbnail_bake::PaletteState>()
            .init_resource::<crate::editor::thumbnail_bake::BakeState>()
            .add_systems(
                Update,
                (
                    spawn_or_respawn_creature,
                    // Right after the spawn: the content and the rectangle both
                    // describe what is on screen this frame, and both are one
                    // frame stale if they wait. `.chain()` below makes the order
                    // explicit rather than leaving it to the scheduler.
                    tag_viewport_content,
                    sync_creator_viewport,
                    frame_editor_camera_after_an_edit,
                    apply_selection_command,
                    read_viewport_click,
                    // Cell presses are a document edit, so they sit with the other
                    // edit-driven systems and before `refresh_view_model`, which
                    // republishes the selection they may have changed.
                    crate::editor::thumbnail_bake::read_part_cell_click,
                    wheel_scales_selection,
                    refresh_view_model,
                    crate::creator::tick_creator_runtime,
                    publish_viewport_state,
                )
                    .chain(),
            )
            .init_resource::<CreatorViewport>();
    }
}

/// Creates the atlas texture the part thumbnails are baked into.
///
/// A separate `Startup` system because it needs `Assets<Image>`, which
/// [`crate::editor::thumbnail_bake::start_bake`] does have -- but keeping the
/// creation here means the texture exists before anything can ask to render into
/// it, and one `init_resource` cannot allocate an asset handle.
fn create_atlas(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    let handle = images.add(crate::editor::thumbnail_bake::atlas_image());
    commands.insert_resource(crate::editor::thumbnail_bake::AtlasHandle(handle));
}

/// Spawns the camera the creature is rendered through.
///
/// # Why there are two cameras, when "one owner per entity kind" is this crate's rule
///
/// [`crate::editor::viewport`] explains the whole story; the short version is that
/// `bevy_ui` lays the interface out against `camera.physical_viewport_size()`, so
/// a camera with a sub-rect `Viewport` also *shrinks the UI* into that sub-rect.
/// Measured on Bevy 0.19.1: a `Val::Percent(60)` node is 768 wide with no viewport
/// and 461 wide once the viewport is 768 wide. Giving the single UI camera the
/// creature's rectangle therefore collapses the whole screen, and deriving that
/// rectangle from the laid-out node makes it a shrinking regress.
///
/// So the rule is kept rather than broken: `ScenePlugin` still owns exactly one
/// [`MainCamera`], and it stays full-window and owns the UI. This is a *second*
/// camera with a different marker, spawned by the editor, which owns the
/// rectangle. One rectangle, two cameras, no feedback loop.
///
/// Deliberately **not** marked [`MainCamera`]: every `MainCamera` query in this
/// crate is `.single()`- or count-based, so a second marked camera would make all
/// of them ambiguous at once.
fn spawn_viewport_camera(mut commands: Commands) {
    commands.spawn(crate::editor::viewport::viewport_camera_bundle(None));
}

/// Marks the existing [`MainCamera`] as the camera the UI is laid out against.
///
/// Load bearing and easy to miss. `DefaultUiCamera` picks the **highest-order**
/// camera targeting the primary window when none is marked — and the viewport
/// camera deliberately has a higher order. Without this marker `bevy_ui` silently
/// re-targets the interface at the 768x665 sub-rect camera and the layout
/// collapses. Executed on Bevy 0.19.1 to confirm the marker wins over `order`.
///
/// `PostStartup` because [`MainCamera`] is spawned by `ScenePlugin` in `Startup`
/// and `Commands::spawn` is deferred, so a query here is the first one that can
/// see it.
fn claim_ui_camera(mut commands: Commands, cameras: Query<Entity, With<MainCamera>>) {
    for entity in cameras.iter() {
        commands.entity(entity).insert(bevy::ui::IsDefaultUiCamera);
    }
}

/// Puts the creature and the lights on the viewport camera's render layer.
///
/// # Why this is needed at all
///
/// The viewport camera renders into a sub-rect that the UI camera also draws
/// into. Without layer separation the creature is drawn twice: once by the UI
/// camera across the whole window (where the panel covers most of it) and once by
/// the viewport camera. The second pass happens to overwrite the first at the same
/// depth, so the *picture* would look right while costing twice the draw calls —
/// which is exactly the kind of thing that reads as "fine" until the frame time
/// moves.
///
/// The lights must move too, or the creature renders unlit: a directional light on
/// layer 0 is invisible to a camera restricted to layer 1, and a black creature on
/// a black background is indistinguishable from a creature that is not there.
///
/// [`crate::editor::viewport::CreatorViewport`] notes that `RenderLayers` has no
/// effect on UI — neither `bevy_ui` nor `bevy_ui_render` mentions it — so
/// restricting the viewport camera cannot hide the interface.
///
/// The creature's entities are new every time the document changes, so this sweeps
/// every frame: tagging only at spawn time made the creature vanish on the first
/// edit and reappear on undo, because the replacement entities were never marked.
fn tag_viewport_content(
    mut commands: Commands,
    creatures: Query<Entity, (With<OnStage>, Without<GroundPlane>)>,
    lights: Query<Entity, With<DirectionalLight>>,
) {
    for entity in creatures.iter().chain(lights.iter()) {
        // `try_insert`, not `insert`: `spawn_or_respawn_creature` runs earlier in
        // this same chain and queues the previous creature for despawn, and the
        // despawn is applied at the end of the schedule -- so this frame's query
        // still returns those entities, and a plain `insert` on one panics with
        // "Entity despawned". The whole editor died on the first respawn because
        // of it. A missing render layer on an entity that is leaving is harmless.
        commands
            .entity(entity)
            .try_insert(bevy::camera::visibility::RenderLayers::layer(1));
    }
}

/// Publishes the laid-out UI node as the authoritative [`CreatorViewport`].
///
/// # The one writer, and why it runs every frame
///
/// The rectangle is derived from [`ComputedNode`] and [`UiGlobalTransform`], which
/// Taffy rewrites whenever the window resizes or the layout changes. Polling is
/// cheaper than diffing and cannot miss an update: there is no event to subscribe
/// to, and a layout that moved the node without a window event is not a thing the
/// editor can be told about.
///
/// Ordering: after [`spawn_or_respawn_creature`] and before
/// [`frame_editor_camera_after_an_edit`], so the camera is configured against the
/// rectangle for the content that is actually on screen. It reads the UI node laid
/// out in the previous frame's `PostUpdate`, which is the only one that exists —
/// `Commands::spawn` is deferred and layout has not run yet on the first frames.
fn sync_creator_viewport(
    viewports: ViewportNodes,
    mut viewport: ResMut<CreatorViewport>,
    mut cameras: Query<&mut Camera, With<CreatorViewportCamera>>,
) {
    // No node yet means the UI has not laid out. That is an ordinary state on the
    // first frames, so it leaves the resource `available: false` and the camera's
    // viewport `None` — which renders full-window, the pre-existing behaviour. It
    // is deliberately NOT the window rectangle: substituting that would make the
    // camera frame against, and the guard accept, a region that is not the 3D one.
    let next = viewports
        .iter()
        .next()
        .map(|(node, transform)| {
            // The scale factor is the camera's, read from the camera that renders
            // this node's target -- the same source `bevy_ui`'s own picking backend
            // uses. Falling back to 1.0 before `camera_system` has run is correct:
            // that is the factor it would report anyway.
            let scale = cameras
                .iter()
                .next()
                .and_then(|camera| camera.target_scaling_factor())
                .unwrap_or(1.0);
            CreatorViewport::from_node(node, transform, scale)
        })
        .unwrap_or_default();
    if next == *viewport {
        return;
    }
    *viewport = next;
    // Applied to the camera here rather than at spawn so a window resize moves the
    // viewport. `camera_system` runs in `PostUpdate` and detects the change via
    // `computed.old_viewport_size`, so an `Update`-time write lands the same frame.
    if let Ok(mut camera) = cameras.single_mut() {
        camera.viewport = next.camera_viewport();
    }
}

/// Republishes the viewport's geometry and its camera for BRP.
///
/// The four framing fields in [`CreatorRuntimeState`] are written by
/// [`crate::creator::tick_creator_runtime`] from the [`MainCamera`], which in the
/// editor is now the *UI* camera and is deliberately never moved. Overwriting them
/// would break the viewer's meaning of the same fields, so the editor adds its own
/// rather than repurposing theirs: `camera_position` describes the camera the
/// creature is seen through, and these describe how that camera is placed.
///
/// Runs after `tick_creator_runtime` so it wins, and reports `None` for a camera
/// that does not exist rather than leaving the previous frame's numbers in place.
fn publish_viewport_state(
    viewport: Res<CreatorViewport>,
    cameras: Query<&Transform, With<CreatorViewportCamera>>,
    framing: Option<Res<crate::loader::CameraFraming>>,
    mut runtime: ResMut<CreatorRuntimeState>,
) {
    runtime.viewport_available = viewport.available;
    runtime.viewport_physical_position = Some(viewport.physical_position);
    runtime.viewport_physical_size = Some(viewport.physical_size);
    runtime.viewport_scale_factor = viewport.scale_factor;

    let Some(camera) = cameras.iter().next() else {
        runtime.viewport_camera_position = None;
        runtime.viewport_camera_distance_to_centre = None;
        return;
    };
    let translation = camera.translation;
    runtime.viewport_camera_position = Some([translation.x, translation.y, translation.z]);
    // The same quantity the viewer publishes, over the same centre, so the two
    // are comparable numbers rather than two definitions of "framed".
    runtime.viewport_camera_distance_to_centre = framing.map(|framing| {
        let (lo, hi) = framing.bounds;
        let centre = Vec3::new(
            (lo[0] + hi[0]) * 0.5,
            (lo[1] + hi[1]) * 0.5,
            (lo[2] + hi[2]) * 0.5,
        );
        (translation - centre).length()
    });
}

/// The plugin itself. Zero-sized: the request is data on the session, not
/// configuration on a plugin, so there is exactly one place the request can be
/// read from.
#[derive(Debug, Clone, Copy, Default)]
pub struct CreatorEditorPlugin;

/// The session, as a resource so the UI can reach it.
///
/// A `Resource` and not a field on the plugin because every system that needs it
/// is a plain `fn`, and threading a plugin field through `Param`s would be a
/// second way to hold the same state.
#[derive(Resource, Debug)]
pub struct CreatorEditorState {
    /// The document, its history, and the projection.
    pub session: CreatorSession,
    /// The revision the scene was last built from.
    built_revision: u64,
    /// The selection the scene was last built for.
    ///
    /// Separate from `built_revision` because selection is **not** a document
    /// edit: it changes no block, so the revision does not move, so nothing would
    /// otherwise tell the spawn system the tint on screen is stale. The first
    /// version of this file had no such field, and the result was that clicking a
    /// part selected it correctly, applied it to the session, published it in two
    /// reflected resources -- and changed **nothing on screen**, because the
    /// selection tint is written once at spawn.
    ///
    /// A user reported "I cannot click anywhere", which was a true observation of
    /// a true defect: the click worked and was invisible. Selection that cannot be
    /// seen is not selection.
    built_selection: Option<usize>,
    /// Whether the camera has been framed on the projection **and found**.
    ///
    /// Separate from `built_revision` because the two genuinely diverge on the
    /// first frame: `ScenePlugin` spawns the camera in `Startup`, and
    /// `Commands::spawn` is deferred, so the respawn system runs before any
    /// camera exists. Folding "did I frame?" into "did I build?" would either
    /// claim a framing that never happened or drop the framing for the rest of
    /// the session. It is only set once a camera was actually found.
    framed: bool,
    /// The view the UI tree was spawned from, re-read each frame so the UI can
    /// reflect the selection without the UI owning any state.
    pub view: CreatorUiView,
}

/// The cursor pick from this frame, and the agent-facing control surface.
///
/// Reflected, and deliberately holding **OpenSpore's own plain data only**:
/// never a `Transform`, a `Handle`, or an entity id. In Bevy 0.19 `bevy_transform`,
/// `bevy_camera` and `bevy_mesh` register **zero** types for reflection, so a
/// BRP query for one of those fails with "isn't registered or used in the world"
/// and, under `strict: false`, fails *silently as zero rows* -- which reads
/// exactly like "the bridge is broken". See `AGENTS.md` and `docs/BRP.md`.
///
/// # Why this exists rather than "use the mouse"
///
/// The editor's primary interaction is a viewport click, and a synthetic click
/// injected over BRP does not reliably reach `ButtonInput<MouseButton>` or
/// `Window::cursor_position` on this platform -- which is a fact about the
/// *transport*, not about the picking, and it is exactly the kind of ambiguity
/// that gets recorded as "picking does not work". The picking maths is proved by
/// its own unit tests; what this resource adds is a way to drive and read the
/// same selection over a channel that works, so "does the selection reach the
/// document" is answerable at all.
#[derive(Resource, Reflect, Debug, Default, Clone)]
#[reflect(Resource)]
pub struct LastPick {
    /// The block the cursor hit most recently, or `None`.
    pub block: Option<u32>,
    /// The block the user has selected, or `None`.
    ///
    /// Distinct from `block`: `block` is a *hit*, this is the *selection*. A hit
    /// that is refused does not become a selection, and collapsing the two would
    /// make a refused selection look like a successful one.
    pub selected: Option<u32>,
    /// How the pick was made: `"world_bounds"`, or `"none"`.
    pub method: String,
    /// How many blocks the ray passed through, not just the winner.
    ///
    /// A creature's limb boxes contain each other, so this is normally more than
    /// one and "the part under the cursor" is a choice among them.
    pub hits: u32,
    /// Left-button press events this app actually observed, ever.
    ///
    /// The diagnostic that separates "the pointer never reached us" from "the
    /// pointer reached us and the pick was rejected". Those two failures look
    /// identical from the outside -- nothing gets selected -- and guessing between
    /// them is how a picking bug gets reported as a UI bug.
    pub presses_seen: u32,
    /// Presses that arrived but fell outside the viewport's rectangle.
    ///
    /// Non-zero with `presses_seen` non-zero means the guard is wrong, most likely
    /// a units mismatch: `ComputedNode::size` and `GlobalTransform` are in
    /// **physical** pixels while `Window::cursor_position` is in **logical** ones,
    /// and on any display where those differ the rectangle test rejects every
    /// cursor including ones plainly inside the viewport.
    pub rejected_outside_viewport: u32,
    /// The last cursor position observed, in the window's own units.
    pub last_cursor: Option<[f32; 2]>,
    /// The scale the last wheel gesture applied, or `None`.
    pub last_scale: Option<f32>,
    /// Wheel frames that changed a scale, ever: one history row each.
    ///
    /// This is the counter that makes the history granularity an observation
    /// instead of a claim. If undo needs more presses than this number to get back
    /// to where the scroll started, something else is writing history; if one press
    /// undoes a whole flick of several notches, the granularity is not the
    /// per-frame one documented on [`wheel_scales_selection`] any more.
    pub scale_applications: u32,
    /// Wheel frames that carried a real delta but landed outside the viewport.
    ///
    /// Non-zero means the cursor was over the panel while the wheel turned. That is
    /// correct behaviour once the grid pages on the wheel -- it is the thing that
    /// stops scrolling the catalog from resizing the creature -- so this is a
    /// measurement, not an error counter.
    pub wheel_rejected_outside_viewport: u32,
    /// The geometry of the last wheel guard evaluation, as it saw it.
    ///
    /// Present because the guard's correctness cannot be read off its own counter: a
    /// rejection counter of zero is right when the cursor is over the viewport and
    /// also right when the cursor never moved at all, and those two are the whole
    /// difference between a working guard and a broken input bridge. The four numbers
    /// are the inputs and the output, so the test can be redone by hand against real
    /// values instead of argued about.
    pub wheel_probe: Option<WheelProbe>,
    /// The coordinate mapping behind the last click, and its verdict.
    ///
    /// The click path's equivalent of [`Self::wheel_probe`], and the reason the
    /// viewport contract is checkable at all from outside the app: it carries the
    /// cursor, the rectangle it was tested against, the resulting NDC, and whether
    /// the ray was actually built. A selection that did not change is otherwise
    /// indistinguishable from a click that never reached the picker.
    pub pick_probe: Option<PickProbe>,
    /// Selection requests that the document refused, ever.
    ///
    /// A refusal is not an error state to clear; it is a count. A UI that silently
    /// dropped a refused selection would look identical to a UI that never
    /// received one.
    pub refused_selections: u32,
}

/// Spawns the screen's furniture. Once.
///
/// The creature is **not** spawned here: [`spawn_or_respawn_creature`] owns
/// that, and having two owners is how a respawn ends up spawning the creature
/// twice.
fn spawn_creator_screen(mut commands: Commands, state: Res<CreatorEditorState>) {
    spawn_creator_ui(&mut commands, &state.view);
}

/// Removes the debug ground plane.
///
/// # `PostStartup`, and that is the whole content of this note
///
/// The first version of this ran in `Startup`, alongside `ScenePlugin`'s own world
/// spawn, and it did nothing. Those two systems take disjoint parameters, so Bevy
/// runs them **in parallel** -- and `Commands::spawn` is deferred, so the query for
/// `GroundPlane` executed before the plane existed to be found. The removal
/// "succeeded" silently, every frame, forever.
///
/// `PostStartup` is the first schedule in which every `Startup` spawn has been
/// applied, which is the only reason a query can see anything that `Startup`
/// spawned. A system that needs to observe startup output has to run after
/// startup.
///
/// # Why it matters beyond tidiness
///
/// The plane is opaque, 400x400, at `y = -0.01`. `0x135ff098` is a 10-deep spine
/// whose centre sits at `y = -0.529`, so the plane sat between the camera (at
/// `y = +0.137`) and the creature. Every counter said the model was fine -- ten
/// blocks resolved, ten visible, a valid framing at distance 1.589 -- and the screen
/// was a grey floor. Nothing rendered *because it was all correctly there and then
/// covered up*, which is the hardest class of bug to spot from inside the model.
fn remove_debug_ground(mut commands: Commands, ground: Query<Entity, With<GroundPlane>>) {
    for entity in ground.iter() {
        commands.entity(entity).despawn();
    }
}

/// Respawns the creature when, and only when, the session has moved on.
fn spawn_or_respawn_creature(
    mut commands: Commands,
    mut state: ResMut<CreatorEditorState>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    existing: Query<Entity, (With<OnStage>, Without<CreatorRootMarker>)>,
    roots: Query<Entity, With<CreatorRootMarker>>,
    mut runtime: ResMut<CreatorRuntimeState>,
) {
    // Rebuild when the DOCUMENT changed **or** the SELECTION changed. Both feed
    // what is on screen -- the revision for geometry, the selection for the tint --
    // and conflating them means one of the two never triggers.
    if state.built_revision == state.session.revision()
        && state.built_selection == state.session.selection()
    {
        return;
    }
    // `Commands::spawn` is deferred and `despawn` is a command too, so the old
    // entities are gone next frame and the new ones appear the frame after. Two
    // frames of churn on an edit is acceptable; a leaked creature is not.
    for entity in existing.iter() {
        commands.entity(entity).despawn();
    }
    for entity in roots.iter() {
        commands.entity(entity).despawn();
    }

    let selection = state.session.selection();
    // The document's OWN skin colour, read from the document being edited. This is
    // the whole reason the creature is not grey any more, and it is one line
    // because the data was already parsed: `CreatorDocument::properties` has held
    // `<skincolor1..3>` since the parser was written.
    let palette = SkinPalette::from_document(state.session.document());
    let (outcome, _) = spawn_projection_skin(
        state.session.stage(),
        selection,
        state.session.scale(),
        palette,
        &mut commands,
        &mut meshes,
        &mut materials,
    );
    // The block table comes from the session's OWN projection rather than being
    // rebuilt here, so a BRP reader sees the same rows the editor is editing.
    runtime.blocks = state.session.stage().runtime_state(selection).blocks;
    runtime.resolved_count = outcome.resolved as u32;
    runtime.unresolved_count = outcome.unresolved as u32;
    runtime.visible_block_count = outcome.spawned as u32;
    runtime.selected_block = selection.map(|index| index as u32);
    runtime.total_triangles = outcome.triangles as u32;
    runtime.total_vertices = outcome.vertices as u32;
    runtime.total_bones = outcome.bones as u32;
    state.built_selection = selection;
    runtime.max_depth = state.session.stage().projection.max_depth() as u32;
    state.built_revision = state.session.revision();
}

/// Frames the camera on the projection, once, in `PostStartup`.
///
/// # Why the editor needs its own and does not get one for free
///
/// The viewer's framing lives in `CreatorPlugin`, and the editor deliberately
/// does **not** add `CreatorPlugin` -- it spawns the creature itself, and two
/// plugins owning one entity kind is the bug this crate's own docs call out
/// ("Camera order ambiguities", the duplicate-spawn-as-framing-bug). So the
/// editor frames too, using the *same* [`frame_camera`](crate::loader::frame_camera)
/// the viewer uses, because the framing maths is verified against 47 real
/// documents and must not be re-derived here.
///
/// The first version of this file did neither, and the symptom was a correct
/// creature rendered as a three-pixel speck at the default camera distance: the
/// document decoded, 1896 triangles spawned, 10 blocks were on screen, and the
/// camera sat at `ScenePlugin`'s `(0, 6, 34)` looking at nothing in particular.
/// Nothing in the runtime state said "unframed", because `camera_framed` is set
/// by the viewer's own tick and the editor had no equivalent -- which is the
/// failure mode this repository's trap list already names.
fn frame_editor_camera(
    mut cameras: Query<&mut Transform, With<CreatorViewportCamera>>,
    mut state: ResMut<CreatorEditorState>,
) {
    let Some(framing) = state.session.stage().camera_framing() else {
        return;
    };
    // No camera yet is a normal, expected state on the first frame, so it is not
    // an error -- and deliberately does NOT set `framed`, so the next attempt
    // happens rather than the session believing it framed a creature it never saw.
    //
    // Framed on the VIEWPORT camera, not on `MainCamera`: the creature is rendered
    // by the viewport camera, so framing the other one would point a camera
    // nothing is drawn through away from the creature while the visible camera sat
    // at `ScenePlugin`'s parking spot. That is the "framing bug's clothes" this
    // module's history already records once.
    if let Ok(mut transform) = cameras.single_mut() {
        crate::loader::frame_camera(&mut transform, framing.bounds, framing.vertical_fov_degrees);
        apply_side_on_view(&mut transform, framing.bounds);
        state.framed = true;
    }
}

/// Swings the camera so it looks ACROSS the creature's longest axis.
///
/// # Why, and it is not a nit
///
/// [`crate::loader::frame_camera`] uses a fixed 3/4 direction of `(0, 0.42, 0.91)`,
/// which reads well for a roughly isotropic subject and degenerates for anything
/// elongated. `0x135ff098` is a **28:1 sliver**: x extent 0.044 against z extent
/// 1.220. A camera on +Z looks straight down a tapering ten-segment spine, so the
/// spine reads as a vertical column of unrelated blobs rather than as a spine. The
/// creature is not wrong and the geometry is not wrong; the viewing axis is.
///
/// So the axis is chosen from the subject rather than from a guess about what a
/// creature looks like: the camera goes perpendicular to whichever world axis the
/// creature is longest along, which is the only rule that works for an arbitrary
/// part arrangement.
///
/// The first version of this picked the longest axis ITSELF, which is exactly
/// backwards -- for a Z-spine it put the camera back on +Z and reproduced the
/// end-on view it was written to fix.
///
/// # What it preserves
///
/// The centre and the distance stay exactly as [`crate::loader::frame_camera`] set
/// them; only the direction changes. So the verified framing arithmetic -- the part
/// that decides whether the creature FITS -- is untouched, and this decides only
/// whether it is seen from a useful angle.
fn apply_side_on_view(transform: &mut Transform, bounds: ([f32; 3], [f32; 3])) {
    let extent = [
        bounds.1[0] - bounds.0[0],
        bounds.1[1] - bounds.0[1],
        bounds.1[2] - bounds.0[2],
    ];
    let longest = match extent
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(core::cmp::Ordering::Equal))
    {
        Some((axis, _)) => axis,
        None => return,
    };
    let centre = Vec3::new(
        (bounds.0[0] + bounds.1[0]) * 0.5,
        (bounds.0[1] + bounds.1[1]) * 0.5,
        (bounds.0[2] + bounds.1[2]) * 0.5,
    );
    // Across = any axis that is NOT the long one, so the long axis runs across the
    // screen and its length is visible.
    let across = if longest == 0 { Vec3::Y } else { Vec3::X };
    // A 3/4 elevation on top, so the subject is never dead-on flat: an
    // axis-aligned camera makes a symmetric part look symmetric and hides
    // everything about its shape.
    let direction = (across * 0.92 + Vec3::new(0.0, 0.42, 0.0)).normalize();
    // Keep the distance `frame_camera` chose: the framing arithmetic decides
    // whether the creature FITS and is verified against 47 real documents, so only
    // the direction changes here and never the distance.
    let distance = (transform.translation - centre).length().max(1e-3);
    *transform =
        Transform::from_translation(centre + direction * distance).looking_at(centre, Vec3::Y);
}

/// Re-frames after an edit, because a new part changes the extent.
///
/// Only when the creature was actually rebuilt, so this costs one query per edit
/// rather than one per frame. A creature the user has zoomed into is re-framed
/// on edit -- which is defensible while the extent is what decides the framing,
/// and is the thing to revisit once the camera has user orbit on it.
fn frame_editor_camera_after_an_edit(
    cameras: Query<&mut Transform, With<CreatorViewportCamera>>,
    state: ResMut<CreatorEditorState>,
) {
    if state.framed && state.built_revision == state.session.revision() {
        return;
    }
    frame_editor_camera(cameras, state);
}

/// Turns a click in the viewport into a selection change.
fn read_viewport_click(
    buttons: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window>,
    cameras: Query<(&Camera, &GlobalTransform), With<CreatorViewportCamera>>,
    viewport: Res<CreatorViewport>,
    mut state: ResMut<CreatorEditorState>,
    mut last_pick: ResMut<LastPick>,
) {
    // Only act on an actual click. This system runs EVERY frame, and the first
    // version called `session.select(None)` whenever the cursor was not over
    // anything -- so it cleared the selection 60 times a second and quietly
    // undid any selection made by `CreatorSelectionCommand`, the launch flag, or
    // the frame before. The symptom was the two resources disagreeing:
    // `applied: 5, applied_count: 1` from the command while `selected_block` stayed
    // `null`, which reads exactly like "selection does not reach the document".
    //
    // "Not a click" and "a click that hit nothing" are different events and only
    // the second one is allowed to change the selection.
    if !buttons.just_pressed(MouseButton::Left) {
        return;
    }
    last_pick.presses_seen += 1;
    if let Some(cursor) = windows.iter().find_map(Window::cursor_position) {
        last_pick.last_cursor = Some([cursor.x, cursor.y]);
    }
    let (block, hits, probe) = pick_under_cursor(
        &buttons,
        &windows,
        &cameras,
        &viewport,
        state.session.projection(),
        &mut last_pick.rejected_outside_viewport,
    );
    last_pick.hits = hits;
    last_pick.pick_probe = probe;
    // Same block clicked twice in a row is not a new event, so re-running the
    // selection would only push another identical row into the history.
    if block.map(|index| index as u32) == last_pick.block && block.is_some() {
        last_pick.selected = block.map(|index| index as u32);
        return;
    }
    last_pick.block = block.map(|index| index as u32);
    last_pick.method = if block.is_some() {
        "world_bounds".to_owned()
    } else {
        "none".to_owned()
    };
    match state.session.select(block) {
        Ok(()) => last_pick.selected = block.map(|index| index as u32),
        Err(error) => {
            last_pick.refused_selections += 1;
            warn!("selection refused: {error}");
        }
    }
}

/// The block under the cursor, or `None`.
///
/// Returns `None` for every "not a pick" case alike -- no click, no cursor, no
/// camera, no laid-out viewport, a click on the panel, a cursor the camera cannot
/// convert. They are one answer because the caller has one job: decide whether
/// the selection changed. A caller that needed to tell "missed" from "not a
/// click" would be building a different feature.
///
/// The third value is a [`PickProbe`]: the inputs and the verdict of the
/// coordinate mapping, so the chain can be checked from outside instead of
/// inferred from a selection that may or may not have moved.
///
/// # The chain, in four steps
///
/// ```text
///   Window::cursor_position        logical px, window top-left, +y down
///     -> CreatorViewport gate      physical -> logical, reject if outside
///     -> Camera::viewport_to_world Bevy subtracts the logical viewport min itself
///     -> picking::pick_all         the unchanged ray/AABB maths
/// ```
///
/// Step three passes the **raw window cursor** through untouched, which is what
/// upstream does (`bevy_picking-0.19.1/src/backend.rs` feeds
/// `pointer_loc.position` straight in). It looks like a missing conversion and is
/// not: `viewport_to_ndc` computes `(position - logical_rect.min) / size`, so
/// subtracting the offset here as well would double-subtract it and the ray would
/// come out shifted by the viewport's origin.
fn pick_under_cursor(
    buttons: &ButtonInput<MouseButton>,
    windows: &Query<&Window>,
    cameras: &Query<(&Camera, &GlobalTransform), With<CreatorViewportCamera>>,
    viewport: &CreatorViewport,
    projection: &spore_morphology::Projection,
    rejected: &mut u32,
) -> (Option<usize>, u32, Option<PickProbe>) {
    if !buttons.just_pressed(MouseButton::Left) {
        return (None, 0, None);
    }
    let Some(cursor) = windows.iter().find_map(Window::cursor_position) else {
        return (None, 0, None);
    };
    let Some((camera, camera_transform)) = cameras.iter().next() else {
        return (None, 0, None);
    };

    let mut probe = PickProbe {
        cursor_logical: [cursor.x, cursor.y],
        ndc: [0.0, 0.0],
        scale: viewport.scale_factor,
        rect_position: viewport.physical_position,
        rect_size: viewport.physical_size,
        inside: false,
        had_viewport: viewport.available,
        mapped: false,
    };

    // A click on the panel is not a click in the viewport. The gate is the SAME
    // `CreatorViewport` the camera's viewport was set from, so the region the user
    // can click and the region the creature is drawn in cannot disagree -- which
    // is the defect this whole module exists to remove, and which the old code
    // could not have: it compared the cursor against the UI node while the ray was
    // built from the camera, and the two were separately derived.
    //
    // A cursor outside is rejected, never clamped to the edge. Clamping would
    // fabricate a click on whatever sits against the border.
    let Some(ndc) = viewport.cursor_to_ndc(cursor) else {
        probe.inside = false;
        *rejected += 1;
        return (None, 0, Some(probe));
    };
    probe.inside = true;
    probe.ndc = [ndc.x, ndc.y];

    // The camera's own viewport is the last thing that could disagree with the
    // gate, and it is checked rather than trusted: if the sync system has not run,
    // or has written a different rectangle, the mapping above and the ray below
    // would be computed against two different frames. That is the one case the old
    // code refused, and refusing is still right -- but it is now a statement about
    // a real inconsistency rather than about the camera having a viewport at all.
    if camera.viewport.as_ref().map(|view| {
        (
            [view.physical_position.x, view.physical_position.y],
            [view.physical_size.x, view.physical_size.y],
        )
    }) != Some((viewport.physical_position, viewport.physical_size))
    {
        warn!(
            "camera viewport {:?} disagrees with the measured UI viewport {:?}; \
             refusing rather than picking against the wrong frame",
            camera.viewport, viewport
        );
        return (None, 0, Some(probe));
    }

    let Ok(ray) = camera.viewport_to_world(camera_transform, cursor) else {
        return (None, 0, Some(probe));
    };
    probe.mapped = true;
    let hits = picking::pick_all(PickRay::from(ray), projection);
    (
        hits.first().map(|hit| hit.block),
        hits.len() as u32,
        Some(probe),
    )
}

/// The gate's verdict, with every number it compared.
///
/// # One rectangle, one predicate, three consumers
///
/// The click gate, the wheel gate and the pick ray all need "is this window-space
/// cursor inside the Creator viewport?". They used to answer it three different
/// ways -- the click and the wheel each tested [`bevy_ui::ComputedNode`] and the ray
/// was built from a camera whose viewport was the whole window. Three rectangles,
/// and the defect was that the region you could *see* was not the region you could
/// *click*. All three now read [`CreatorViewport`], so there is no second
/// reconstruction left to drift.
///
/// This function is a *probe* rather than the predicate itself so the wheel can
/// publish what it compared. Returning only a `bool` from a guard nobody can see
/// inside is how `inside: true` with `had_node: false` stayed indistinguishable
/// from a real pass -- and that pair is exactly what found both bugs in `a780d85`.
///
/// # Why not `ComputedNode::contains_point` any more
///
/// It is the better test: it inverts the node's transform and runs the same
/// signed-distance function the renderer draws rounded rectangles with, so it stays
/// correct under rotation and border radius. It is dropped because it answers a
/// *different question* -- it tests the UI node, not the viewport rectangle -- and
/// keeping it would mean the clickable region and the rendered region were derived
/// separately again, which is the defect.
///
/// The rectangle is exact for this node: `layout::viewport()` sets no
/// `border_radius` and no rotation, so the axis-aligned rect and the SDF agree.
/// That is stated rather than assumed, because the day a node is rounded or
/// animated this predicate becomes approximate and the honest fix is to teach
/// [`CreatorViewport`] the radius -- not to reintroduce a second geometry path.
///
/// # No measurement yet is not "inside"
///
/// When the UI has not laid out, [`CreatorViewport::available`] is `false` and this
/// reports `inside: false`. The previous code answered `true`, reasoning that an
/// absent node is "no opinion". That reasoning is what let every panel click through
/// for the whole of `a780d85`: the guard had no measurement, took the permissive
/// branch, and reported success. Failing closed for a few frames is visible; failing
/// open is invisible.
/// The inputs and the verdict, in the units the gate compared.
fn viewport_probe(cursor: Vec2, viewport: &CreatorViewport) -> WheelProbe {
    let scale = viewport.scale_factor;
    let point = pointer_in_node_space(cursor, scale);
    WheelProbe {
        cursor_logical: [cursor.x, cursor.y],
        point_physical: [point.x, point.y],
        scale,
        node_size: [
            viewport.physical_size[0] as f32,
            viewport.physical_size[1] as f32,
        ],
        node_position: viewport.physical_position,
        inside: viewport.cursor_to_ndc(cursor).is_some(),
        had_node: viewport.available,
    }
}

/// Logical cursor -> the physical-pixel space a [`ComputedNode`] is measured in.
///
/// One function, because the click path and the wheel path must not disagree about
/// which space a cursor is in; a guard that measures the wheel in logical units and
/// the click in physical ones accepts in one place and refuses in the other, with no
/// way to tell from the counters which is which.
fn pointer_in_node_space(cursor: Vec2, scale: f32) -> Vec2 {
    cursor * scale
}

/// An agent- or keyboard-issued selection, applied to the real document.
///
/// # Why this exists, and it is not a workaround for a broken picker
///
/// The picker is proved by its own unit tests and by the `--select-block` launch
/// path. What is missing is a *transport*: on this platform a synthetic click
/// injected over BRP reports `"Mouse button clicked successfully"` and never
/// reaches `ButtonInput<MouseButton>` or `Window::cursor_position`, so the
/// viewport ray is never built. That is a fact about the input transport, and
/// recorded as "the picker does not work" it would be wrong.
///
/// This resource makes selection reachable over a channel that does work, so the
/// question "does a selection reach the document" is answerable at all -- and the
/// brief asks for exactly this (`creator_select_block`).
///
/// One field in, one out: [`request`](Self::request) is what the caller asked
/// for, [`applied`](Self::applied) is what the document accepted. They differ
/// when the request is refused, and that difference is the whole point.
#[derive(Resource, Reflect, Debug, Default, Clone)]
#[reflect(Resource)]
pub struct CreatorSelectionCommand {
    /// The block to select, or `None` to clear the selection.
    ///
    /// Written by a caller. Read and immediately overwritten by
    /// [`apply_selection_command`], so a stale request cannot re-fire.
    pub request: Option<u32>,
    /// What the last request resolved to, or `None` if it was refused or absent.
    pub applied: Option<u32>,
    /// How many commands have been applied.
    pub applied_count: u32,
}

/// Applies a [`CreatorSelectionCommand`] to the session.
fn apply_selection_command(
    mut commands: ResMut<CreatorSelectionCommand>,
    mut state: ResMut<CreatorEditorState>,
) {
    let Some(request) = commands.request.take() else {
        return;
    };
    let index = Some(request as usize);
    match state.session.select(index) {
        Ok(()) => {
            commands.applied = index.map(|value| value as u32);
            commands.applied_count += 1;
        }
        Err(error) => {
            // The request is cleared either way. Leaving it set would retry every
            // frame forever, and a refusal that retries silently is a spin.
            commands.applied = None;
            warn!("selection request {request:?} refused: {error}");
        }
    }
}

/// Selects a block from outside the mouse -- a keyboard shortcut, or an agent
/// over BRP.
///
/// Exists because a synthetic click is not a reliable transport, and because the
/// brief asks for agent-facing creator tooling: this is `creator_select_block`,
/// the smallest piece that makes selection answerable without a pointer.
///
/// Returns `false` when the index does not name a block, and leaves the selection
/// **unchanged** in that case. Silently selecting the nearest block instead would
/// make a typo look like success.
pub fn select_block_from_outside(state: &mut CreatorEditorState, block: Option<usize>) -> bool {
    match state.session.select(block) {
        Ok(()) => true,
        Err(error) => {
            warn!("selection refused: {error}");
            false
        }
    }
}

/// The viewport's laid-out UI node and its screen transform.
///
/// A named alias because `clippy::type_complexity` is right: this is a five-part
/// type that appears in a system signature and in a helper's, and a caller that
/// misspelled one of them would get a confusing error from deep inside a
/// function.
type ViewportNodes<'w, 's> = Query<
    'w,
    's,
    (&'static ComputedNode, &'static UiGlobalTransform),
    (With<CreatorUiPart>, With<Viewport>),
>;

/// Marker so the viewport's UI node can be found by kind.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Viewport;

/// Scales the selected block with the mouse wheel.
///
/// # The interaction is the ORIGINAL's, not a generic editor's
///
/// The reference frame states it in as many words -- "Scroll the mouse wheel to
/// scale parts" -- so this is wheel-over-the-viewport on the selected block, with no
/// gizmo, no axis handles and no modifier keys. A 3-axis gizmo would be a modern-
/// editor affordance the original does not have.
///
/// # Why the factor is multiplicative and clamped
///
/// Multiplicative so repeated scrolls compose the way a user expects (each notch is
/// the same relative change, not the same absolute one), and clamped so a long
/// scroll cannot drive the scale to zero or infinity -- both of which the corpus
/// does contain: authored block scales run 0.28 .. 2.254181, and a zero scale
/// collapses the part to a point while an unbounded one makes the next
/// re-projection's bounds meaningless.
///
/// # History granularity is one FRAME, and gesture coalescing is NOT implemented
///
/// [`bevy::input::mouse::AccumulatedMouseScroll`] is cleared by Bevy every frame, so
/// this system runs at most once per frame and each run that changes the scale
/// records exactly one history row. Two consequences, stated because the earlier
/// version of this comment claimed the opposite:
///
/// - One *gesture* is many frames and therefore many undo steps. Coalescing them
///   would need a "replace the top of the history" operation on a snapshot history,
///   which changes documented semantics of [`crate::editor::session`] and is not
///   done here. Until it is, undo steps frame by frame. This is a UX ceiling, not a
///   defect, and it is counted rather than assumed: see
///   [`LastPick::scale_applications`] and [`LastPick::wheel_rejected_outside_viewport`].
/// - Several notches inside ONE frame collapse to a single factor, because only
///   the sign of the accumulated delta is used. A fast flick therefore scales less
///   than its notch count suggests. Using the magnitude would make one frame's
///   worth of notches frame-rate dependent, so the sign is the more defensible of
///   the two and the behaviour is documented instead of tuned away.
fn wheel_scales_selection(
    wheel: Res<bevy::input::mouse::AccumulatedMouseScroll>,
    keys: Res<ButtonInput<KeyCode>>,
    windows: Query<&Window>,
    viewport: Res<CreatorViewport>,
    mut state: ResMut<CreatorEditorState>,
    mut last_pick: ResMut<LastPick>,
) {
    let delta = wheel.delta.y;
    if delta.abs() < f32::EPSILON {
        return;
    }
    let Some(index) = state.session.selection() else {
        return;
    };
    // The wheel is not only a scale control: it is also how a grid pages. A cursor
    // over the panel must not resize the creature, so this shares the click path's
    // guard -- now literally the same call on the same [`CreatorViewport`], so the
    // two cannot disagree and the scaling factor no longer has to be looked up from
    // a camera here: the rectangle carries the factor it was measured under.
    let Some(cursor) = windows.iter().find_map(Window::cursor_position) else {
        return;
    };
    let probe = viewport_probe(cursor, &viewport);
    last_pick.wheel_probe = Some(probe);
    if !probe.inside {
        last_pick.wheel_rejected_outside_viewport += 1;
        return;
    }
    // Shift is the documented coarse step, not an invention: fine is 1.05, coarse
    // 1.35, so a shift-scroll crosses a useful range in a few notches while a plain
    // scroll stays precise.
    let coarse = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
    let factor = if coarse { 1.35 } else { 1.05 };
    let current = state
        .session
        .document()
        .blocks
        .get(index)
        .map(|block| block.transform.scale)
        .unwrap_or(1.0);
    let next = (current * if delta > 0.0 { factor } else { 1.0 / factor })
        .clamp(MIN_PART_SCALE, MAX_PART_SCALE);
    if (next - current).abs() < f32::EPSILON {
        // Already at a limit. Recording nothing keeps undo from having a step that
        // changes nothing, which is the difference between "at the limit" and
        // "broken".
        return;
    }
    if let Err(error) = state.session.apply_scale(index, next) {
        warn!("scale refused: {error}");
        return;
    }
    last_pick.last_scale = Some(next);
    last_pick.scale_applications += 1;
}

/// The smallest scale a part may be scaled to.
///
/// Inside the corpus: authored block scales run 0.28 .. 2.254181, so a floor below
/// that admits the user to write documents the game has never contained, and a
/// floor at 0.28 would refuse to let them undo back out of it.
pub const MIN_PART_SCALE: f32 = 0.05;

/// The largest scale a part may be scaled to.
///
/// **OpenSpore's own ceiling**, and deliberately generous rather than matching the
/// corpus maximum: an editor should not refuse a change the user asked for just
/// because the shipped content never did it. It exists to bound the re-projection,
/// not to police authoring.
pub const MAX_PART_SCALE: f32 = 8.0;

/// Mirrors the session into the UI's view model.
///
/// The one direction the data flows. The UI never writes the view: a button
/// changes the session, and the session is reflected here. That is what keeps
/// "the picture" and "the document" from becoming two answers.
fn refresh_view_model(
    mut state: ResMut<CreatorEditorState>,
    mut runtime: ResMut<CreatorRuntimeState>,
    palette: Res<crate::editor::thumbnail_bake::PaletteState>,
) {
    let blocks = state.session.stage().block_count();
    state.view.active_tab = state.view.active_tab.min(blocks.max(1).saturating_sub(1));

    // The grid counts the CATALOGUE -- the parts the user can choose from -- and
    // not the blocks the current document happens to have.
    //
    // It used to count `projection().resolved().count()`, which is the document's
    // block count, so a ten-block spine offered ten parts and the reference's
    // forty could never appear. Worse, the number moved whenever the document was
    // edited: adding a block would add a cell to the parts grid. Two different
    // questions, one counter.
    let drawable = palette
        .palette
        .as_ref()
        .map(|catalogue| catalogue.drawable())
        .unwrap_or(0);
    state.view.cells = drawable;
    let pages = drawable
        .max(1)
        .div_ceil(crate::editor::thumbnail_bake::TILES_PER_PAGE);
    state.view.pages = NonZeroUsize::new(pages).unwrap_or(NonZeroUsize::MIN);

    // The selected block is republished EVERY frame, not only when the revision
    // moves. Selecting a block changes no document state, so the revision does not
    // change, so `spawn_or_respawn_creature` does not run -- and a reader watching
    // `selected_block` would watch a stale `null` while the editor plainly had a
    // selection. That was a real bug: `creator_select_block` reported
    // `applied: 5, applied_count: 1` from the command resource while
    // `selected_block` stayed `null` in the runtime state, so the two resources
    // disagreed about whether anything was selected.
    runtime.selected_block = state.session.selection().map(|index| index as u32);
    runtime.skin = SkinPalette::from_document(state.session.document())
        .map(|palette| palette.describe())
        .unwrap_or_else(|| "no <skincolor*> in this document".to_owned());
}

/// Opens a session from a request, for `lib::build_creator_editor_app`.
pub fn open_session(request: &CreatorEditorRequest) -> Result<CreatorSession, String> {
    CreatorSession::open(&request.source, &request.packages, request.scale)
        .map_err(|error| error.to_string())
}

/// Marks the session's initial selection, if the request named one.
pub fn apply_initial_selection(
    session: &mut CreatorSession,
    select: Option<usize>,
) -> Result<(), String> {
    session.select(select).map_err(|error| error.to_string())
}

/// What the wheel guard saw on its last evaluation.
///
/// The inputs and the verdict, in the units the guard actually compared, so the
/// comparison can be repeated from the outside without trusting the verdict. It
/// exists because the guard's correctness cannot be read off its own counter: a
/// rejection count of zero is right when the cursor is over the viewport, and also
/// right when the cursor never moved, and also right when the guard is matching no
/// node at all. Those three were indistinguishable until this was published, and the
/// third is the one that was true.
#[derive(Reflect, Debug, Clone, Copy, PartialEq)]
pub struct WheelProbe {
    /// The cursor, in `Window::cursor_position` (logical) units.
    pub cursor_logical: [f32; 2],
    /// The same point after conversion, in `ComputedNode` (physical) units.
    pub point_physical: [f32; 2],
    /// The camera's target scaling factor -- the multiplier between the two.
    pub scale: f32,
    /// The viewport rectangle's measured size, in physical units.
    pub node_size: [f32; 2],
    /// The viewport rectangle's measured top-left, in physical units.
    ///
    /// Published because `node_size` alone cannot distinguish "a viewport at the
    /// window's left edge" from "a viewport starting half way across", and those
    /// two disagree about where every cursor is. With both numbers the guard's
    /// verdict can be recomputed by hand instead of believed.
    pub node_position: [u32; 2],
    /// Whether the guard accepted the point.
    pub inside: bool,
    /// Whether a viewport node existed to compare against at all.
    ///
    /// `false` means the guard **did not run**: it took its no-opinion branch and
    /// accepted everything, which is indistinguishable from a pass unless stated.
    pub had_node: bool,
}

/// What the click guard and the cursor mapping saw on the last click.
///
/// Published for the same reason as [`WheelProbe`], and it is the one artefact that
/// makes the whole coordinate chain observable: the cursor as the window reported
/// it, the rectangle it was tested against, the NDC the mapping produced, and
/// whether `viewport_to_world` then accepted it. Every step of
///
/// ```text
///   window cursor -> viewport-local -> NDC -> ray -> block
/// ```
///
/// can be recomputed from these eight numbers, which is how the live check proves
/// the chain rather than asserting that a selection moved.
#[derive(Reflect, Debug, Clone, Copy, PartialEq)]
pub struct PickProbe {
    /// The cursor, in `Window::cursor_position` (logical) units.
    pub cursor_logical: [f32; 2],
    /// The NDC the mapping produced. Meaningless when [`Self::inside`] is `false`.
    pub ndc: [f32; 2],
    /// The logical-to-physical factor the rectangle was measured under.
    pub scale: f32,
    /// The rectangle's physical top-left, as the gate tested it.
    pub rect_position: [u32; 2],
    /// The rectangle's physical size, as the gate tested it.
    pub rect_size: [u32; 2],
    /// Whether the cursor was inside the rectangle.
    pub inside: bool,
    /// Whether a rectangle had been measured at all.
    ///
    /// `false` means the guard did not run against a real rectangle, which is
    /// distinct from a cursor that was measured and found outside.
    pub had_viewport: bool,
    /// Whether `viewport_to_world` accepted the cursor and produced a ray.
    pub mapped: bool,
}

/// The normal-mode constant, re-exported so a caller building an editor app
/// cannot spell it differently from the spawn system.
pub use crate::loader::NORMAL_MODE as EDITOR_NORMAL_MODE;
impl CreatorEditorState {
    /// Wraps a session and a view into the editor's resource.
    ///
    /// [`built_revision`] starts at `u64::MAX` on purpose. The session's own
    /// revision starts at 0, and a "could not happen" sentinel of 0 would make
    /// the first frame skip the spawn and leave the creature a frame late for no
    /// stated reason. A sentinel that is *actually* impossible is the only kind
    /// worth having.
    pub fn new(session: CreatorSession, view: CreatorUiView) -> Self {
        Self {
            session,
            built_revision: u64::MAX,
            built_selection: None,
            framed: false,
            view,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::pointer_in_node_space;

    #[test]
    fn the_cursors_own_units_are_logical_and_the_nodes_are_physical() {
        // The conversion Bevy's own picking backend runs
        // (`picking_backend.rs`: `pointer_position * target_scaling_factor`). A
        // scale of 1.0 is this machine's, which is precisely why assuming it is the
        // bug: the conversion is invisible here and load bearing everywhere else.
        let cursor = bevy::math::Vec2::new(300.0, 400.0);
        assert_eq!(
            pointer_in_node_space(cursor, 1.0),
            cursor,
            "at scale 1 the two spaces coincide"
        );
        assert_eq!(
            pointer_in_node_space(cursor, 2.0),
            bevy::math::Vec2::new(600.0, 800.0),
            "a HiDPI target doubles every cursor coordinate"
        );
        // Both axes, because a half-applied conversion passes on a square display.
        assert_eq!(
            pointer_in_node_space(bevy::math::Vec2::new(11.0, 23.0), 1.5),
            bevy::math::Vec2::new(16.5, 34.5)
        );
    }
}
