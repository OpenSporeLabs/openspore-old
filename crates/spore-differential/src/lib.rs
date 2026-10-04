//! A differential harness for the Rust asset parsers.
//!
//! The Rust crates in this workspace were ported from two references: a C++17
//! implementation under `src/assets/` and independent stdlib-only Python oracles
//! under `tools/spore/`. Both agree on the committed synthetic fixtures in
//! `tests/fixtures/`. **Neither has ever been checked against the whole real
//! corpus**, which is what this crate is for.
//!
//! A port that has only ever met its own fixtures is not validated. It agrees
//! with the fixture generator by construction. What it has not done is meet the
//! 5 GB of real records in `SPORE/Data/*.package` and
//! `SPORE/DataEP1/*.package`, where the format's corners actually live.
//!
//! # What it does
//!
//! For each question it asks, it runs **one oracle** and **one Rust
//! implementation** over the **same records** and diffs them:
//!
//! | report | question | Rust | oracle |
//! |---|---|---|---|
//! | [`compare::dbpf_index`] | do we read the same index rows? | `spore_dbpf::parse_index` | `dbpf.py::read` |
//! | [`compare::record_bytes`] | do we decompress to the same bytes? | `spore_dbpf::extract_record` | `dbpf.py::getdata` |
//! | [`compare::rw4`] | do we print the same section-walk line? | `spore_rw4::Rw4::describe` | `rw4.py::describe` |
//! | [`compare::gmdl`] | do we walk a model the same way? | `spore_gmdl::parse` | `gmdl.py::Gmdl` |
//! | [`compare::raster`] | do we read the same envelope, the same layer count and the same pixels? (the raster oracle's framing is **refuted** — see that module) | `spore_texture` | `raster.py`, `dxt5.py` |
//! | [`compare::manifest`] | do we build the same rows, and what do we mean by "decodable"? | `ManifestBuilder::from_store` | `manifest.py` |
//! | [`compare::worldobj`] | what does the oracle see where we have no decoder? | *(none)* | `worldobj.py` |
//!
//! The Python oracles are the specification. **They are never modified to make
//! them agree.** If two implementations disagree, that disagreement is the
//! deliverable.
//!
//! One oracle has since been *refuted by the corpus* rather than by this crate:
//! `raster.py`'s derived layer count and `dxt5.py`'s 8-byte block both encode a
//! raster framing that a byte count over 4 386 of 4 386 records rejects (see
//! `docs/RASTER-FRAMING.md` §2 and [`compare::raster`]). The rule above still
//! holds — the files are not edited — so the disagreement is now the reported
//! result of that comparison, classified and attributed per record. The framing
//! itself is checked against a second transcription of the measured table in
//! Python instead ([`oracle`], "Why one command is NOT an oracle at all").
//!
//! # Running it
//!
//! `cargo test -p spore-differential` is hermetic and needs no game: it runs
//! against the committed fixtures and against an empty corpus. The real corpus
//! lives behind `#[ignore]`:
//!
//! ```text
//! cargo test -p spore-differential --release -- --ignored --nocapture
//! ```
//!
//! # A divergence has to quote both sides
//!
//! [`Divergence`] carries the record's identity, the field path that differs,
//! and **both** values verbatim, and its `Display` prints all three. A finding
//! that cannot be quoted on both sides is an opinion, and this crate does not
//! emit opinions as findings.
//!
//! Three verdicts that are *not* bugs are named explicitly, because collapsing
//! them into "disagreement" would drown the disagreements that are real:
//!
//! * [`Agreement::BothFailed`] -- both decoders refused the same bytes. That is
//!   agreement about the bytes.
//! * [`DivergenceClass::Definitional`] -- the two sides answer different
//!   questions (`spore-assets` means "a per-record decoder exists in this
//!   workspace" where `manifest.py` means "looks decodable"). Reported in full,
//!   not counted as a defect.
//! * [`DivergenceClass::OneSidedDecode`] -- exactly one side produced a value.
//!   That *is* a finding: whichever side refused something the other handled.
//!
//! # Missing inputs are ordinary
//!
//! [`Corpus::discover`] returns an **empty corpus, not an error**, when there is
//! no `SPORE/` tree, and every comparison over an empty corpus reports
//! [`Status::Skipped`] with a sentence saying why. A machine without the game,
//! and a checkout without `python3`, both produce a well-formed answer: this did
//! not run. Neither is ever reported as agreement.
//!
//! # Memory discipline
//!
//! The corpus is ~5 GB and the largest package is 995 MB. The harness is
//! memory-flat on the Rust side:
//!
//! * a package is memory-mapped read-only through `spore-assets`, so the image
//!   stays virtual (`osptool verify` does the same at 4 MB peak RSS against
//!   949 MB of mapped data);
//! * exactly one record's payload resident at a time, dropped at the end of each
//!   iteration, with sampled comparisons bounded deterministically;
//! * the only structure that grows with the corpus is the index, which is a few
//!   tens of thousands of fixed-width rows (about 700 KB for 17 119 rows).
//!
//! **One** oracle process per package, so `dbpf.read`'s whole-file slurp happens
//! once and only one package is ever held.
//!
//! # What this costs, measured
//!
//! Measured on `Spore_Content.package` (995 268 083 bytes, 17 119 records), the
//! index comparison, peak RSS of the whole process tree:
//!
//! | | peak RSS |
//! |---|---|
//! | the oracle alone (`dbpf.read` + `getdata`) | **973 MiB** |
//! | the whole tree, oracle plus this harness | **997 MiB** |
//!
//! So the harness adds about 24 MiB over the oracle's own unavoidable cost --
//! and 973 of the 997 is `dbpf.read` slurping the package into a `bytes`, which
//! is the oracle's design and is **not** corrected here (see
//! [`compare::dbpf_index`]). The Rust side holds none of it: the package is
//! mapped, not read. Nothing accumulates across records on either side.
//!
//! # No new dependencies
//!
//! The crate depends on the workspace's own `spore-*` crates and nothing else --
//! no `serde`. The JSON it exchanges with the Python driver goes through
//! [`json`], a small hand-rolled reader and writer.
//!
//! [`json`] is tolerant **on purpose**: it accepts the bare `NaN` / `Infinity`
//! literals Python's `json.dumps` emits (gmdl bounding boxes really do contain
//! them), ignores keys it does not know, and reports a parse failure as a typed
//! error with a byte offset instead of a panic. A differential harness that
//! crashes on a formatting difference cannot report a semantic one.

