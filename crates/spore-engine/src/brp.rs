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
/// Registering OpenSpore's *real* runtime state (`LoadedScene`,
/// `WorldResources`, `NormalMapPolicy`, `CameraFraming`) would be the useful
/// follow-up, and it is deliberately not done here: `Reflect` is a
/// `bevy_reflect` derive, and putting it on domain types is a design decision
/// about the Bevy-free domain layer, not an infrastructure one.
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
    app.add_systems(Update, tick_probe);
}
