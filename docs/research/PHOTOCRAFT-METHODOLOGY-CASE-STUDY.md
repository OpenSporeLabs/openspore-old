# PhotoCraft methodology case study

Owner: orchestrator · Status: research · Date: 2026-10-16

What OpenSpore should learn from `github.com/storytold/photocraft`, a Rust
reconstruction of Adobe Photoshop. **Methodology only** — PhotoCraft is not a
dependency, and none of its code is ported
(`../architecture/SPECIFICATION-DRIVEN-RECONSTRUCTION.md` §6).

## Provenance of this study

- Repo: `https://github.com/storytold/photocraft`, branch `main`, latest commit
  at capture **`2a952f164c0b4cf936b1c6497db10dd095341c00`** (2026-10-16T05:48:30Z).
- Source: `raw.githubusercontent.com`, the HTML tree endpoint, and
  `commits/main.atom`. The GitHub REST API returned **HTTP 403** (rate-limited),
  so star/fork/issue counts were not verifiable.
- Licence: MIT + Apache-2.0, plus `NOTICE`/`ATTRIBUTION.md`. Content read
  read-only; treated as untrusted technical input.

## 1. What it actually is (stated maturity)

README calls itself a "clean-room reimplementation of Adobe Photoshop … in pure
Rust", badge *early alpha*. It does **not** claim full parity: `docs/target-app-parity.md`
puts "ready for real work" at **~45%** (est. range 40–50%), and `docs/gaps.md`
records its own earlier estimate was "far too optimistic". Scale (from
`docs/architecture.md`): 25 library crates, 4 apps, ~539k LoC, ~8,040 `#[test]`
functions. So the case study is *evidence-disciplined reconstruction*, not a
finished clone — which is exactly why it is useful.

## 2. Public specification vs reference behavior

- **Spec-first where a spec exists:** the Adobe **PSD/PSB** file-format spec, the
  Adobe `.shc` contour format, the PSD descriptor structures, plus open standards
  (OpenEXR, TIFF/BigTIFF, PNG, HEIF/ISO-BMFF, ICC, SVG, PDF). A published format
  spec is enough to write a *decoder*.
- **A format spec is not an application spec.** Rendering semantics, edit
  behavior, UI and app state transitions are **not** in the file-format doc; for
  those it uses an executable oracle (the CPU compositor) and reference output
  (Photoshop-authored files).
- **It measures the target app without running it:** `target-app-parity.md`
  derives the Photoshop 2026 feature denominator from `Info.plist`, format
  plug-in PiPL flags, `.lproj` language packs, `Default Keyboard Shortcuts.kys`
  (67 tools / 107 menu commands), `Required/` plug-ins (85 filters / 12 format
  plug-ins), and 81 "… Tool" binary strings. Denominator gaps are named (its
  `Default Menus.mnu` was not parsed).

## 3. Specification → contract → implementation → oracle → parity (three traces)

| Type | Evidence | Implementation | Oracle | Tolerance | Honest gap |
|---|---|---|---|---|---|
| **Menu command / adjustment** | Photoshop 32-bit rules encoded verbatim in test comments (`crates/gpu/tests/parity.rs`) | `crates/engine/src/adjust_cmds.rs`; `CommandSpec` registry in `commands.rs` | **CPU compositor** `photocraft_compose::flatten` — GPU must match it | `1.0/255.0 + 4·f32::EPSILON` (premultiplied) | GPU tests **skip** when no `Rgba32Float` adapter |
| **File format (Smart Objects)** | PSD descriptor spec + observed behavior; scorecard FILE-215-1 | `crates/psd` (`PlLd`+`SoLd`, `lnk2`, `smart_map.rs`) | the file's **own merged composite**, and a stricter re-render | `PASS_TOL = 2.0/255.0`, floor `PASS_FLOOR = 5` | several rows `partial`/`missing`; user reports Photoshop refuses some exports |
| **Compositing (blend/mask/clip)** | Photoshop blend math; CPU compositor is the executable spec | `crates/compose` (CPU) vs `crates/gpu` (`compose.wgsl`) | CPU reference | 2/255 (file header), `1/255 + 4ε` in the float path | fallback paths are *tested*, not assumed (`…_or_fall_back`) |

Pattern: pick the strongest available oracle → implement in the lowest layer →
compare at an **explicit, commented tolerance** → set a **monotone floor**.

## 4. Corpus / oracle discipline (the strongest idea)

- **Real corpora are never committed.** `xtask/src/corpus_pins.rs`: `cargo xtask
  corpus --all` fetches them into a gitignored `corpus/` at **pinned commits** and
  verifies each file against committed **`*.sha256` manifests**. CI's cache key
  hashes the pins, so changing a pin is the only thing that refetches.
- Pins include its own `photocraft-corpus` (Photoshop-authored), `psd-tools`
  (MIT), `ag-psd` (MIT), `heic-rs`, `pillow_heif`, OpenEXR, Affinity docs,
  PngSuite (public domain). Each carries a generated per-corpus README with
  source commit + license. **This is the mechanism that makes real-data testing
  legal**: pin, hash, keep out of git, attribute.
