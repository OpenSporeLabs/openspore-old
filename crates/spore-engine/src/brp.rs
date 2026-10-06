//! The development-only Bevy Remote Protocol bridge.
//!
//! This module exists so the *only* place OpenSpore mentions `bevy_remote` is
//! behind the `brp` cargo feature. Everything above it -- `domain`, `loader`,
//! `convert`, `cli` -- is unaware that a debug bridge exists, and so is every
//! build that does not ask for it.
//!
//! # What it buys
//!
//! [`BrpExtrasPlugin`] registers the BRP transport (an HTTP listener, default
//! port 15702) plus the methods that turn a running window into something an
//! autonomous agent can drive:
//!
//! | method | why an agent needs it |
//! |---|---|
//! | `brp_extras/screenshot` | see what the renderer actually produced |
//! | `brp_extras/shutdown` | end the app without a signal |
//! | `brp_extras/get_diagnostics` | FPS / frame time, so "it launched" is not confused with "it is running" |
//! | `brp_extras/send_keys`, `type_text` | drive input |
//! | `brp_extras/*_mouse` | drive the pointer |
//!
//! Plus Bevy's own `world.*` methods (query / get / insert / mutate entities,
//! components and resources) once `bevy_remote`'s `RemotePlugin` is present --
//! which `BrpExtrasPlugin` adds for us.
//!
//! # Port selection
//!
//! Upstream's own precedence is
//! `BRP_EXTRAS_PORT` > `BrpExtrasPlugin::with_port()` > 15702. We add nothing on
//! top: this function calls the no-argument `BrpExtrasPlugin`, so an operator
//! picks the port by environment and the Rust side stays the single default.
//!
//! # Why the plugin is added in `base_app`, not per-mode
//!
//! `--placeholder` and `--record` are separate launch paths and a future agent
//! would otherwise have to know which one it got. Adding it to the shared
//! assembly point means *every* windowed mode is reachable the same way, and
//! `--info` still never reaches here: that path returns before an `App` is
//! built, so the headless CI gate cannot accidentally open a listener.

use bevy::prelude::*;

/// The port `BrpExtrasPlugin` listens on unless `BRP_EXTRAS_PORT` overrides it.
///
/// Named here so `docs/BRP.md` and an error message have one authority.
pub const DEFAULT_PORT: u16 = 15702;

/// A tiny reflectable resource that exists only to prove the bridge works.
///
/// # Why this is needed at all
///
/// BRP can only read or mutate a component or resource whose type is in Bevy's
/// `AppTypeRegistry`. In **Bevy 0.19 only asset types register themselves** --
/// `bevy_transform`, `bevy_camera` and `bevy_mesh` contain **zero**
/// `register_type` calls, so `Transform`, `GlobalTransform`, `Visibility` and
/// `Name` all fail with
/// `Component ... isn't registered or used in the world`. `registry.schema`
/// on a running `openspore` returns 112 types and every one of them is an
/// asset or an asset handle.
///
/// The consequence is that without *some* registered type, `world.query` with
/// component data returns zero rows and no mutation is expressible, even though
/// the bridge is connected perfectly well. That is a real gap, not a broken
/// install.
///
/// This resource closes it for the dev bridge, in the dev bridge's own module,
/// at zero cost to the domain: it registers a live counter an agent can read,
/// write and watch, which is the honest demonstration that the read/write path
/// end-to-end. It is `#[cfg(feature = "brp")]`-only and changes no product
/// behaviour.
///
/// [`register_openspore_types`] registers OpenSpore's *real* runtime state beside
/// it -- [`crate::creator::CreatorRuntimeState`] and friends. The reason that
/// registration lives here rather than in `creator` is the one that matters: this
/// is the only module in the workspace that talks to the type registry, so the
/// `brp` feature stays a single `#[cfg]` in one place and `creator` stays a plain
/// runtime module with no knowledge of a debug bridge.
#[derive(Resource, Reflect, Default, Debug)]
#[reflect(Resource)]
pub struct BrpProbe {
    /// Incremented every frame by [`tick_probe`].
    pub frames: u64,
    /// A plain string, so string mutation is exercised too.
    pub label: String,
}

/// Keeps [`BrpProbe::frames`] moving, which is what makes it worth watching.
fn tick_probe(mut probe: ResMut<BrpProbe>) {
    probe.frames = probe.frames.wrapping_add(1);
}

