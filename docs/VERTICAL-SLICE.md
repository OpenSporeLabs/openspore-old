# The vertical slice — run it, and what is verified

The smallest end-to-end proof that the OpenSpore Rust/Bevy engine works: a
native application that opens a window and renders a 3D scene. Two forms — one
that needs the game install and one that does not.

---

## Run it

```bash
# 1. No game data at all: a built-in procedural creature.
cargo run -q -p spore-engine -- --placeholder

# 2. A real decoded Spore record (needs SPORE/ on disk).
cargo run -q -p spore-engine -- --preset documented-asset \
    --package SPORE/Data/Spore_Content.package

# 3. Headless equivalent of (2); opens no window and is the CI gate.
cargo run -q -p spore-engine -- --preset documented-asset \
    --package SPORE/Data/Spore_Content.package --info
```

Escape closes the window. Exit codes: `0` ok, `2` bad command line, `3` asset
unavailable.

---

## What the placeholder renders

A **modular creature assembly**, not a single mesh. Ten modules — a box trunk, a
head with a snout and two eyes, a tail and four legs — built by
`crate::domain::assembly::quadruped()` and turned into a Bevy hierarchy by
`crate::scene::spawn_placeholder_creature`.

The split is the point:

* the **structure and placement are Bevy-free** (`domain::assembly`), so they are
  asserted in `cargo test -p spore-engine` with no window and no GPU;
* the **renderer only consumes** them: it picks a mesh per primitive and a colour
  per module *kind*, and owns nothing about their meaning.

The assembly is **placeholder content**. It is not Spore geometry, it is not a
claim about what a creature is, and no number in it is derived from the original
game. It exists so a later failure is attributable to the asset path rather than
to the renderer.

---

## What is verified

| claim | how it was checked |
|---|---|
| the workspace builds and tests hermetically | `cargo test --workspace` (no game, no GPU) |
| the domain assembly places and measures correctly | 10 unit tests in `domain/assembly.rs` |
| the renderer spawns one mesh per module under one staged root | `scene::tests::the_placeholder_is_a_modular_assembly_under_one_staged_root` |
| a real record decodes and reports | `--info` (see `docs/RUST-ENGINE.md` §3) |
| **a window opens and the scene renders** | see "Runtime status" below — this needs a real display and is recorded per run |

Compilation is **not** proof the renderer works. Only a launch with a display is.

### Runtime status

Recorded honestly, per run (2026-10-16, CachyOS, AMD RX 7900 XTX / RADV,
Mesa, Vulkan):

* [x] ran `--placeholder` on a real display: a window opened and the modular
  creature rendered — trunk, tilted head with snout and two eyes, tail and four
  legs standing on the ground plane with a cast shadow. Captured from the window
  with `maim` (window 1533×865).
* [x] ran the headless `--info` gate against `SPORE/Data/Spore_Content.package`:
  decodes one gmdl record and exits **0**.
* [ ] ran a real-asset *window* on a display in this session: not done; only the
  `--info` path was exercised. The placeholder path is what proves the renderer.

Launch notes: the compositor is Wayland, so an X11 screenshot tool cannot see the
default (Wayland) window; the run above forced `WINIT_UNIX_BACKEND=x11` under
XWayland purely to capture it. The renderer itself is unchanged by that choice.

(If a display is unavailable, say so and leave the row unchecked rather than
reporting PASS. An untested render is not a tested render.)

---

## Important decisions

* **No new crates.** The repository already has a Rust + Bevy engine with the
  right boundaries (`spore-core` … `spore-morphology` are Bevy-free; only
  `spore-engine` touches Bevy). A parallel `openspore-core`/`openspore-app` would
  duplicate working, tested code. The procedural assembly was added to the
  existing `domain` layer instead, honouring its "nothing below imports `bevy`"
  rule.
* **Placeholder geometry is code-generated**, so no proprietary asset enters the
  tree — consistent with the clean-room rules in `AGENTS.md`.
* **Bevy stays at 0.19.1** for this milestone; see `docs/PLATFORMS.md` for the
  rationale and the plan if an upgrade is wanted.

## Unresolved questions / next step

1. **Camera framing for the placeholder.** The fixed placeholder camera was tuned
   for one sphere; the assembly is sized in the domain and scaled for
   presentation (`PLACEHOLDER_SCALE`). The clean fix is to frame the camera from
   the assembly's own bounds, as the asset path already does — the next small
   improvement.
2. **Real behaviour specs.** Per `docs/BEHAVIOR-EVIDENCE.md`, the next evidence
   work is to turn verified `spore-recomp` observations into graded OpenSpore
   specifications — not to port recomp code.
3. **Content packs.** The first concrete step is a pure manifest + dependency
   validator; see `docs/CONTENT-MODEL.md` §5.
