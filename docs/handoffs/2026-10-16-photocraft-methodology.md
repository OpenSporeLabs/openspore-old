# Handoff — PhotoCraft methodology → Spore reconstruction methodology

- **Date:** 2026-10-16
- **Status:** complete (research); next experiment *proposed, not implemented*
- **Owner:** orchestrator
- **Base revision:** `main` @ `eb38cae` (uncommitted work preserved; nothing staged/committed)

## Objective & scope

Research how PhotoCraft reconstructs Photoshop and adapt the strongest,
**verified** parts into an OpenSpore reconstruction methodology. Deliverables:
a PhotoCraft case study, a Spore specification map, a unified evidence-model
proposal (reusing `spore_core::EvidenceLevel`), format/behaviour coverage models,
a methodology-to-Spore mapping table, narrow `AGENTS.md` corrections, and a dated
handoff. Explicit non-goals: no new dependency, no port, no new subsystem, no
wholesale `AGENTS.md` rewrite.

## What was discovered (with sources)

**PhotoCraft** (`storytold/photocraft`, `main` @ `2a952f164c0b4cf936b1c6497db10dd095341c00`,
2026-10-16T05:48:30Z; MIT+Apache-2.0; GitHub REST API was HTTP 403 so stars/issues
unverified). Its methodology, demonstrated by code/tests/CI:
- Generated, CI-checked status docs that cannot drift: `docs/parity-checklist.md`
  (`cargo xtask parity`), `docs/scorecard.md` (`cargo xtask scorecard --check`).
- Monotone regression floors: `FLOOR = 631` (`crates/ui-egui/src/parity.rs`),
  `PASS_FLOOR = 5`; "raise as features land, never lower".
- Independent executable oracle + explicit tolerance: CPU compositor
  `photocraft_compose::flatten` vs GPU (`crates/gpu/tests/parity.rs`, `1/255 + 4ε`);
  Photoshop-authored files (`PASS_TOL = 2/255`).
- Corpora never committed: `xtask/src/corpus_pins.rs` pins commits + SHA-256
  manifests, fetches to a gitignored `corpus/`, generates per-corpus licence
  READMEs (psd-tools MIT, ag-psd MIT, PngSuite public domain, etc.).
- Layering enforced in CI (`cargo xtask layers`, `xtask/src/layers.rs`; UI crates
  forbidden below L6). Never-crash lints on 29/29 crates.
- Honest gaps: readiness "~45%" is *estimated*; the one critical oracle (does
  Photoshop accept our exports) is manual (`docs/gaps.md` G1).

**Spore ecosystem** (survey 2026-10-16, `github.com/Spore-Community`):
- **No published Spore format spec exists** — all format knowledge is community
  RE or real files.
- `SporeModder-FX` (GPL-3.0): parsers/writers + `reg_type|file|property|simulator.txt`
  + `Documentation/*.txt`. `HashManager.java` proves name→id is **FNV-1**
  (`0x811C9DC5`, `0x1000193`, multiply-then-xor, lowercased).
- `SporeModder-Blender-Addons` (GPL-3.0): RW4/GMDL import↔export = a conformance oracle.
- `Spore-ModAPI`: **no licence** → code blocked; `SporeGhidra_*.xml` + `SDKtoGhidra/`
  readable as knowledge.
- `SporeTools`/`ModBrowser`/`SporeHash-VSCode` (MIT), `sporeget` (Unlicense).

**Local OpenSpore machinery (reused, not duplicated):** `spore_core::evidence`
(`EvidenceLevel` 7 rungs, `EvidenceState`, `SourceClass`, `Provenance`, `Fact<T>`);
per-crate `claims` modules (`spore-gmdl/src/claims.rs`, …); `spore-differential`
(`src/oracle.rs`, `src/compare/*`, `tests/*`); `docs/tooling/evidence-model.md`
(pack schema `openspore-evidence-pack-1`); `docs/tooling/reconstruction-coverage.md`
(`coverage.py`, refuses a single percentage); `docs/tooling/validation-dimensions.md`
(static vs runtime axes); `docs/replacement-status.json`.

## Changes made and why

1. **`AGENTS.md`** — two narrow corrections (§7 of the task), no other edits:
   - Removed "Reimplement by analysis only (clean room)"; replaced with a
     provenance-aware rule permitting licensed community reuse, forbidding
     proprietary EA/Maxis copying and Spore asset bytes, linking
     `docs/research/LICENSE-PROVENANCE.md`.
   - Fixed the self-contradictory sentence to: OpenSpore and `spore-recomp` are
     separate projects, neither may depend on the other as a production/build
     dependency, OpenSpore may read reports read-only; links `docs/BOUNDARIES.md`.
