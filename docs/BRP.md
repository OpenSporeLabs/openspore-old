# BRP — driving a running `openspore` from an autonomous agent

`openspore` is a Bevy application, so it can be inspected, mutated,
screenshotted, driven and shut down from outside the process over the
**Bevy Remote Protocol** (BRP). This is the integration that makes a coding
agent able to do that, and this file records exactly what was built and
verified.

It is **development-only**. It is off by default, it is in exactly one crate,
and nothing in the decode path knows it exists.

---

## Versions

| Thing | Version | Where it comes from |
|---|---|---|
| `bevy` (workspace) | **0.19.1** | `Cargo.lock` |
| `bevy_remote` | **0.19.1** | pulled in by `bevy_brp_extras` |
| `bevy_brp_extras` | **0.22.8** | crates.io release |
| `bevy_brp_mcp` | **0.22.8** | crates.io release |

Compatibility was **not guessed**. `bevy_brp_extras` 0.22.8's own
compatibility table (`extras/README.md` upstream) maps Bevy `0.19` →
`bevy_brp_extras 0.22.8`, and `mcp/README.md` maps `0.19` →
`bevy_brp_mcp 0.22.8`. The upstream `main` branch is already `0.23.0-dev`
against the same `bevy = "0.19.1"`, but it is unpublished, so a released
0.22.8 on both sides is the pin that can actually be installed today.

`cargo tree -p spore-engine --features brp -e normal` confirms the union:
`bevy v0.19.1`, `bevy_remote v0.19.1`, `bevy_brp_extras v0.22.8` — one Bevy,
one version, no duplicates.

> **Upgrade rule:** `bevy` and `bevy_brp_extras`/`bevy_brp_mcp` must be bumped
> together. Bumping Bevy alone will either fail to resolve or — worse — pull a
> second `bevy_ecs` into the graph and break the "one Bevy" property above.

---

## What changed

| File | Change |
|---|---|
| `crates/spore-engine/Cargo.toml` | new `brp` feature; `bevy_brp_extras` as an **optional** dep |
| `crates/spore-engine/src/brp.rs` | **new** — the isolated bridge module; later extended with `register_openspore_types` |
| `crates/spore-engine/src/creator.rs` | `Reflect` derives on the creator runtime state and block marker, so they are visible to the bridge |
| `crates/spore-engine/src/lib.rs` | `pub mod brp` (cfg-gated) + a 3-line `brp::add(&mut app)` in `base_app` |
| `tools/brp/mcp_call.py` | **new** — minimal stdio MCP client used by the smoke test |
| `tools/brp/smoke.sh` | **new** — the reproducible smoke test |
| `opencode.json` | registers the `brp` MCP server |
| `docs/BRP.md` | this file |
| `docs/BRP-SMOKE.md` | **new** — the verified transcript |

That is the whole footprint. In particular:

* **No domain crate gained a dependency.** `spore-core` … `spore-morphology`
  are untouched; the load-bearing "everything above `loader` is Bevy-free"
  invariant is intact. The creator types that carry `Reflect` live in
  `spore-engine`, which is the Bevy crate by definition — `Reflect` is a
  `bevy_reflect` derive, and putting it on a domain type would make the
  Bevy-free layer depend on Bevy.
* **The default build is byte-identical in dependency terms.**
  `cargo tree -p spore-engine -e normal | grep -c brp` → `0`.

### Why a cargo feature rather than an env var

An env-var gate would have to keep `bevy_brp_extras` in the dependency graph of
every build, which means every build pays for `bevy_remote` (plus the
UI/text/sprite family Bevy 0.19 `bevy_remote` already pulls in) and — worse —
the hermetic `cargo test --workspace` would be compiling a network listener it
never uses. A feature removes it from the graph entirely, so
"development-only" is a property of the dependency graph and not a runtime `if`.

### Why `base_app` and not per-mode

`--placeholder`, `--record …` and `--creator …` are separate launch paths. Adding
the plugin in the shared assembly point means every windowed mode is reachable
identically — an agent does not have to know which mode it launched. `--info`
still never gets there: that path returns before an `App` is constructed, so the
headless CI gate cannot accidentally open a listener.