- Multiple oracle strategies by format: the authoring app's own merged composite;
  deliberate **re-render from source** (discard cached pixels to test its own
  engine); round-trip; embedded-thumbnail fallback; third-party parser test sets;
  synthetic unit fixtures; property/fuzz/malformed-input tests (20 fuzz targets).
- `test-corpus` **fails** when the corpus is missing — no silent skip.

## 5. Parity / scorecard management

Two **generated** documents, each CI-checked so they cannot drift from code:

- `docs/parity-checklist.md` ← `cargo xtask parity` (menu tree ⋈ live command
  registry). "631 of 631 menu items live", with a hard regression floor
  `FLOOR = 631` in `crates/ui-egui/src/parity.rs`.
- `docs/scorecard.md` ← `cargo xtask scorecard`, generated from
  `perf/budgets.toml`, `perf/baseline.json`, `crates/io/tests/corpus.rs`,
  `scorecard/*.toml` and the source tree; CI runs `--check` (static, no benchmarks).

Vocabulary is `done` / `partial` / `missing`, each with target + issue link +
note naming the exact symbols/tests. Every headline number carries a **date and a
kind (measured vs estimated)**, weights are documented, and it self-revises
loudly. `docs/gaps.md` is a ranked, agent-actionable queue ("take the highest
entry nobody is working on"), marking what blocks beta.

## 6. Architecture (enforced, not aspirational)

- 25 crates in 8 layers, ordered `L0 foundation → L7 apps`; **UI crates forbidden
  below L6**. The rule engine is `xtask/src/layers.rs` (unit-tested, built from
  `cargo metadata --no-deps`); **CI runs `cargo xtask layers`**, so an illegal
  dependency fails the build.
- **Nothing commits `unsafe`** except one documented macOS AppKit helper;
  `#![deny(clippy::unwrap_used, expect_used, panic, unimplemented, todo, unreachable)]`
  workspace-wide, `panic_hunt.rs`, and the scorecard reports "never-crash lints
  on 29 of 29 crates".
- Undo/redo = whole-document snapshots per transaction, justified because tiles
  are `Arc` copy-on-write; a byte budget caps history.
- Command registry is indexed and proven order-stable (`command_lookup.rs`);
  headless CLI + an MCP/automation surface reuse the same commands.

## 7. Demonstrated by code/tests vs intended/estimated

**Demonstrated:** command-lookup stability; CPU↔GPU parity (~1/255) across blend
modes/adjustments/groups/masks/clipping/fills/effects; menu parity 631/631 with a
hard floor; corpus round-trips ~100% with oracle floors (148/170 io, 249/309
psd-tools, 188/258 Photoshop-authored); enforced layering; never-crash lints;
20 fuzz targets; wasm build; Photoshop opens its Smart-Object exports as live
smart objects.

**Intended/estimated/partial:** the headline readiness percentages (subjective
weights); perf (22/58 over budget, 19 not run, 5 "not measurable yet"); AI
(~15%); plug-in/script ecosystem (absent); localization/hardware partial.

**Its one unautomated critical oracle:** whether Photoshop *accepts* the written
PSDs is verified by a human opening files on the owner's Mac (`docs/gaps.md` G1).
PhotoCraft names this gap rather than papering over it.

## 8. Transferable principles (each demonstrated above)

1. One **generated, CI-checked** parity/scorecard doc — reports that compile don't rot.
2. A **monotone regression floor**, "raise as features land, never lower".
3. An **independent executable oracle** + an **explicit, commented tolerance**.
4. **Never commit real data: pin + SHA-256 + fetch**, with per-corpus licences.
5. **Layering as an executable rule** run by CI, not a diagram.
6. A **renderer-agnostic core** with no GUI/vendor dependency, testable headlessly.
7. **Separate measured from estimated**, and revise your own optimism out loud.
8. A **ranked, agent-actionable gap list** with impact + estimate.
9. **Never-crash as a testable invariant** (`forbid(unsafe)` + `deny(unwrap)`).
10. **Multi-oracle corpora + honest skips** (skip *with a printed reason*).

## 9. What does **not** transfer to Spore

- **No published Spore format spec.** Photoshop's spec-first path is unavailable;
  Spore's formats are community reverse-engineered (see `SPORE-SPECIFICATION-MAP.md`).
- **The reference app is a proprietary EXE, not a re-runnable oracle.** We cannot
  shell out to "Spore authoring files" the way PhotoCraft opens a Photoshop PSD;
  the oracle is a GOG install observed under controlled runs.
- **A decades-old modding ecosystem is an oracle, not just a source.** SporeModder
  FX / Blender addons are *independent implementations we can differential
  against* — PhotoCraft had no comparable ecosystem to lean on.
- **A richer simulation.** Photoshop is stateless image ops; Spore has game
  simulation state machines, so "behavior parity" is a much larger surface.
- No headline "% complete" is transferable; Spore's coverage is a **matrix of
  separately-defined dimensions** (already the stance of `docs/tooling/reconstruction-coverage.md`).
