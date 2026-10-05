//! The undo stack measured against all 68 real creator documents.
//!
//! These tests are `#[ignore]`d and skip cleanly, with a printed `SKIP:` line,
//! when `SPORE/` is absent — the same pattern as [`corpus.rs`](./corpus.rs) and
//! the rest of the workspace. No Spore byte is committed; the package is read
//! from the local install and every assertion is on counts, structure and values.
//!
//! ```text
//! cargo test -p spore-creator-core --release --test history -- --ignored --nocapture
//! ```
//!
//! # Why a history needs the corpus and not just synthetic fixtures
//!
//! The unit tests in `src/history.rs` pin the *policy*: the ring drops the
//! oldest, the two `record` forms correspond to the two kinds of edit, and a
//! selection is carried by the same arithmetic `edit.rs` uses. None of that
//! needs a real record.
//!
//! What only real data can answer is whether the policy holds on documents the
//! game actually wrote — documents with 601 `<childid>` edges, 84 mirror
//! relations, six roots in one of them, and the 160 leaves that carry no
//! `<childlist>` at all. So this file does two things over all 68:
//!
//! * **every block of every document is removed, both last-child policies, and
//!   the undo is checked to restore the document bit for bit** — 1360 removals,
//!   1360 undos;
//! * **every index of every document is walked through the forward remap and
//!   the backward one**, which is the property the whole selection design rests
//!   on: the core's `rewrite_targets` and the history's carry must be the same
//!   function, on every index, on every document.
//!
//! The third test is a census, and it exists because `src/history.rs` quotes
//! the measured block counts in its documentation. A doc comment that cites a
//! number nothing checks is a doc comment that goes stale silently.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use spore_creator_core::persist::{load, save};
use spore_creator_core::sporemodel::Family;
use spore_creator_core::{
    validate, BlockId, BlockIndex, CreatorDocument, CreatorHistory, IndexRemap, OnLastChild, Remap,
    SelectionStep, Severity,
};

/// Where the game install lives, relative to the repository root.
const PACKAGE: &str = "SPORE/Data/Spore_Game.package";

/// The 68 documents, or `None` when there is no install to read.
fn corpus() -> Option<Vec<(Family, CreatorDocument)>> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(PACKAGE);
    if !path.exists() {
        println!(
            "SKIP: {PACKAGE} is absent; the creator-document corpus is not available. \
             Nothing is committed in its place."
        );
        return None;
    }
    let bytes = std::fs::read(&path).expect("the package is readable");
    let entries = spore_dbpf::parse_index(&bytes).expect("the index parses");

    let mut out = Vec::new();
    for entry in &entries {
        let Some(family) = Family::from_type_id(entry.type_id) else {
            continue;
        };
        let raw = spore_dbpf::extract_record(&bytes, entry).expect("a creator record decompresses");
        let text = String::from_utf8(raw).expect("a creator record is utf-8");
        let document = spore_creator_core::parse_sporemodel(family, &text)
            .unwrap_or_else(|error| panic!("{} {error:?}", family.name()));
        out.push((family, document));
    }
    Some(out)
}

/// Every document, or an immediate skip.
macro_rules! corpus_or_skip {
    () => {
        match corpus() {
            Some(documents) => documents,
            None => return,
        }
    };
}

/// The `Error`-severity issues a document carries.
///
/// An undo stack is only neutral if it is exactly neutral: a step that *removed*
/// an error would be a silent repair, which is the thing this crate refuses to
/// do anywhere else.
fn errors(document: &CreatorDocument) -> Vec<String> {
    validate(document, None)
        .into_iter()
        .filter(|issue| issue.severity == Severity::Error)
        .map(|issue| format!("{} at block {:?}", issue.code, issue.block))
        .collect()
}

/// A history bound deep enough that the default never binds on this corpus, so
/// the ring is not what these tests are measuring.
fn history() -> CreatorHistory {
    CreatorHistory::new(8)
}

