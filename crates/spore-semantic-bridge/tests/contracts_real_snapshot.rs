//! The contract layer against the committed 98 MB snapshot.
//!
//! These cases are `#[ignore]`d **and** skip when the artifact is absent, so a
//! machine without it runs the suite green while a machine with it can say
//! exactly what it checked:
//!
//! ```sh
//! cargo test -p spore-semantic-bridge --test contracts_real_snapshot -- --ignored
//! ```
//!
//! # Why the numbers are pinned
//!
//! Every count here is a property of the committed artifact, not of the contract
//! layer, and each was measured over the whole file rather than sampled. They are
//! pinned so that a re-export which changes what the passport knows cannot pass
//! unnoticed, and so that the prose in `docs/SEMANTIC-CONTRACTS.md` has something
//! to be wrong about.
//!
//! Nothing here reads a runtime overlay, a runtime producer's report or any game
//! asset. The runtime half arrives as a documented artifact id inside the
//! committed catalog, which is the whole point of
//! [`crate::claims::RUNTIME_BASIS_IS_A_REFERENCE_NOT_A_PARSE`].

mod support;

use spore_semantic_bridge::invariants::all_lifecycle;
use spore_semantic_bridge::{
    all_invariants, Canonicalization, ContractLedger, EvidenceLevel, PassportStance, Snapshot,
};

/// Records carrying an SDK name, and the count of lifecycle tokens among them.
const SDK_NAMED: usize = 1_171;
/// Records whose SDK name's final segment is one of the closed token set.
const LIFECYCLE_ROLES: usize = 55;
/// Records with at least one vtable membership.
const DISPATCH_RECORDS: usize = 6_712;
/// Records with an ABI record that names a convention.
const ABI_KNOWN: usize = 407;
/// Records with an ABI record whose convention is `null`.
const ABI_UNDETERMINED: usize = 233;
/// Records stating an open runtime gate.
const OPEN_GATES: usize = 423;
/// Records stating a closed runtime gate.
const CLOSED_GATES: usize = 191;
/// Records with a reconstruction record that states no gate at all.
const UNSTATED_GATES: usize = 113;

/// The address a runtime virtual call reached that the universe cannot place.
const UNPLACED_RUNTIME_ADDRESS: u32 = 0x0092_5050;

fn snapshot() -> Option<&'static Snapshot> {
    static ONCE: std::sync::OnceLock<Option<Snapshot>> = std::sync::OnceLock::new();
    ONCE.get_or_init(|| {
        let path = support::committed_path()?;
        Some(
            Snapshot::open(&path)
                .unwrap_or_else(|error| panic!("the committed snapshot must open: {error}")),
        )
    })
    .as_ref()
}

macro_rules! snapshot_or_skip {
    () => {
        match snapshot() {
            Some(snapshot) => snapshot,
            None => {
                eprintln!(
                    "SKIPPED: knowledge/semantic/function-passport-v1.jsonl is not present; the \
                     artifact is optional and this case cannot run without it"
                );
                return;
            }
        }
    };
}

/// One ledger for the whole suite: building it costs a second full parse.
fn ledger() -> Option<&'static ContractLedger<'static>> {
    static LEDGER: std::sync::OnceLock<Option<ContractLedger<'static>>> =
        std::sync::OnceLock::new();
    LEDGER
        .get_or_init(|| {
            let snapshot = snapshot()?;
            Some(
                ContractLedger::build(snapshot)
                    .expect("the committed snapshot must project into a ledger"),
            )
        })
        .as_ref()
}

macro_rules! ledger_or_skip {
    () => {
        match ledger() {
            Some(ledger) => ledger,
            None => {
                eprintln!(
                    "SKIPPED: knowledge/semantic/function-passport-v1.jsonl is not present; the \
                     artifact is optional and this case cannot run without it"
                );
                return;
            }
        }
    };
}

#[test]
#[ignore = "reads the committed 98 MB snapshot; run with --ignored"]
fn the_projection_covers_every_record_exactly_once() {
    let snapshot = snapshot_or_skip!();
    let ledger = ContractLedger::build(snapshot).expect("the snapshot must project");
    assert_eq!(ledger.len(), snapshot.function_count());
    assert!(!ledger.is_empty());
    assert_eq!(
        ledger.dispatch_record_count(),
        DISPATCH_RECORDS,
        "the number of records carrying a table membership"
    );
    assert_eq!(
        ledger.lifecycle_record_count(),
        LIFECYCLE_ROLES,
        "the closed token set matches {LIFECYCLE_ROLES} of the {SDK_NAMED} named records; a \
         prefix rule would have matched more and been wrong about several"
    );
    assert_eq!(ledger.known_conventions().count(), ABI_KNOWN);
    assert_eq!(ledger.open_gate_count(), OPEN_GATES);
}

