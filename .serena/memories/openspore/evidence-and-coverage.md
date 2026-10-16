# OpenSpore — evidence, claims, coverage

Canonical vocab lives in `crates/spore-core/src/evidence.rs` — reuse, never reinvent:
- `EvidenceLevel` 7 rungs: UNKNOWN < APPROXIMATION < INFERRED < SUPPORTED <
  OBSERVED < CONFIRMED < VERIFIED.
- `EvidenceState` (LIVE|DERIVED|PERSISTED|MISSING), `SourceClass`
  (generated_index|committed_artifact|ghidra|derived|runtime; static excludes runtime).
- `Fact<T>`: Available (must carry provenance) | Unavailable (=> Unknown+Missing,
  carries reason). **Unavailable is never 0/empty/false.**

Per-crate `claims` modules (`spore-gmdl/src/claims.rs`, `spore-texture/src/claims.rs`,
`spore-material`, `spore-rigblock`, `spore-creator-core`, `spore-cellcontent`,
`spore-semantic-bridge`) list what the crate *does not know*, each graded. Extend,
never delete. `claim(subject) -> Option<Fact>`; absence != UNKNOWN.

Coverage/validation (binary-scoped, over `SporeApp.exe`):
- `docs/tooling/reconstruction-coverage.md` + `coverage.py` — a matrix of
  separately-defined dimensions that **refuses a single % complete**.
- `docs/tooling/validation-dimensions.md` — static verdict (PASS/WARN/FAIL/UNKNOWN/
  NOT_AVAILABLE) is never a runtime claim; runtime is GATED/PASS; GATED != failure.
- `docs/tooling/evidence-model.md` — evidence-pack schema `openspore-evidence-pack-1`.
- Differential harness: `crates/spore-differential` (`src/oracle.rs`, `src/compare/*`)
  = Rust parsers vs Python oracles in `tools/spore`.

Gap: no per-**format-family** coverage view; the methodology doc proposes one
(see `openspore/next-task`).