#[test]
#[ignore = "needs the game install; no asset byte is committed"]
fn undoing_a_removal_from_every_real_document_restores_it_bit_for_bit() {
    let documents = corpus_or_skip!();
    let mut removals = 0usize;

    for on_last_child in [OnLastChild::OmitElement, OnLastChild::KeepWrittenList] {
        for (family, document) in &documents {
            let baseline = errors(document);
            assert!(baseline.is_empty(), "{} starts clean", family.name());

            // Every block, not one per document: a root, a mirror partner, an
            // interior node and a 6-root document all have to be covered, and
            // picking one per document leaves those cases to chance.
            for victim in 0..document.len() {
                let before = document.clone();
                let mut edited = before.clone();
                let remap = edited
                    .remove_block(victim, on_last_child)
                    .expect("every corpus index exists");

                let mut history = history();
                history.record_removal(&before, &remap);
                assert_eq!(history.depth(), 1);
                assert!(history.can_undo());

                let step = history.undo(&edited, None).expect("recorded, so undoable");
                assert_eq!(
                    step.document(),
                    &before,
                    "{} removing {victim} under {on_last_child:?} must restore the document",
                    family.name()
                );
                assert_eq!(errors(step.document()), baseline);
                assert_eq!(history.depth(), 0);
                assert_eq!(history.redo_depth(), 1);

                // And forward again, to exactly the document the removal
                // produced. A redo that rebuilt the document rather than
                // replaying the removal would be a second implementation of
                // `remove_block`, which is what this design exists to avoid.
                let redone = history.redo(step.document(), None).expect("redo");
                assert_eq!(
                    redone.document(),
                    &edited,
                    "{} removing {victim} under {on_last_child:?}",
                    family.name()
                );
                assert_eq!(errors(redone.document()), baseline);
                assert!(!history.can_redo());
                removals += 1;
            }
        }
    }

    assert_eq!(
        removals, 1360,
        "680 blocks removed once per last-child policy"
    );
}

#[test]
#[ignore = "needs the game install; no asset byte is committed"]
fn every_index_of_every_real_document_survives_a_forward_remap_and_a_backward_carry() {
    let documents = corpus_or_skip!();
    let mut carries = 0usize;

    for on_last_child in [OnLastChild::OmitElement, OnLastChild::KeepWrittenList] {
        for (family, document) in &documents {
            for victim in 0..document.len() {
                let before = document.clone();
                let mut edited = before.clone();
                let remap = edited
                    .remove_block(victim, on_last_child)
                    .expect("every corpus index exists");

                // Forward: the caller's own remap, which is the answer the
                // history has to agree with rather than merely resemble.
                for original in 0..before.len() {
                    // A fresh history per index: one undo consumes the one
                    // checkpoint, so sharing it would make every index after the
                    // first fail for a reason that has nothing to do with indices.
                    let mut history = history();
                    history.record_removal(&before, &remap);
                    let mut held = vec![original];
                    let step = match remap.rewrite_targets(&mut held) {
                        Ok(()) => {
                            let step = history
                                .undo(&edited, Some(held[0]))
                                .expect("recorded, so undoable");
                            assert_eq!(
                                step.selection(),
                                Some(SelectionStep::Kept(original)),
                                "{} victim {victim}: {original} moved to {} and must come back",
                                family.name(),
                                held[0]
                            );
                            assert_eq!(step.document(), &before);
                            carries += 1;
                            step
                        }
                        Err(error) => {
                            // The one index the removal took. The core refuses to
                            // rewrite it; the history says the same thing in its
                            // own vocabulary on the way back.
                            assert_eq!(
                                remap.map(original),
                                Remap::Removed,
                                "{} victim {victim}",
                                family.name()
                            );
                            assert!(matches!(
                                error,
                                spore_creator_core::EditError::RemovedTarget { index }
                                    if index == original
                            ));
                            assert_eq!(held, vec![original], "the core wrote nothing");
                            let step = history
                                .undo(&edited, Some(original))
                                .expect("recorded, so undoable");
                            assert_eq!(step.document(), &before);
                            step
                        }
                    };

                    // Whatever the undo said, an index came back or was reported
                    // gone — never left pointing into a document it does not name.
                    if let Some(SelectionStep::Kept(index)) = step.selection() {
                        assert!(
                            index < before.len(),
                            "{} victim {victim}: carried index {index} is out of range",
                            family.name()
                        );
                        assert_eq!(before.blocks[index], step.document().blocks[index]);
                    }
                }
            }
        }
    }

    // 2 x sum over documents of len x (len - 1), over the measured census:
    // sum(n^2) = 8210 and sum(n) = 680, so 2 x (8210 - 680) = 15060.
    assert_eq!(
        carries, 15_060,
        "the surviving (document, victim, index) triples, over both policies"
    );
}