#[test]
#[ignore = "reads the committed 98 MB snapshot; run with --ignored"]
fn every_abi_record_is_either_determined_or_an_explicit_abstention() {
    let _ = snapshot_or_skip!();
    let ledger = ledger_or_skip!();
    let mut undetermined = 0usize;
    let mut not_recorded = 0usize;
    for passport in ledger.snapshot().passports() {
        let passport = passport.expect("the committed snapshot must parse");
        match ledger.for_va(passport.canonical_va()).abi {
            spore_semantic_bridge::AbiStanding::Known { .. } => {}
            spore_semantic_bridge::AbiStanding::Undetermined { level, .. } => {
                undetermined += 1;
                assert_eq!(
                    level,
                    EvidenceLevel::Unknown,
                    "{} declines to name a convention, so its grade is UNKNOWN",
                    passport.canonical_va_text()
                );
                assert_eq!(
                    passport.convention().reason(),
                    Some(spore_semantic_bridge::REASON_CONVENTION_UNDETERMINED)
                );
            }
            spore_semantic_bridge::AbiStanding::NotRecorded { reason } => {
                not_recorded += 1;
                assert!(
                    !reason.is_empty(),
                    "a missing record must carry the format's own reason, never a default"
                );
            }
        }
    }
    assert_eq!(undetermined, ABI_UNDETERMINED);
    assert_eq!(
        undetermined + not_recorded + ABI_KNOWN,
        ledger.len(),
        "the three outcomes must partition the corpus: no fourth state, no collapse"
    );
}

#[test]
#[ignore = "reads the committed 98 MB snapshot; run with --ignored"]
fn the_four_gate_states_are_counted_separately() {
    let _ = snapshot_or_skip!();
    let ledger = ledger_or_skip!();
    let mut open = 0usize;
    let mut closed = 0usize;
    let mut unstated = 0usize;
    let mut no_record = 0usize;
    for passport in ledger.snapshot().passports() {
        let passport = passport.expect("the committed snapshot must parse");
        match ledger.for_va(passport.canonical_va()).runtime_gate {
            None => no_record += 1,
            Some(gate) => match gate.gate {
                spore_semantic_bridge::RuntimeGate::Open => open += 1,
                spore_semantic_bridge::RuntimeGate::Closed => closed += 1,
                spore_semantic_bridge::RuntimeGate::Unstated => unstated += 1,
            },
        }
    }
    assert_eq!(open, OPEN_GATES);
    assert_eq!(closed, CLOSED_GATES);
    assert_eq!(
        unstated, UNSTATED_GATES,
        "a record that states no gate is a fourth state, not an unstated open one"
    );
    assert_eq!(
        open + closed + unstated + no_record,
        ledger.len(),
        "the four states must partition the corpus"
    );
}

#[test]
#[ignore = "reads the committed 98 MB snapshot; run with --ignored"]
fn the_runtime_reached_address_that_cannot_be_placed_yields_no_contract() {
    let _ = snapshot_or_skip!();
    let ledger = ledger_or_skip!();

    let resolution = ledger.snapshot().resolve(UNPLACED_RUNTIME_ADDRESS);
    assert_eq!(resolution.rule, Canonicalization::NonFunctionEntity);
    assert_eq!(resolution.canonical, None);
    assert_eq!(
        resolution.offset, None,
        "an address with no containing body has no offset: zero would be a fabricated answer"
    );
    assert!(
        !ledger.snapshot().contains_entry(UNPLACED_RUNTIME_ADDRESS),
        "and no entry sits there to be found"
    );

    let at = ledger.for_va(UNPLACED_RUNTIME_ADDRESS);
    assert!(!at.is_placed());
    assert!(
        at.is_empty() && at.contracts().is_empty(),
        "0x00925050 was reached through a real virtual call; the honest report is an empty \
         contract set with an unplaced resolution, not a manufactured passport"
    );
    assert_eq!(
        ledger.stance(UNPLACED_RUNTIME_ADDRESS),
        PassportStance::Unplaced
    );
    assert_eq!(
        ledger.for_va(UNPLACED_RUNTIME_ADDRESS).abi.reason(),
        "no_passport_record"
    );
}

