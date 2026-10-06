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
            .add_systems(
                PostStartup,
                (remove_debug_ground, frame_editor_camera, ui::font::apply),
            )
            .add_systems(
                Update,
                (
                    spawn_or_respawn_creature,
                    frame_editor_camera_after_an_edit,
                    apply_selection_command,
                    read_viewport_click,
                    wheel_scales_selection,
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
    if !cursor_over_viewport(cursor, viewports) {
        *rejected += 1;
        return (None, 0);
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

/// Whether a window-space cursor sits inside the viewport's UI rect.
///
/// # One predicate, because two callers need one answer
///
/// The click path needs it to refuse a press on the panel, and the wheel needs it
/// for the same reason with more force: the wheel is also how a thumb grid pages,
/// so a cursor over the panel that scaled the selected part would make scrolling
/// the catalog silently resize the creature. Written twice it would be two answers
/// to one question, and they would drift.
///
/// # No viewport node means no opinion
///
/// The camera covers the whole window, so when the UI has not laid out yet there is
/// nothing to contradict. That is an absence of evidence rather than a hit, so it
/// answers `true` and leaves the caller's own guard (a real click, a real scroll)
/// to decide whether the input counts. Returning `false` here would instead make
/// input silently dead during the first frames, which reads exactly like a broken
/// picker and is not one.
/// # The shared hazard: PHYSICAL node units against a LOGICAL cursor
///
/// [`bevy_ui::ComputedNode::size`] and [`bevy_ui::ui_node::UiGlobalTransform`] are
/// in **physical** pixels while [`bevy::window::Window::cursor_position`] is in
/// **logical** ones. On a display where those differ the rect test rejects every
/// cursor, including ones plainly inside the viewport. This is pre-existing in the
/// click path and measured there; it is NOT fixed here, because the only machine
/// available has a scale factor of 1.0 where the two are equal, so a "fix" could
/// not be shown to fix anything. Widening it to two callers makes the hazard more
/// load bearing, so it is recorded on both counters instead:
/// [`LastPick::rejected_outside_viewport`] and
/// [`LastPick::wheel_rejected_outside_viewport`]. A non-zero rejection count with
/// clicks that plainly landed inside is this bug and nothing else.
fn cursor_over_viewport(cursor: Vec2, viewports: &ViewportNodes) -> bool {
    let Some((node, node_transform)) = viewports.iter().next() else {
        return true;
    };
    let translation = node_transform.translation();
    cursor_in_rect(
        cursor,
        Vec2::new(translation.x, translation.y),
        node.size * 0.5,
    )
}

/// Whether a cursor lies inside an axis-aligned rect given its centre and half-size.
///
/// # Why the edges are inclusive, and why the test pins them
///
/// A UI node's rect *is* its half-size box, so a cursor exactly on the boundary is
/// over the viewport. Both `<` and `<=` here pass every "obviously inside" and every
/// "obviously outside" case, which means a flipped comparison survives a casual
/// check and only shows up as a one-pixel dead edge. So the tests below assert the
/// boundary explicitly in both directions and assert the two axes independently: a
/// rect test that checked only `x` would pass every point off to the side.
fn cursor_in_rect(cursor: Vec2, centre: Vec2, half: Vec2) -> bool {
    let local = cursor - centre;
    local.x.abs() <= half.x && local.y.abs() <= half.y
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
    viewports: ViewportNodes,
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
    // guard instead of re-deriving the viewport rect.
    let Some(cursor) = windows.iter().find_map(Window::cursor_position) else {
        return;
    };
    if !cursor_over_viewport(cursor, &viewports) {
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

#[cfg(test)]
mod tests {
    use super::cursor_in_rect;

    /// The rect both callers share: a viewport node centred at the window centre.
    fn rect() -> (bevy::math::Vec2, bevy::math::Vec2) {
        (
            bevy::math::Vec2::new(640.0, 360.0),
            bevy::math::Vec2::new(384.0, 360.0),
        )
    }

    #[test]
    fn cursor_at_the_centre_is_inside() {
        let (centre, half) = rect();
        assert!(cursor_in_rect(centre, centre, half));
    }

    #[test]
    fn every_edge_is_inclusive_in_both_directions() {
        let (centre, half) = rect();
        for point in [
            centre + bevy::math::Vec2::new(half.x, 0.0),
            centre - bevy::math::Vec2::new(half.x, 0.0),
            centre + bevy::math::Vec2::new(0.0, half.y),
            centre - bevy::math::Vec2::new(0.0, half.y),
            centre + half,
            centre - half,
        ] {
            assert!(
                cursor_in_rect(point, centre, half),
                "edge {point:?} must count as inside"
            );
        }
    }

    #[test]
    fn one_pixel_past_an_edge_is_outside() {
        let (centre, half) = rect();
        for point in [
            centre + bevy::math::Vec2::new(half.x + 1.0, 0.0),
            centre - bevy::math::Vec2::new(half.x + 1.0, 0.0),
            centre + bevy::math::Vec2::new(0.0, half.y + 1.0),
            centre - bevy::math::Vec2::new(0.0, half.y + 1.0),
        ] {
            assert!(
                !cursor_in_rect(point, centre, half),
                "one pixel past {point:?} must be outside"
            );
        }
    }

    #[test]
    fn the_two_axes_are_tested_independently() {
        // A rect test that only checked one axis would pass every one of these.
        let (centre, half) = rect();
        let well_inside_x = centre + bevy::math::Vec2::new(0.0, half.y + 50.0);
        assert!(!cursor_in_rect(well_inside_x, centre, half));
        let well_inside_y = centre + bevy::math::Vec2::new(half.x + 50.0, 0.0);
        assert!(!cursor_in_rect(well_inside_y, centre, half));
    }

    #[test]
    fn a_degenerate_rect_accepts_only_its_own_point() {
        // Guards the opposite defect: a half-size of zero must not turn into an
        // "everything is inside" answer through a division or a sign slip.
        let centre = bevy::math::Vec2::new(10.0, 10.0);
        let zero = bevy::math::Vec2::ZERO;
        assert!(cursor_in_rect(centre, centre, zero));
        assert!(!cursor_in_rect(
            centre + bevy::math::Vec2::new(0.5, 0.0),
            centre,
            zero
        ));
    }
}