### Port selection

Left entirely to upstream: `BRP_EXTRAS_PORT` > `BrpExtrasPlugin::with_port()`
> default **15702**. The Rust side calls the no-argument constructor, so the
default lives in one place.

---

## Running it

```bash
# 1. the MCP server (once)
cargo install bevy_brp_mcp --version 0.22.8 --locked --root ~/.local

# 2. launch the app with the bridge compiled in
cargo run -p spore-engine --features brp -- --placeholder

# …or the real vertical slice
cargo run -p spore-engine --features brp -- \
    --preset documented-asset --package SPORE/Data/Spore_Content.package

# …or a whole creator document, which is the mode that has runtime state to read
cargo run -p spore-engine --features brp -- \
    --creator 0x2b978c46:0x40626200:0x05e3df13 \
    --package SPORE/Data/Spore_Game.package \
    --package SPORE/Data/Spore_Graphics.package
```

Optional: `BRP_EXTRAS_PORT=9000` moves the listener.

Then, from the agent: `brp_list_bevy` → `brp_launch` → `world_query` →
`brp_extras_screenshot` → `brp_extras_shutdown`.

---

## What the bridge gives an agent

From `bevy_brp_extras`:

| method | purpose |
|---|---|
| `brp_extras/screenshot` | PNG of the window / a camera viewport / an entity crop |
| `brp_extras/shutdown` | graceful exit, no signal needed |
| `brp_extras/get_diagnostics` | FPS and frame time |
| `brp_extras/set_window_title` | label the window |
| `brp_extras/send_keys`, `type_text` | keyboard input |
| `brp_extras/click_mouse`, `double_click_mouse`, `send_mouse_button`, `move_mouse`, `drag_mouse`, `scroll_mouse` | pointer input |

Plus Bevy's own BRP methods, which `BrpExtrasPlugin` makes available by adding
`RemotePlugin`: `world.*` (query / get / insert / mutate / remove on entities,
components and resources), `rpc.discover`, and the hierarchy and event tools
`bevy_brp_mcp` layers on top.

`bevy_brp_mcp` adds the agent-facing wrappers — `brp_launch`, `brp_list_bevy`,
`world_find_entities_by_name`, `world_get_components_watch`, `read_log`,
`brp_type_guide` — plus `brp_list_agent_tools` for app-published methods.

---

## What an agent can read

### The types that are registered, and the ones that cannot be

`brp.rs` is the only module in the workspace that talks to Bevy's
`AppTypeRegistry`, and it registers **OpenSpore-owned** types only:

| type | what it answers |
|---|---|
| `creator::CreatorRuntimeState` (resource) | the whole creator scene: document identity and family, block / resolved / unresolved / **visible** counts, selection, vertex / triangle / bone totals, `max_depth`, package count, `scene_stable`, `frame`, and `blocks[]` — one row per authored block |
| `creator::CreatorBlockState` | a per-block row: parent index, depth, `<blockid>`, the `rw4` / `gmsh` keys, counts, weakest evidence grade, and `stop_stage` / `stop_reason` for a block that did not resolve |
| `creator::CreatorBlockMarker` (component) | the entity side: which block this entity is, its parent and depth, and whether it is the selection |
| `creator::CreatorRootMarker` (component) | the one entity carrying the creature's world transform |
| `brp::BrpProbe` (resource) | the live counter this integration was built around |

`blocks[]` is the reason `CreatorRuntimeState` is worth reading at all. A block
that failed to resolve **spawns nothing** — there is no empty mesh standing in for
it — so an agent looking at entities alone would see a smaller creature and no
explanation. The row is the explanation, and it is there whether or not the block
reached the screen.

### The reflection trap, restated because it is still the trap

**In Bevy 0.19 only asset types register themselves.** `bevy_transform`,
`bevy_camera` and `bevy_mesh` contain **zero** `register_type` calls, so
`Transform`, `GlobalTransform`, `Visibility` and `Name` are invisible to BRP. A
`world.query` for one of them fails with