2. **New `docs/research/LICENSE-PROVENANCE.md`** — authoritative reuse policy +
   verified per-repo licence matrix + blocked register. (Per operator decision:
   OpenSpore is GPL-3.0-compatible; "no licence" repos remain blocked because
   default copyright applies, not because of copyleft.)
3. **New `docs/research/PHOTOCRAFT-METHODOLOGY-CASE-STUDY.md`**.
4. **New `docs/research/SPORE-SPECIFICATION-MAP.md`**.
5. **New `docs/architecture/SPECIFICATION-DRIVEN-RECONSTRUCTION.md`** — evidence
   model, 8-level format coverage + behaviour coverage, mapping table, next experiment.
6. **New `docs/handoffs/README.md`** and this handoff.
7. **New `docs/README.md`** — documentation index (none existed).

## Verification actually performed

- Inspected `git status`; **all pre-existing uncommitted work preserved**.
- `git diff --check` and the full diff reviewed — no unrelated code touched
  (AGENTS.md has exactly the two intended edits; the rest are new files).
- Docs authored from primary sources read this session (file contents, not
  summaries). PhotoCraft/ecosystem facts are cited to file/commit; the GitHub API
  rate-limit is recorded as a limitation, not worked around.
- **No build, test, clippy or fmt was run for this task** — the deliverables are
  Markdown + two comment-free rule edits, which do not compile. No build/test
  claim is made.

## Decisions & rejected alternatives

- **Extend, don't invent:** reuse `spore_core::EvidenceLevel`/`Fact` and the
  existing coverage/validation vocabulary; a new scale or a headline "% complete"
  was rejected (the repo already refuses one).
- **Narrow `AGENTS.md` edits only** (task §7); a full rewrite was rejected.
- **No PhotoCraft dependency/port** (task §11).
- **Next experiment = a coverage generator, not a parser** (task §11).
- Licensed community code may be reused (operator decision), but *no-licence*
  repos stay blocked — that is copyright default, not policy taste.

## Unresolved / assumptions / blockers

- GitHub REST API 403: PhotoCraft star/issue counts and some org-repo last-commit
  dates are unverified; refresh when the API is available.
- No primary Spore spec exists; property-list and animation families have **no**
  dedicated OpenSpore crate yet (gaps recorded in the spec map).
- `osptool format-coverage` is **proposed, not built**.

## Relevant files, symbols, commands

- `crates/spore-core/src/evidence.rs` (`EvidenceLevel`, `Fact`), `crates/spore-gmdl/src/claims.rs`.
- `crates/spore-differential/src/{oracle.rs,compare/*,corpus.rs}`; `tests/gmdl_reference_table.rs`.
- `docs/tooling/{evidence-model,reconstruction-coverage,validation-dimensions}.md`; `docs/replacement-status.json`.
- `python3 tools/openspore.py coverage` (existing, binary-scoped); `cargo test -p spore-differential --release -- --ignored` (real corpus).

## External repositories investigated / still unaudited

Investigated (read-only): `storytold/photocraft`; Spore-Community
SporeModder-FX, SporeModder-Blender-Addons, Spore-ModAPI, SporeTools,
ModAPI-Launcher-Kit, ModBrowser. Still unaudited: the badge-only org repos'
trees, `emd4600/SporeModder-FX` upstream, `Rosalie241/SporeModLoader`, and the
`modapi-docs.sporecommunity.com` content.

## Next action (exact)

Implement **`osptool format-coverage`** in `crates/spore-tools` per
`docs/architecture/SPECIFICATION-DRIVEN-RECONSTRUCTION.md` §7: (1) serialize each
crate `claims` module to an `openspore-spec-claim-1` index and round-trip it;
(2) emit a per-family 8-level coverage table (`openspore-format-coverage-1`) from
that index + `docs/replacement-status.json` + `spore-differential` test names;
(3) add a per-family monotone floor and a `--check` flag; (4) verify determinism
(byte-identical across two runs) and that it runs with no `SPORE/`, GPU or network.
Start from `crates/spore-tools` (the `osptool` binary) and the existing
`coverage.py` shape; do **not** add format parsing.

## Continuation prompt (fresh session)

> Read `docs/handoffs/2026-10-16-photocraft-methodology.md` and
> `docs/architecture/SPECIFICATION-DRIVEN-RECONSTRUCTION.md` §7. In
> `crates/spore-tools`, add `osptool format-coverage`: a read-only, deterministic
> generator that builds an `openspore-spec-claim-1` index from the format crates'
> `claims` modules and emits an `openspore-format-coverage-1` table over the 8
> levels per family, sourced from `docs/replacement-status.json` and
> `spore-differential` test names, with a per-family monotone floor and a
> `--check`. First prove the claims→index round-trip; add a hermetic unit test
> (no `SPORE/`). Do not add a new dependency or any format parsing. Run
> `cargo test -p spore-tools` and `cargo clippy -p spore-tools --all-targets -- -D warnings`.
