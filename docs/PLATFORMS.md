# Platforms — the support path and what is actually verified

**Nothing here is a support claim for a platform that has not been built and run.**
A target is supported only when its build *and* runtime path have been exercised.

---

## Verified today

| target | status | what was actually done |
|---|---|---|
| Linux (x86-64) | **verified** | native `cargo run -p spore-engine`; a window opens and the scene renders. `wayland` and `x11` are both enabled in the Bevy feature set (`Cargo.toml`). |
| Linux, headless | **verified** | `--info` decodes and exits with no window; this is the CI gate. |

Anything not listed is unverified.

---

## The intended path for Windows and macOS

The renderer is Bevy/wgpu, which targets Vulkan, Metal and DX12. The work to
reach Windows and macOS is therefore *not* a renderer port; it is:

1. **Build the workspace on the target** (`cargo build --workspace`). wgpu picks
   the backend; no OpenSpore code is backend-specific.
2. **Check the two Bevy feature choices.** This workspace enables `x11` and
   `wayland` explicitly. A Windows or macOS build needs the corresponding
   winit backend, which the platform's default feature set supplies — verify
   rather than assume, since this build sets `default-features = false`.
3. **Run the vertical slice** (`openspore --placeholder`, then a real asset) and
   record what was seen.

No OpenSpore source is expected to change. If it does, that is a finding worth
writing down.

## Web (wasm) and mobile — assessed, not attempted

* **Web (wasm):** Bevy renders to WebGL2/WebGPU in a browser, but this workspace
  currently depends on filesystem packages (`Spore_Content.package`, ~1 GB) and
  memory-maps them (the one `unsafe` in `spore-assets`). Neither maps cleanly to
  a browser. Treat wasm as a **future research target**, not a build flag, and do
  not claim it until a browser run is recorded.
* **Mobile (iOS/Android):** Bevy targets both. The blockers are the same
  filesystem/memory-mapping assumptions plus touch input and asset delivery.
  Again: a research target, unevaluated here.

The honest position for both is the repository's standing one: unverified until
built and run, and the write-up says which.

---

## Portability rules in force

* **No OS-specific API without a boundary.** There is currently no
  platform-specific code in `spore-engine`; if one is ever added, it goes behind
  a narrow interface (the same discipline as `docs/BOUNDARIES.md`).
* **No hard-coded filesystem assumptions.** Package paths come from the command
  line today. When application-data locations are needed, they belong behind a
  single resolver, not sprinkled as `$HOME/...` strings.
* **Create platform abstractions when a requirement justifies them**, not for
  hypothetical scenarios. There is no abstraction layer today because nothing
  needs one yet; the rule is where it would go, not that it exists.

---

## Unresolved question: the Bevy version

The workspace is pinned to **Bevy 0.19.1**, paired with `bevy_brp_extras` /
`bevy_brp_mcp` **0.22.8** (that pairing is a hard compatibility constraint, see
`docs/BRP.md`). Bevy **0.20.0** is the current stable release.

This milestone deliberately did **not** upgrade. Rationale:

* the existing engine is a working 18 kLOC Bevy application with a matched BRP
  bridge; a minor-version bump is a real migration (the AGENTS.md trap list
  already records several 0.19 renames), not a version-string edit;
* an upgrade would touch every crate and every integration test and could leave
  the tree broken — which is exactly the outcome the "preserve existing work"
  constraint forbids;
* the new work in this milestone (the procedural assembly) uses only stable
  `Meshable`/primitive APIs, so it does not depend on either version's specifics.

**Next step if the upgrade is wanted:** a dedicated, bounded migration that bumps
`bevy`, `bevy_brp_extras` and `bevy_brp_mcp` together, runs
`cargo test --workspace` and the BRP smoke test, and records the result in
`docs/BRP.md`. Do it as its own change, not folded into a feature.