#[test]
#[ignore = "reads the committed 98 MB snapshot; run with --ignored"]
fn the_renderer_frontier_addresses_are_placed_but_the_passport_is_silent_about_them() {
    let _ = snapshot_or_skip!();
    let ledger = ledger_or_skip!();

    for va in [0x011e_1d50u32, 0x011e_1ba0, 0x011f_9250, 0x007c_b590] {
        let at = ledger.for_va(va);
        assert!(at.is_placed(), "0x{va:08x} is inside a known body");
        assert_eq!(
            ledger.stance(va),
            PassportStance::ExplicitlySilent,
            "0x{va:08x} carries a record, and all five of the groups a rule could read are explicit \
             absences -- which is why every catalog entry lists it as unstated"
        );
        assert!(
            at.contracts().is_empty() && at.is_empty(),
            "0x{va:08x} is placed but every group is an explicit absence, so it yields no \
             contract at all: a non-finding is not a claim. This is the whole reason the catalog \
             lists these addresses as unstated."
        );
    }

    // The interior addresses the run named are inside those bodies, which is how
    // the ledger resolves them rather than losing them.
    for interior in [0x011e_1da0u32, 0x011e_1c41, 0x011f_92d5, 0x011f_945f] {
        let at = ledger.for_va(interior);
        assert_eq!(
            at.resolution.rule,
            Canonicalization::ContainingFunctionEntry,
            "0x{interior:08x} is inside a body"
        );
        assert!(at.resolution.offset.is_some_and(|offset| offset > 0));
    }
}

#[test]
#[ignore = "reads the committed 98 MB snapshot; run with --ignored"]
fn the_catalog_audit_reproduces_what_the_document_says_about_each_anchor() {
    let _ = snapshot_or_skip!();
    let ledger = ledger_or_skip!();
    let audit = ledger.audit(all_invariants());
    assert_eq!(audit.len(), all_invariants().len());

    let mut placed = 0usize;
    let mut unplaced = 0usize;
    for entry in &audit {
        for anchor in &entry.anchors {
            match anchor.rule {
                Canonicalization::NonFunctionEntity => {
                    unplaced += 1;
                    assert_eq!(anchor.stance, PassportStance::Unplaced);
                }
                _ => {
                    placed += 1;
                    assert_eq!(
                        anchor.stance,
                        PassportStance::ExplicitlySilent,
                        "{}: the passport holds a record for 0x{:08x} and states none of the five \
                         things a rule could read",
                        entry.id,
                        anchor.va
                    );
                }
            }
        }
        assert_eq!(
            entry.level,
            all_invariants()
                .iter()
                .find(|candidate| candidate.core().id == entry.id)
                .expect("an audited id is a real id")
                .core()
                .level
        );
    }
    assert_eq!(
        unplaced, 1,
        "exactly one anchored address is the one the universe cannot place: 0x00925050"
    );
    assert!(
        placed >= 6,
        "the renderer rules are anchored on addresses the passport places, so the audit has \
         something to say about them; found {placed}"
    );
}

#[test]
#[ignore = "reads the committed 98 MB snapshot; run with --ignored"]
fn the_import_table_thunk_is_the_only_graphics_device_record_and_the_triage_skips_it() {
    let _ = snapshot_or_skip!();
    let snapshot = snapshot().expect("checked by the macro");
    let at = ledger_or_skip!().for_va(0x0120_bb50);
    assert!(
        at.dispatch.is_none(),
        "a six-byte import thunk has no table membership"
    );
    let passport = snapshot
        .lookup(0x0120_bb50)
        .expect("the thunk is an indexed entry");
    assert!(passport.identity().is_thunk);
    assert_eq!(
        passport.classification().priority,
        "IGNORE",
        "triage skips the one place the snapshot sees the device surface, which is why the rule \
         about import analysis is stated rather than left to the priority column"
    );
    assert!(passport.graph().callees.is_empty());
    assert!(
        !passport.graph().callers.is_empty(),
        "and it does have a caller, so the edge exists"
    );
}

#[test]
#[ignore = "reads the committed 98 MB snapshot; run with --ignored"]
fn the_ordering_contract_accepts_the_sequence_it_was_derived_from() {
    let _ = snapshot_or_skip!();
    let contract = &all_lifecycle()[0];
    let observed = contract.phases;
    assert_eq!(
        contract.check_order(observed),
        Ok(()),
        "the contract must accept its own required order"
    );
    assert_eq!(
        contract.check_order(&observed[..3]),
        Ok(()),
        "a partial sequence is not a failure"
    );
}