```
Component ... isn't registered or used in the world
```

and under `strict: false` it fails **silently as zero rows**, which reads exactly
like "the bridge is broken". It is not. On a live `openspore`,
`registry.schema` returns 112 types and every one is an asset or an asset handle.

So this integration does **not** try to register Bevy's types, and there is no way
to: it would mean upstream crates' decisions becoming this crate's dependency
graph. **A connected bridge and an inspectable app are different claims**, and the
second one is satisfied by types OpenSpore owns. The consequence is that a block's
*position* is not readable per entity over BRP — read `world_position` from
`CreatorRuntimeState.blocks` instead, which is the same value and is stated.

### Names

The creator path sets `Name`s: `creator.root` and `creator.block.<index>`. This is
only about **addressability** — `world_find_entities_by_name` returned 0 against
241 live entities before this, because OpenSpore set none. A `Name` is not
reflected, so it cannot be *read* as component data; use
`CreatorBlockMarker.block` for that.

### Readiness: poll, do not sleep

`CreatorRuntimeState.scene_stable` is `false` until the spawn system has run, and
`frame` counts `Update`s. An agent should poll both rather than
`sleep 2 and hope`:

```
1. brp_launch --features brp -- --creator 0x2b978c46:0x40626200:0x05e3df13 ...
2. poll world.get on CreatorRuntimeState until scene_stable == true and frame > 0
3. read the counts; a resolved_count below block_count is a partial creature, and
   blocks[] names every block that is missing
4. brp_extras/screenshot
5. mutate CreatorRuntimeState.selected_block to move the selection
6. brp_extras/shutdown
```

Step 5 is a real capability rather than a demo: the resource is `Reflect`, so
BRP can write `selected_block` and the next frame's spawn colours the block. (The
spawn reads the selection once, at `Startup`; re-tinting after a mutation is not
wired up yet — see *Limitations*.)

### Non-BRP modes

`LoadedScene`, `WorldResources`, `NormalMapPolicy` and `CameraFraming` are **not**
registered. They belong to the record-staging path, they hold Bevy asset handles
and `Arc`s, and reflecting them would mean `Reflect` derives on the Bevy-free
domain layer for the benefit of a debug bridge. The creator state was reflected
because it is plain data with no handles, and because a partial creature has to be
reportable.

---

## Known side effect: `Cargo.lock` moves two versions DOWN

Adding `bevy_brp_extras` is **not** free at the lockfile level, and the effect
reaches the default build:

| package | before | after | why |
|---|---|---|---|
| `image` | 0.25.10 | **0.25.9** | `bevy_brp_extras 0.22.8` pins `image = "=0.25.9"` |
| `moxcms` | 0.8.1 | **0.7.11** | `image 0.25.9` requires `moxcms ^0.7.4`; `0.25.10` wanted `^0.8.0` |

`Cargo.lock` is a single union for the whole workspace, so it is
**feature-agnostic**: an *optional* dependency's exact pin constrains every
build, including ones that never compile `bevy_brp_extras`. `image 0.25.9` is
consumed by `bevy_image`, which is part of Bevy, so this is not confined to the
dev path.

**It is internally consistent and safe.** `bevy_image 0.19.1` requires
`image = "0.25.2"`, which `0.25.9` satisfies; and the `image 0.25.9` +
`moxcms 0.7.11` pair is exactly the combination the `image` crate was built and
tested against. Verified after the change: `cargo test --workspace` →
**1622 passed, 0 failed**, and the app renders at 60 FPS with a non-uniform
screenshot.

**It is still reported rather than buried**, because a dev-only dependency
quietly moving the renderer's image stack is exactly the kind of thing that
should never be silent. If the lockfile churn is unwanted, the fix is to move
the BRP integration into its **own workspace** so its lock cannot touch this
one. That is more structure than this task justified, so it is recorded as
follow-up work instead.