#![forbid(unsafe_code)]
#![warn(missing_docs, missing_debug_implementations)]

pub mod compare;
pub mod corpus;
pub mod floats;
pub mod json;
pub mod oracle;
pub mod report;
pub mod sample;

pub use corpus::{Corpus, CorpusError, LocatedPackage, LocatedRecord, RecordKind};
pub use floats::{describe_tolerance, FLOAT_TOLERANCE};
pub use json::{Json, JsonError};
pub use oracle::{OracleConfig, OracleError, OraclePackageInfo, OracleSession};
pub use report::{
    Agreement, DifferentialReport, Divergence, DivergenceClass, RecordIdentity, Status,
};

/// Runs every comparison over one corpus, in [`compare::ALL`] order.
///
/// Never fails: a comparison whose oracle cannot be reached comes back as a
/// `Skipped` report, so one missing interpreter does not hide the six results
/// behind it.
#[must_use]
pub fn run_suite(corpus: &Corpus, config: &OracleConfig) -> Vec<DifferentialReport> {
    vec![
        compare::dbpf_index::compare(corpus, config),
        compare::record_bytes::compare(corpus, config),
        compare::rw4::compare(corpus, config),
        compare::gmdl::compare(corpus, config),
        compare::raster::compare(corpus, config),
        compare::manifest::compare(corpus, config),
        compare::worldobj::compare(corpus, config),
    ]
}

/// A multi-line summary of a whole suite run.
#[must_use]
pub fn summarize(reports: &[DifferentialReport]) -> String {
    let mut out = String::new();
    for report in reports {
        out.push_str(&report.summary());
        out.push_str("\n\n");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_suite_runs_every_declared_comparison_over_an_empty_corpus() {
        let corpus = Corpus::empty();
        let reports = run_suite(&corpus, &OracleConfig::detect());
        assert_eq!(reports.len(), compare::ALL.len());
        for (report, name) in reports.iter().zip(compare::ALL) {
            assert_eq!(report.comparison, *name);
            assert!(report.status.is_skipped(), "{name} ran on an empty corpus");
            assert!(
                !report.is_clean(),
                "{name} must never look clean when skipped"
            );
            assert!(report.summary().contains("skipped"), "{name}");
        }
    }

    #[test]
    fn a_summary_mentions_every_comparison() {
        let text = summarize(&run_suite(&Corpus::empty(), &OracleConfig::detect()));
        for name in compare::ALL {
            assert!(text.contains(name), "{name} missing from the summary");
        }
    }
}
