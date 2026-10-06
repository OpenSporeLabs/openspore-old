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

use crate::creator::{
    spawn_projection_skin, CreatorRootMarker, CreatorRuntimeState, DocumentSource,
};
use crate::editor::picking::{self, PickRay};
use crate::editor::session::CreatorSession;
use crate::editor::skin::SkinPalette;
use crate::editor::ui;
use crate::editor::ui::spawn::{spawn_creator_ui, CreatorUiPart, CreatorUiView};
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
            .add_systems(
                Startup,
                |mut commands: Commands, mut fonts: ResMut<Assets<Font>>| {
                    let coverage = ui::font::resolve(&mut fonts);
                    commands.insert_resource(coverage);
                },
            )
            // The framing lives here rather than in `CreatorPlugin` because the
            // editor does not add `CreatorPlugin`: it spawns the creature itself,
            // and two owners of one entity kind is how this crate once got
            // "Camera order ambiguities" and a framing bug's clothes.
            .add_systems(PostStartup, (frame_editor_camera, ui::font::apply))
            .add_systems(
                Update,
                (
                    spawn_or_respawn_creature,
                    frame_editor_camera_after_an_edit,
                    apply_selection_command,
                    read_viewport_click,
                    refresh_view_model,
                    crate::creator::tick_creator_runtime,
                )
                    .chain(),
            );
    }
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
fn spawn_creator_screen(
    mut commands: Commands,
    state: Res<CreatorEditorState>,
    ground: Query<Entity, With<GroundPlane>>,
) {
    spawn_creator_ui(&mut commands, &state.view);
    // The debug ground plane is opaque and 400x400 at y = -0.01, so it buries any
    // creature whose centre sits below that -- and `0x135ff098` centres at
    // y = -0.529. Every symptom said the model was fine: ten blocks resolved, ten
    // visible, a valid framing. The screen was a grey floor. See
    // `scene::GroundPlane`.
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
    mut cameras: Query<&mut Transform, With<MainCamera>>,
    mut state: ResMut<CreatorEditorState>,
) {
    let Some(framing) = state.session.stage().camera_framing() else {
        return;
    };
    // No camera yet is a normal, expected state on the first frame, so it is not
    // an error -- and deliberately does NOT set `framed`, so the next attempt
    // happens rather than the session believing it framed a creature it never saw.
    if let Ok(mut transform) = cameras.single_mut() {
        crate::loader::frame_camera(&mut transform, framing.bounds, framing.vertical_fov_degrees);
        state.framed = true;
    }
}

/// Re-frames after an edit, because a new part changes the extent.
///
/// Only when the creature was actually rebuilt, so this costs one query per edit
/// rather than one per frame. A creature the user has zoomed into is re-framed
/// on edit -- which is defensible while the extent is what decides the framing,
/// and is the thing to revisit once the camera has user orbit on it.
fn frame_editor_camera_after_an_edit(
    cameras: Query<&mut Transform, With<MainCamera>>,
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
    cameras: Query<(&Camera, &GlobalTransform), With<MainCamera>>,
    viewports: ViewportNodes,
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
    let (block, hits) = pick_under_cursor(
        &buttons,
        &windows,
        &cameras,
        &viewports,
        state.session.projection(),
        &mut last_pick.rejected_outside_viewport,
    );
    last_pick.hits = hits;
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
/// camera, no viewport node, a click on the panel, a cursor the camera cannot
/// convert. They are one answer because the caller has one job: decide whether
/// the selection changed. A caller that needed to tell "missed" from "not a
/// click" would be building a different feature.
fn pick_under_cursor(
    buttons: &ButtonInput<MouseButton>,
    windows: &Query<&Window>,
    cameras: &Query<(&Camera, &GlobalTransform), With<MainCamera>>,
    viewports: &ViewportNodes,
    projection: &spore_morphology::Projection,
    rejected: &mut u32,
) -> (Option<usize>, u32) {
    if !buttons.just_pressed(MouseButton::Left) {
        return (None, 0);
    }
    let Some(cursor) = windows.iter().find_map(Window::cursor_position) else {
        return (None, 0);
    };
    let Some((camera, camera_transform)) = cameras.iter().next() else {
        return (None, 0);
    };

    // A click on the panel is not a click in the viewport. The camera fills the
    // whole window -- the framing it has is verified against 47 real documents
    // and re-deriving it for a narrower viewport is a separate piece of work --
    // so the panel genuinely overlaps the 3D view, and without this guard every
    // button press would also select whatever part happens to be behind it.
    if let Some((node, node_transform)) = viewports.iter().next() {
        let centre = node_transform.translation();
        let half = node.size * 0.5;
        let local = cursor - Vec2::new(centre.x, centre.y);
        if local.x.abs() > half.x || local.y.abs() > half.y {
            *rejected += 1;
            return (None, 0);
        }
    }

    // A window-space cursor is only viewport space when the camera's viewport IS
    // the window, which is the case here because `ScenePlugin` spawns the one
    // camera with no explicit viewport. That is an assumption about the scene, so
    // it is checked rather than trusted: a camera with a viewport that does not
    // cover the whole target is refused instead of producing a ray from the
    // wrong origin, which would pick a part somewhere off-screen.
    let target = camera
        .physical_target_size()
        .map(|size| Vec2::new(size.x as f32, size.y as f32))
        .unwrap_or(Vec2::ZERO);
    let viewport_position = match camera.viewport.clone() {
        Some(viewport) => {
            if Vec2::new(
                viewport.physical_position.x as f32,
                viewport.physical_position.y as f32,
            ) != Vec2::ZERO
                || Vec2::new(
                    viewport.physical_size.x as f32,
                    viewport.physical_size.y as f32,
                ) != target
            {
                warn!("camera viewport does not cover its target; refusing to guess the cursor mapping");
                return (None, 0);
            }
            cursor
        }
        None => cursor,
    };
    let Ok(ray) = camera.viewport_to_world(camera_transform, viewport_position) else {
        return (None, 0);
    };
    let hits = picking::pick_all(PickRay::from(ray), projection);
    (hits.first().map(|hit| hit.block), hits.len() as u32)
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
    (&'static ComputedNode, &'static GlobalTransform),
    (With<CreatorUiPart>, With<Viewport>),
>;

/// Marker so the viewport's UI node can be found by kind.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Viewport;

/// Mirrors the session into the UI's view model.
///
/// The one direction the data flows. The UI never writes the view: a button
/// changes the session, and the session is reflected here. That is what keeps
/// "the picture" and "the document" from becoming two answers.
fn refresh_view_model(
    mut state: ResMut<CreatorEditorState>,
    mut runtime: ResMut<CreatorRuntimeState>,
) {
    let resolved = state.session.projection().resolved().count();
    let blocks = state.session.stage().block_count();
    state.view.cells = resolved;
    state.view.active_tab = state.view.active_tab.min(blocks.max(1).saturating_sub(1));
    state.view.pages = NonZeroUsize::new(resolved.max(1)).unwrap_or(NonZeroUsize::MIN);

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