Lockfile totals: **409 → 485 packages**, 76 added, none removed, `bevy` still
`0.19.1`.

---

## Verified

Recorded honestly; see `docs/BRP-SMOKE.md` for the transcript.

* workspace builds clean **with and without** `--features brp`
* `bevy_brp_mcp` 0.22.8 installs and starts as a stdio MCP server
* the MCP server enumerates the running app on port 15702
* a real entity/resource was queried
* screenshot capture ran
* `brp_extras/shutdown` ended the process
* no `openspore` process remained afterwards

---

## Limitations / follow-up

* **Screenshots are the primary window's composited target.** A minimized,
  hidden or fully occluded surface can legitimately produce a black PNG.
  `brp_extras/screenshot` succeeding is not evidence that scene content is
  non-uniform — check the file if that matters.
* **Entity-crop screenshots need `Aabb` + `GlobalTransform`** (UI nodes use
  computed UI bounds). Entities OpenSpore spawns without a mesh-derived Aabb
  cannot be cropped individually; capture the window instead.
* **`get_diagnostics` needs the `diagnostics` feature**, on by default in
  `bevy_brp_extras`. Turning extras' default features off removes
  `FrameTimeDiagnosticsPlugin`.
* **Textual UI-tree / snapshot inspection is not provided** — a separate BRP
  capability, not part of this integration.
* **Bevy 0.19 reports method *names* through `rpc.discover` but not
  descriptions or parameter/result schemas.** Use `brp_type_guide` or
  `brp_list_agent_tools` for shapes.
* **No BRP surface for the decode layer.** Everything above `loader`
  (`spore-dbpf`, `spore-gmdl`, `spore-assets`, …) runs *before* Bevy exists and
  is entirely outside BRP's reach. `--info` reports it; the MCP bridge cannot
  query it. Anything an agent needs to know about decoding is `osptool`'s job.
* **Not covered:** an automated in-repo test that asserts the bridge is present
  when `brp` is on. `base_app` needs a window, and per the trap already
  documented in `AGENTS.md`, "the app assembles" is not a headless-testable
  claim. The smoke test is the check.
* **Reflection now covers the creator path, and only the creator path.**
  `CreatorRuntimeState`, `CreatorBlockState`, `CreatorBlockMarker` and
  `CreatorRootMarker` are registered, and creator entities carry `Name`s — see
  *What an agent can read*. The record-staging types (`LoadedScene`,
  `WorldResources`, `NormalMapPolicy`, `CameraFraming`) are still not registered,
  deliberately: they hold asset handles and `Arc`s, and reflecting them would put
  `Reflect` on the Bevy-free domain layer for the benefit of a debug bridge.
* **Moving the selection does not re-tint an already-spawned creature.** The
  spawn system reads `selected` once, in `Startup`. A BRP write to
  `CreatorRuntimeState.selected_block` updates the reported selection but not the
  already-created material. Making the tint follow the selection is a small change
  (a system that rewrites `MeshMaterial3d` for the marked entity); it is not done.
* **Per-entity world transforms are still not readable.** `Transform` and
  `GlobalTransform` are unregistered in Bevy 0.19 and cannot be registered from
  here, so an agent reads positions from `CreatorRuntimeState.blocks` rather than
  from each entity's `GlobalTransform`.
* **Moving the integration to a separate workspace** would stop
  `bevy_brp_extras`'s `=0.25.9` pin from moving `image`/`moxcms` in this
  lockfile. See the section above.
* **Mouse input was never exercised.** All the mouse methods are registered and
  listed by `rpc.discover`, but the smoke test sent no click, drag or scroll.
  OpenSpore has no pointer handling today, so there is nothing to assert against
  yet.
* **Key names are Rust variant spellings** — `send_keys` wants `"Escape"`, and
  `"ESCAPE"` fails with `Invalid key code 'ESCAPE': Unknown key code`. Same
  likely applies to mouse button names. `brp_extras/send_keys` and
  `brp_extras/shutdown` have **no** MCP tool wrapper; reach them with
  `brp_execute`.