/// Registers OpenSpore's own runtime types so a query can return rows for them.
///
/// # Which types, and which deliberately not
///
/// Bevy 0.19 registers **only asset types**, so `world.query` with component or
/// resource data returns nothing for anything outside `bevy_asset`,
/// `bevy_material` and friends -- and under `strict: false` it fails *silently as
/// zero rows*, which reads exactly like "the bridge is broken". So the answer is
/// not to work around the registry but to give it OpenSpore-owned types that are
/// genuinely worth reading.
///
/// Registered:
///
/// | type | why an agent needs it |
/// |---|---|
/// | [`crate::creator::CreatorRuntimeState`] | the whole creator scene as one read: document identity, block/resolved/unresolved/visible counts, selection, triangle/vertex/bone totals, `scene_stable`, `frame`, and one row per block **including the ones with no entity** |
/// | [`crate::creator::CreatorBlockState`] | a per-block row: which records it resolved to, its counts, its provenance grade, and the stage and reason for a block that did not |
/// | [`crate::creator::CreatorBlockMarker`] | the entity side, so `world.query` can find the block entities and read which one is selected |
/// | [`crate::creator::CreatorRootMarker`] | the one entity carrying the creature's world transform |
/// | [`crate::loader::BrpProbe`] | a live counter an agent can read, write and watch, which is the honest demonstration that the read/write path works end to end |
///
/// **Not** registered, and not attempted: `Transform`, `GlobalTransform`,
/// `Visibility` and `Name`. Those live in `bevy_transform`, `bevy_camera` and
/// `bevy_mesh`, which contain **zero** `register_type` calls in 0.19.1, so they
/// are invisible to BRP no matter what this module does -- `registry.schema` on a
/// live `openspore` returns 112 types and every one is an asset or an asset
/// handle. Registering Bevy's types from here is not possible either: it would
/// mean upstream crates' decisions becoming this crate's dependency graph.
/// A connected bridge and an inspectable app are different claims; the second one
/// needs types OpenSpore owns.
///
/// Two further consequences, both recorded in `AGENTS.md`, and both unchanged
/// here: `world.find_entities_by_name` returns rows only because the creator path
/// now sets `Name`s (it set none before, which is why it returned 0 against 241
/// live entities), and component-level mutation and `+watch` remain reachable only
/// for the types listed above.
fn register_openspore_types(app: &mut App) {
    app.register_type::<crate::creator::CreatorRuntimeState>();
    app.register_type::<crate::creator::CreatorBlockState>();
    app.register_type::<crate::creator::CreatorBlockMarker>();
    // The editor's two reflected resources. Minimum dev-only reflection, and only
    // because without it BRP reports them as `Unknown resource type` while the app
    // is plainly running -- so "is the editor wired up" is unanswerable over the
    // one channel built to answer it. Registered here rather than unconditionally
    // so the hermetic test build gains no registry entries.
    app.register_type::<crate::editor::plugin::LastPick>();
    // The probe is a field of LastPick, and an unregistered type is invisible to
    // BRP's reflection: the field would come back omitted or empty, which is the
    // "bridge is broken" symptom for a value that is perfectly fine.
    app.register_type::<crate::editor::plugin::WheelProbe>();
    app.register_type::<crate::editor::plugin::CreatorSelectionCommand>();
    app.register_type::<crate::creator::CreatorRootMarker>();
}

/// Adds the BRP bridge to an app that has already had its window configured.
///
/// `BrpExtrasPlugin` is deliberately the plain constructor: it detects an
/// existing `RemotePlugin` / `RemoteHttpPlugin` and composes with it rather
/// than adding a second transport, so this stays correct if OpenSpore later
/// grows its own remote plugin.
pub fn add(app: &mut App) {
    use bevy_brp_extras::BrpExtrasPlugin;
    info!(
        "openspore: BRP dev bridge enabled (bevy_brp_extras), listening on port {} unless \
         BRP_EXTRAS_PORT overrides it",
        DEFAULT_PORT
    );
    app.add_plugins(BrpExtrasPlugin);
    app.init_resource::<BrpProbe>();
    app.register_type::<BrpProbe>();
    register_openspore_types(app);
    app.add_systems(Update, tick_probe);
}
