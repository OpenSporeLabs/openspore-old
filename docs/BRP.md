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
| `crates/spore-engine/src/brp.rs` | **new** — the isolated bridge module |
| `crates/spore-engine/src/lib.rs` | `pub mod brp` (cfg-gated) + a 3-line `brp::add(&mut app)` in `base_app` |
| `tools/brp/mcp_call.py` | **new** — minimal stdio MCP client used by the smoke test |
| `tools/brp/smoke.sh` | **new** — the reproducible smoke test |
| `opencode.json` | registers the `brp` MCP server |
| `docs/BRP.md` | this file |
| `docs/BRP-SMOKE.md` | **new** — the verified transcript |

That is the whole footprint. In particular:

* **No domain crate gained a dependency.** `spore-core` … `spore-morphology`
  are untouched; the load-bearing "everything above `loader` is Bevy-free"
  invariant is intact.
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

`--placeholder` and `--record …` are separate launch paths. Adding the plugin
in the shared assembly point means every windowed mode is reachable identically
— an agent does not have to know which mode it launched. `--info` still never
gets there: that path returns before an `App` is constructed, so the headless
CI gate cannot accidentally open a listener.

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
* **The useful follow-up is reflection, not more tooling.** Registering
  OpenSpore's real runtime state (`LoadedScene`, `WorldResources`,
  `NormalMapPolicy`, `CameraFraming`) and giving spawned entities `Name`s is
  what turns this from "the bridge connects" into "the bridge can see and drive
  OpenSpore". It needs `Reflect` derives on domain types, which is a design
  decision about the Bevy-free domain layer and so was left alone here. The
  measurement behind that recommendation is in `docs/BRP-SMOKE.md`.
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