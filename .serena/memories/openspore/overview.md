# OpenSpore — orientation

Spore engine + creation studio: Bevy 0.19.1 Rust workspace (`crates/`, 15 crates)
alongside a C++17/CMake + Python RE corpus (`src/`, `tools/`, `knowledge*/`) that
is the differential reference. `AGENTS.md` is the compact rules router.

- Engine: `spore-engine` (Bevy). `spore-tools` = `osptool` CLI.
- Load-bearing invariant: `spore-core` .. `spore-material`, plus `spore-creator-core`,
  `spore-rigblock`, `spore-morphology`, `spore-differential` are **Bevy-free**;
  `spore-assets` never touches a GPU. Keep it.
- Build/test: `cargo test --workspace` (hermetic: no game, no GPU);
  `cargo clippy --workspace --all-targets -- -D warnings`; `cargo fmt --all --check`.
  Vertical slice: `cargo run -p spore-engine -- --preset documented-asset --package SPORE/Data/Spore_Content.package`.
- Hard rules: no Spore asset byte committed (SPORE/, *.package gitignored);
  no proprietary EA/Maxis source; OpenSpore and `spore-recomp` are separate and
  mutual-non-dependency (read its reports read-only only); never modify
  `.serena/project.yml`; do not reset/stage/commit.
- Reuse policy: see `docs/research/LICENSE-PROVENANCE.md` (GPL-3.0-compatible;
  no-licence repos are blocked code, readable knowledge).

Docs map: `docs/README.md`. Status: `docs/STATE.md`, `docs/MIGRATION.md`,
`docs/replacement-status.json`, `docs/BOUNDARIES.md`.