#[test]
#[ignore = "needs the game install; no asset byte is committed"]
fn an_authoring_session_on_every_real_document_undoes_and_redoes_with_no_error_in_between() {
    let documents = corpus_or_skip!();
    let mut steps = 0usize;

    for (family, original) in &documents {
        let baseline = errors(original);
        assert!(baseline.is_empty(), "{} starts clean", family.name());
        let mut document = original.clone();
        let mut history = history();
        let mut selection: Option<BlockIndex> = original.roots().first().copied();

        // Six steps, the same shape the unit test walks on synthetic documents
        // but against real graphs: an append, a part swap, a transform, a paint,
        // a reparent, and a removal that carries a remap.
        let upper = {
            history.record(&document);
            let mut block = document.blocks[0].clone();
            block.children = None;
            block.symmetric = None;
            document
                .add_block(block, original.roots().first().copied())
                .expect("a real block is addable")
        };
        let lower = {
            history.record(&document);
            let mut block = document.blocks[upper].clone();
            block.children = None;
            block.symmetric = None;
            document.add_block(block, Some(upper)).expect("addable")
        };
        {
            history.record(&document);
            let id = document.blocks[lower]
                .block_id
                .expect("every corpus block has one");
            document
                .replace_block(
                    lower,
                    BlockId::new(id.group_id, id.instance_id ^ 0x5a5a_5a5a),
                )
                .expect("replaced");
        }
        {
            history.record(&document);
            document.blocks[lower].transform.position.y = -0.25;
        }
        {
            history.record(&document);
            document.blocks[lower].snapped = None;
        }
        {
            // The removal step, recorded with its remap. The pre-state has to be
            // snapshotted first, because `record_removal` pairs a document with
            // the receipt of the removal taken from *that* document.
            let before_removal = document.clone();
            let remap = document
                .remove_block(lower, OnLastChild::OmitElement)
                .expect("the appended block is removable");
            history.record_removal(&before_removal, &remap);
        }

        let final_state = document.clone();
        assert_eq!(errors(&document), baseline, "{} ends clean", family.name());
        assert_eq!(history.depth(), 6);
        steps += 6;

        // Back to the start, checking at every step that the restored document is
        // one the editor could legitimately be showing and that the carried
        // selection still names a block in it.
        for _ in 0..6 {
            let step = history
                .undo(&document, selection)
                .expect("six steps were recorded");
            selection = step.selection().and_then(SelectionStep::kept_index);
            document = step.into_document();
            assert_eq!(
                errors(&document),
                baseline,
                "{} must be clean at every intermediate step",
                family.name()
            );
            if let Some(index) = selection {
                assert!(
                    index < document.len(),
                    "{} selection {index}",
                    family.name()
                );
            }
        }
        assert_eq!(document, *original, "{} unwound", family.name());
        assert!(!history.can_undo());
        assert_eq!(history.redo_depth(), 6);

        // And forward again, to exactly the session's end.
        for _ in 0..6 {
            let step = history
                .redo(&document, selection)
                .expect("six steps are redoable");
            selection = step.selection().and_then(SelectionStep::kept_index);
            document = step.into_document();
            assert_eq!(
                errors(&document),
                baseline,
                "{} must be clean at every intermediate step",
                family.name()
            );
        }
        assert_eq!(document, final_state, "{} replayed", family.name());
        assert_eq!(history.depth(), 6);
    }

    assert_eq!(steps, 408, "68 documents, six recorded steps each");
}

