# OpenSpore — next task (2026-10-16)

Implement `osptool format-coverage` in `crates/spore-tools`, per
`docs/architecture/SPECIFICATION-DRIVEN-RECONSTRUCTION.md` §7.

Read-only, deterministic generator:
1. Build an `openspore-spec-claim-1` index from the format crates' `claims`
   modules; prove a round-trip first.
2. Emit an `openspore-format-coverage-1` table: per resource family, which of the
   8 levels are achieved (identified / parsed / structurally validated / decoded
   to IR / deps resolved / preserved-round-tripped / converted to runtime assets /
   validated vs independent evidence). Sources: the claims index +
   `docs/replacement-status.json` + `spore-differential` test names.
3. Per-family **monotone floor** + `--check` (fails on drift), mirroring
   `coverage.py` and PhotoCraft's scorecard.
4. Emit no single "% complete". Two runs over identical inputs must be
   byte-identical. Must run with no `SPORE/`, no GPU, no network.

Do NOT add a new dependency or any format parsing. Verify:
`cargo test -p spore-tools` + `cargo clippy -p spore-tools --all-targets -- -D warnings`.

Context: `docs/handoffs/2026-10-16-photocraft-methodology.md` (has a continuation prompt).