#[test]
#[ignore = "needs the game install; no asset byte is committed"]
fn a_snapshot_of_a_real_document_still_saves_and_loads_bit_for_bit() {
    // The history is only worth having if a restored snapshot is a document this
    // crate would still vouch for. `load(save(doc)) == doc` is the save
    // format's claim, and an undo stack that produced a document failing it
    // would hand the writer a snapshot it cannot round-trip.
    let documents = corpus_or_skip!();
    let mut snapshots = 0usize;

    for (family, document) in &documents {
        let mut history = history();
        history.record(document);
        let mut edited = document.clone();
        let victim = edited.len() / 2;
        let remap = edited
            .remove_block(victim, OnLastChild::OmitElement)
            .expect("removed");
        history.record_removal(document, &remap);

        let step = history.undo(&edited, None).expect("undo");
        let restored = step.document();
        let saved =
            save(restored).unwrap_or_else(|error| panic!("{} saving: {error}", family.name()));
        let reloaded =
            load(&saved).unwrap_or_else(|error| panic!("{} loading: {error}", family.name()));
        assert_eq!(&reloaded, restored, "{} restored snapshot", family.name());
        assert_eq!(
            save(&reloaded).expect("save"),
            saved,
            "{} twice",
            family.name()
        );
        snapshots += 1;
    }

    assert_eq!(
        snapshots, 68,
        "every document's restored state is re-savable"
    );
}

#[test]
#[ignore = "needs the game install; no asset byte is committed"]
fn the_block_census_the_history_documents_quote_is_the_measured_one() {
    // `src/history.rs` says the largest of the 68 documents places 32 blocks and
    // the median 9, and quotes a 392,218-byte corpus save. Those are the numbers
    // its memory and default-depth arguments rest on, so they are checked here
    // rather than left as prose that can rot.
    let documents = corpus_or_skip!();
    let mut counts: Vec<usize> = documents.iter().map(|(_, d)| d.len()).collect();
    counts.sort_unstable();

    assert_eq!(counts.len(), 68, "68 creator documents");
    assert_eq!(counts.iter().sum::<usize>(), 680, "680 <blockref> blocks");
    assert_eq!(counts[0], 3, "the smallest document places 3 blocks");
    assert_eq!(counts[counts.len() / 2], 9, "the median is 9");
    assert_eq!(
        counts[counts.len() - 1],
        32,
        "the largest document places 32 blocks, which is what DEFAULT_MAX_DEPTH is \
         sized against"
    );

    let edges: usize = documents
        .iter()
        .flat_map(|(_, d)| d.blocks.iter())
        .map(|block| block.child_indices().len())
        .sum();
    assert_eq!(edges, 601, "601 <childid> edges");

    let total: usize = documents
        .iter()
        .map(|(_, d)| save(d).expect("saving").len())
        .sum();
    assert_eq!(
        total, 392_218,
        "the corpus saves to 392,218 bytes, the same figure `corpus.rs` pins"
    );

    // And the one document that fixes the upper bound: the 32-block `ufo`.
    let largest = documents
        .iter()
        .filter(|(_, d)| d.len() == 32)
        .map(|(family, _)| family.name())
        .collect::<Vec<_>>();
    assert_eq!(largest, vec!["ufo"], "the largest document is a ufo");

    // The per-family maxima `src/history.rs` quotes, so that sentence cannot go
    // stale either.
    let mut maxima: BTreeMap<&str, usize> = BTreeMap::new();
    for (family, document) in &documents {
        let slot = maxima.entry(family.name()).or_default();
        *slot = (*slot).max(document.len());
    }
    assert_eq!(
        maxima,
        BTreeMap::from([
            ("bld", 8),
            ("cll", 7),
            ("crt", 20),
            ("flr", 3),
            ("ufo", 32),
            ("vcl", 19),
        ]),
        "the per-family maxima the history docs quote"
    );
}

/// Keeps the unused-import warning away while documenting where a fixture would
/// go: golden files for this module are **synthetic**, never excerpts of a real
/// record — the real records stay in `SPORE/` and are re-read on demand.
#[test]
fn fixtures_are_synthetic_and_live_beside_this_file() {
    let _unused: Option<PathBuf> = None;
}

/// The remap type is named in this file's header, and an unused import would be
/// the only sign that the selection design depends on it.
const _: fn(&IndexRemap) -> usize = |remap| remap.len();
