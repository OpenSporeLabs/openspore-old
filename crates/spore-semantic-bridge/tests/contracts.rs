//! The contract layer, hermetically: synthetic passports only.
//!
//! Nothing here reads the committed 98 MB artifact. Every fixture is written by
//! hand from the schema (see `support::RecordBuilder`), so the suite runs on a
//! machine with no snapshot, no game and no runtime producer. The cases that need
//! the real artifact live in `contracts_real_snapshot.rs`, where they are
//! `#[ignore]`d and skip when it is absent.
//!
//! # What is under test
//!
//! * the **projection** ([`ContractLedger`]): dispatch targets, ABI standing,
//!   name-derived lifecycle roles, runtime gates, and an address the universe
//!   cannot place;
//! * the **committed catalog** ([`crate::invariants`]): coherence, grade
//!   arithmetic, and the refusal to name a platform API;
//! * the **ordering contract**: what a `Startup` system would actually call.

mod support;

use spore_semantic_bridge::invariants::{self, all_lifecycle, all_resource};
use spore_semantic_bridge::{
    all_invariants, invariant, AbiStanding, Canonicalization, ContractLedger, ContractSubject,
    EngineComponent, EvidenceLevel, Fact, LifecycleContract, LifecyclePhase, LifecycleRoleKind,
    PassportStance, ProbedInput, ProvenanceMode, ResourceDecision, RuntimeGate, SemanticContract,
    SemanticInvariant, SourceClass,
};
use support::RecordBuilder;

/// The order the observed run walked, which is the order the contract requires.
const OBSERVED_ORDER: &[LifecyclePhase] = &[
    LifecyclePhase::EnvironmentProbe,
    LifecyclePhase::AdapterSelection,
    LifecyclePhase::DeviceCreation,
    LifecyclePhase::CapabilityProbe,
    LifecyclePhase::ResourceCreation,
];

/// The one lifecycle contract in the catalog.
fn lifecycle_contract() -> &'static LifecycleContract {
    &all_lifecycle()[0]
}

/// A five-record snapshot covering every projection the ledger performs.
fn ledger_fixture() -> support::Fixture {
    support::fixture(&[
        // 0x00401000: two table memberships, one inferred ABI record, an open gate.
        RecordBuilder::new("0x00401000", "0x00001000", 0x4a)
            .sdk("Graphics::Device::Initialize")
            .abi_known("__thiscall", "INFERRED")
            .vtable(&[(0x013f_05d8, 2, Some(9)), (0x013f_1a30, 3, Some(87))])
            .runtime_gate(Some(true))
            .build(),
        // 0x004010c0: an ABI record that declines to name a convention.
        RecordBuilder::new("0x004010c0", "0x000010c0", 0x20)
            .sdk("Graphics::Device::OnExit")
            .abi_undetermined("UNKNOWN")
            .build(),
        // 0x00401100: a name whose final segment the closed token set does not accept.
        RecordBuilder::new("0x00401100", "0x00001100", 0x10)
            .sdk("Graphics::Device::CreateResourceAsync")
            .classified("ENGINE_INTERFACE", "CONFIRMED")
            .build(),
        // 0x00401140: no name at all: every group is an explicit absence.
        RecordBuilder::new("0x00401140", "0x00001140", 0x10)
            .classified("UNKNOWN", "UNKNOWN")
            .subsystem(None)
            .build(),
        // 0x00401200: a closed gate, so "closed" is not confused with "absent".
        RecordBuilder::new("0x00401200", "0x00001200", 0x10)
            .runtime_gate(Some(false))
            .build(),
    ])
}

#[test]
fn a_table_membership_becomes_a_dispatch_contract_carrying_the_groups_own_grade() {
    let fixture = ledger_fixture();
    let ledger = ContractLedger::build(fixture.snapshot()).expect("the fixture must project");

    let at = ledger.for_va(0x0040_1000);
    let dispatch = at.dispatch.as_ref().expect("a membership must be kept");
    assert_eq!(dispatch.va, 0x0040_1000);
    assert_eq!(dispatch.memberships.len(), 2);
    assert_eq!(
        dispatch.level,
        EvidenceLevel::Inferred,
        "the vtable group's grade is copied, never re-graded"
    );
    assert!(
        !dispatch.provenance.is_empty(),
        "a dispatch contract must carry the source's provenance"
    );
    // The slot index is the fact; the displacement is the access.
    assert_eq!(dispatch.memberships[0].slot, 2);
    assert_eq!(dispatch.first_displacement(), Some(0x8));

    let contract = dispatch.to_contract();
    assert_eq!(contract.subject, ContractSubject::VirtualDispatch);
    assert!(contract.is_address_scoped());
    assert!(contract.statement.contains("0x00401000"));
    assert!(
        contract.statement.contains("displacement"),
        "the contract has to say how the slot is reached: {}",
        contract.statement
    );
    assert!(contract.verdict_unchanged());
    assert!(contract.to_fact().is_some());
}

#[test]
fn an_abi_record_that_declines_to_name_a_convention_is_not_an_absent_abi() {
    let fixture = ledger_fixture();
    let ledger = ContractLedger::build(fixture.snapshot()).expect("the fixture must project");

    let known = ledger.for_va(0x0040_1000);
    assert_eq!(
        known.abi.convention(),
        Some("__thiscall"),
        "a record naming a convention reports it"
    );
    assert!(known.abi.is_determined());

    let undetermined = ledger.for_va(0x0040_10c0);
    assert!(
        !undetermined.abi.is_determined(),
        "a record with a null convention determines nothing"
    );
    assert_eq!(
        undetermined.abi.reason(),
        spore_semantic_bridge::REASON_CONVENTION_UNDETERMINED
    );
    assert!(
        !undetermined.abi.provenance().is_empty(),
        "a record was read, so its provenance is carried: the two abstentions must not look alike"
    );

    let absent = ledger.for_va(0x0040_1140);
    assert!(matches!(absent.abi, AbiStanding::NotRecorded { .. }));
    assert!(absent.abi.provenance().is_empty());
    assert_ne!(
        absent.abi.reason(),
        undetermined.abi.reason(),
        "no record and a record that declines are different answers"
    );
}

#[test]
fn a_determined_convention_becomes_a_contract_and_an_abstention_becomes_none() {
    let fixture = ledger_fixture();
    let ledger = ContractLedger::build(fixture.snapshot()).expect("the fixture must project");

    let known = ledger.for_va(0x0040_1000);
    let abi = known
        .contracts()
        .into_iter()
        .find(|contract| contract.subject == ContractSubject::AbiStanding)
        .expect("a determined convention is a contract");
    assert_eq!(abi.level, EvidenceLevel::Inferred, "the record's own grade");
    assert!(abi.statement.contains("__thiscall"));

    for va in [0x0040_10c0, 0x0040_1140] {
        assert!(
            !ledger
                .for_va(va)
                .contracts()
                .iter()
                .any(|contract| contract.subject == ContractSubject::AbiStanding),
            "0x{va:08x} has no convention, so it has no ABI contract: a non-finding is not a \
             claim"
        );
    }
}

#[test]
fn a_lifecycle_role_is_read_only_from_the_closed_token_set() {
    let fixture = ledger_fixture();
    let ledger = ContractLedger::build(fixture.snapshot()).expect("the fixture must project");

    let setup = ledger.for_va(0x0040_1000);
    let role = setup
        .lifecycle
        .as_ref()
        .expect("the name states a setup step");
    assert_eq!(role.kind, LifecycleRoleKind::Setup);
    assert_eq!(role.token, "Initialize");
    assert_eq!(role.name, "Graphics::Device::Initialize");
    assert_eq!(role.role_level(), EvidenceLevel::Inferred);

    let teardown = ledger.for_va(0x0040_10c0);
    let role = teardown
        .lifecycle
        .as_ref()
        .expect("the name states a teardown step");
    assert_eq!(role.kind, LifecycleRoleKind::Teardown);
    assert_eq!(role.token, "OnExit");

    // A `CONFIRMED` name must not promote the role, and a name outside the token
    // set must not produce one at all.
    assert!(
        ledger.for_va(0x0040_1100).lifecycle.is_none(),
        "CreateResourceAsync is not in the closed token set; a prefix match would have claimed it"
    );
    assert!(
        ledger.for_va(0x0040_1140).lifecycle.is_none(),
        "no name, no role"
    );
    assert_eq!(ledger.lifecycle_record_count(), 2);
}

#[test]
fn a_confirmed_name_yields_an_inferred_role_never_a_confirmed_one() {
    let fixture = support::fixture(&[RecordBuilder::new("0x00401000", "0x00001000", 0x10)
        .sdk("Editors::cEditor::Dispose")
        .classified("GAMEPLAY_LOGIC", "CONFIRMED")
        .build()]);
    let ledger = ContractLedger::build(fixture.snapshot()).expect("the fixture must project");
    let at = ledger.for_va(0x0040_1000);
    let role = at.lifecycle.as_ref().expect("Dispose is in the token set");
    assert_eq!(role.name_level, EvidenceLevel::Confirmed);
    let contract = at
        .contracts()
        .into_iter()
        .find(|contract| contract.subject == ContractSubject::LifecycleRole)
        .expect("a role is a contract");
    assert_eq!(
        contract.level,
        EvidenceLevel::Inferred,
        "the name is CONFIRMED; the role read off it is not"
    );
    assert!(contract.statement.contains("inference"));
}

#[test]
fn a_runtime_gate_keeps_all_four_states_and_an_unstated_gate_is_not_a_contract() {
    let fixture = support::fixture(&[
        RecordBuilder::new("0x00401000", "0x00001000", 0x10)
            .runtime_gate(Some(true))
            .build(),
        RecordBuilder::new("0x00401010", "0x00001010", 0x10)
            .runtime_gate(Some(false))
            .build(),
        RecordBuilder::new("0x00401020", "0x00001020", 0x10)
            .runtime_gate(None)
            .build(),
        RecordBuilder::new("0x00401030", "0x00001030", 0x10).build(),
    ]);
    let ledger = ContractLedger::build(fixture.snapshot()).expect("the fixture must project");

    assert_eq!(
        ledger
            .for_va(0x0040_1000)
            .runtime_gate
            .as_ref()
            .map(|g| g.gate),
        Some(RuntimeGate::Open)
    );
    assert_eq!(
        ledger
            .for_va(0x0040_1010)
            .runtime_gate
            .as_ref()
            .map(|g| g.gate),
        Some(RuntimeGate::Closed)
    );
    assert_eq!(
        ledger
            .for_va(0x0040_1020)
            .runtime_gate
            .as_ref()
            .map(|g| g.gate),
        Some(RuntimeGate::Unstated),
        "a record that states no gate says so, which is not the same as having no record"
    );
    assert_eq!(
        ledger.for_va(0x0040_1030).runtime_gate,
        None,
        "no reconstruction record is not an unstated gate inside one"
    );

    assert_eq!(ledger.open_gate_count(), 1, "only the open gate counts");
    let open: Vec<_> = ledger.runtime_gated().collect();
    assert_eq!(open.len(), 1);
    let contract = open[0].to_contract().expect("an open gate is a contract");
    assert!(
        contract.statement.contains("nothing was attempted"),
        "an open gate is neither a pass nor a failure: {}",
        contract.statement
    );
    assert!(contract.verdict_unchanged());
    assert_eq!(contract.subject, ContractSubject::EvidenceDebt);
    assert!(
        ledger
            .for_va(0x0040_1020)
            .runtime_gate
            .as_ref()
            .and_then(|gate| gate.to_contract())
            .is_none(),
        "an unstated gate states nothing, so it is not a claim"
    );
    assert!(
        ledger.for_va(0x0040_1030).contracts().is_empty(),
        "a record with nothing available yields no contracts at all"
    );
}

#[test]
fn an_address_the_universe_cannot_place_yields_no_contract_and_no_passport() {
    // 0x0040104a..0x004010c0 is the gap between the first two bodies.
    let fixture = ledger_fixture();
    let ledger = ContractLedger::build(fixture.snapshot()).expect("the fixture must project");

    let unplaced = ledger.for_va(0x0040_1070);
    assert!(!unplaced.is_placed());
    assert_eq!(
        unplaced.resolution.rule,
        Canonicalization::NonFunctionEntity
    );
    assert_eq!(unplaced.resolution.canonical, None);
    assert_eq!(unplaced.resolution.offset, None);
    assert!(
        unplaced.is_empty() && unplaced.contracts().is_empty(),
        "nothing is manufactured for an address the universe cannot place"
    );
    assert_eq!(
        unplaced.abi.reason(),
        "no_passport_record",
        "and the ABI standing says so with a reason rather than a default"
    );
    assert_eq!(ledger.stance(0x0040_1070), PassportStance::Unplaced);
}

#[test]
fn an_interior_address_inherits_the_contracts_of_the_entry_containing_it() {
    let fixture = ledger_fixture();
    let ledger = ContractLedger::build(fixture.snapshot()).expect("the fixture must project");

    let interior = ledger.for_va(0x0040_1010);
    assert!(interior.is_placed());
    assert_eq!(
        interior.resolution.rule,
        Canonicalization::ContainingFunctionEntry
    );
    assert_eq!(interior.resolution.canonical, Some(0x0040_1000));
    assert_eq!(interior.resolution.offset, Some(0x10));
    assert!(
        interior.dispatch.is_some(),
        "an interior address is the same function, so it carries the same dispatch contract"
    );
    let contract = interior
        .contracts()
        .into_iter()
        .find(|contract| contract.subject == ContractSubject::VirtualDispatch)
        .expect("the dispatch contract is keyed on the entry, not the interior address");
    assert_eq!(
        contract.va,
        Some(0x0040_1000),
        "a contract is about the function; the resolution carries the offset"
    );
}

#[test]
fn the_ledger_projects_exactly_one_row_per_record() {
    let fixture = ledger_fixture();
    let snapshot = fixture.snapshot();
    let ledger = ContractLedger::build(snapshot).expect("the fixture must project");
    assert_eq!(ledger.len(), 5);
    assert!(!ledger.is_empty());
    assert_eq!(ledger.snapshot().function_count(), 5);
    assert_eq!(ledger.dispatch_record_count(), 1);
    assert_eq!(ledger.lifecycle_record_count(), 2);
    assert_eq!(ledger.open_gate_count(), 1);
    assert_eq!(ledger.dispatch_contracts().count(), 1);
    assert_eq!(ledger.lifecycle_roles().count(), 2);
    assert_eq!(ledger.known_conventions().count(), 1);
    assert!(format!("{ledger:?}").contains("ContractLedger"));
}

// ---------------------------------------------------------------------------
// The committed catalog
// ---------------------------------------------------------------------------

#[test]
fn every_catalog_entry_is_coherent_and_sourced() {
    let mut seen = std::collections::BTreeSet::new();
    for entry in all_invariants() {
        let core = entry.core();
        assert!(seen.insert(core.id), "{} is declared twice", core.id);
        assert_eq!(entry.coherence(), Ok(()), "{} is not coherent", core.id);
        assert!(
            core.provenance
                .iter()
                .any(|p| p.source_class == SourceClass::CommittedArtifact),
            "{} must cite the committed document",
            core.id
        );
        assert!(
            !core.consumers.is_empty(),
            "a contract with no consumer is a note, not infrastructure"
        );
    }
    assert_eq!(
        seen.len(),
        12,
        "the catalog changed size; update this and the document"
    );
}

#[test]
fn no_entry_generalises_past_its_observation_and_none_claims_to_be_proven() {
    for entry in all_invariants() {
        let core = entry.core();
        assert!(
            core.level <= core.basis,
            "{} states {} from an observation graded {}",
            core.id,
            core.level,
            core.basis
        );
        assert!(
            core.level < EvidenceLevel::Confirmed,
            "{} claims CONFIRMED or better; nothing in this catalog came from a repeatable \
             differential, which is what those rungs mean",
            core.id
        );
        assert!(core.verdict_unchanged());
        assert!(entry.as_contract().verdict_unchanged());
    }
}

#[test]
fn every_entry_states_what_the_passport_does_not_say() {
    for entry in all_invariants() {
        assert!(
            !entry.core().not_stated.is_empty(),
            "{} must record its boundary, not only its claim",
            entry.core().id
        );
    }
    let unknown = invariant("display_state_meaning_unknown").expect("the known unknown exists");
    assert_eq!(
        unknown.core().level,
        EvidenceLevel::Unknown,
        "an UNKNOWN is never upgraded because it is inconvenient"
    );
    assert!(unknown.core().runtime.is_some());
}

#[test]
fn a_runtime_backed_entry_names_a_portable_artifact_in_its_provenance() {
    let mut runtime_backed = 0usize;
    for entry in all_invariants() {
        let core = entry.core();
        let Some(basis) = core.runtime else {
            continue;
        };
        runtime_backed += 1;
        assert!(
            basis.is_portable(),
            "{} cites a non-portable artifact",
            core.id
        );
        assert!(
            core.provenance
                .iter()
                .any(|p| p.source_class == SourceClass::Runtime),
            "{} admits a runtime observation without saying so in its provenance",
            core.id
        );
    }
    assert!(
        runtime_backed >= 8,
        "the catalog is mostly runtime-backed; found {runtime_backed}"
    );
    for entry in all_invariants() {
        let core = entry.core();
        let says_runtime = core
            .provenance
            .iter()
            .any(|p| p.source_class == SourceClass::Runtime);
        assert_eq!(
            says_runtime,
            core.runtime.is_some(),
            "{} disagrees with itself about whether it is runtime-backed",
            core.id
        );
    }
}

#[test]
fn no_entry_cites_an_absolute_path_or_a_program_image() {
    for entry in all_invariants() {
        for provenance in &entry.core().provenance {
            assert_ne!(provenance.reference, "");
            assert!(
                !provenance.reference.starts_with('/') && !provenance.reference.contains('\\'),
                "{} cites a host-specific path: {}",
                entry.core().id,
                provenance.reference
            );
            assert!(
                !provenance.reference.contains("SporeApp.exe"),
                "{} carries a program image as a source",
                entry.core().id
            );
        }
    }
    assert!(
        all_invariants().iter().all(|entry| entry
            .core()
            .provenance
            .iter()
            .any(|p| p.reference.starts_with("docs/SEMANTIC-CONTRACTS.md"))),
        "every entry must cite the document that writes it out"
    );
}

#[test]
fn a_runtime_entry_is_imported_from_the_producer_not_derived_here() {
    let catalog = all_invariants();
    let with_runtime = catalog
        .iter()
        .find(|entry| entry.core().runtime.is_some())
        .expect("some entry is runtime-backed");
    let runtime = with_runtime.core().runtime.expect("checked above");
    assert!(
        runtime.artifact.starts_with("work/reports/"),
        "the artifact reference is the producer's portable id: {}",
        runtime.artifact
    );
    let imported = with_runtime
        .core()
        .provenance
        .iter()
        .find(|p| p.source_class == SourceClass::Runtime)
        .expect("a runtime source class is present");
    assert_eq!(imported.reference, runtime.artifact);
    assert_eq!(imported.mode, ProvenanceMode::Imported);
    assert!(
        !matches!(
            imported.source_class,
            SourceClass::Ghidra | SourceClass::GeneratedIndex
        ),
        "a runtime observation is not a static analysis product"
    );
}

#[test]
fn every_engine_component_has_a_consumer_so_none_is_a_dead_branch() {
    let used: std::collections::BTreeSet<EngineComponent> = all_invariants()
        .iter()
        .flat_map(|entry| entry.core().consumers.iter().copied())
        .collect();
    for component in [
        EngineComponent::RendererStartup,
        EngineComponent::AssetResolution,
        EngineComponent::DecodePolicy,
        EngineComponent::EvidencePipeline,
        EngineComponent::DispatchTable,
    ] {
        assert!(
            used.contains(&component),
            "{component} is declared as a consumer but nothing consumes it: a component with no \
             contract is a second source of truth waiting to drift"
        );
        assert!(!component.path_hint().is_empty());
        assert!(!component.as_str().is_empty());
    }
}

// ---------------------------------------------------------------------------
// The ordering contract, which is the one a Startup system would call
// ---------------------------------------------------------------------------

#[test]
fn the_observed_order_satisfies_the_lifecycle_contract() {
    let contract = lifecycle_contract();
    assert_eq!(
        contract.check_order(OBSERVED_ORDER),
        Ok(()),
        "the order the run walked must satisfy the contract derived from it"
    );
}

#[test]
fn probing_after_resource_creation_is_reported_as_a_violation() {
    let contract = lifecycle_contract();
    let wrong = [
        LifecyclePhase::EnvironmentProbe,
        LifecyclePhase::AdapterSelection,
        LifecyclePhase::DeviceCreation,
        LifecyclePhase::ResourceCreation,
        LifecyclePhase::CapabilityProbe,
    ];
    let violation = contract
        .check_order(&wrong)
        .expect_err("creating resources before the capability probe must be reported");
    assert_eq!(violation.early, LifecyclePhase::CapabilityProbe);
    assert_eq!(violation.late, LifecyclePhase::ResourceCreation);
    assert_eq!(violation.observed_at, 4);
    assert!(violation.to_string().contains("required to follow"));
}

#[test]
fn a_partial_sequence_is_neither_a_pass_nor_a_failure() {
    let contract = lifecycle_contract();
    // Only the first two phases happened: the run may simply not have got there.
    assert_eq!(
        contract.check_order(&[
            LifecyclePhase::EnvironmentProbe,
            LifecyclePhase::AdapterSelection
        ]),
        Ok(())
    );
    // A phase the contract does not name carries no rank and is not scored.
    assert_eq!(
        contract.check_order(&[
            LifecyclePhase::DeviceCreation,
            LifecyclePhase::ResourceCreation
        ]),
        Ok(())
    );
    // An empty sequence is not a claim that startup happened.
    assert_eq!(contract.check_order(&[]), Ok(()));
}

#[test]
fn the_contract_chain_starts_at_a_probe_and_ends_at_a_terminal_phase() {
    let contract = lifecycle_contract();
    assert_eq!(
        contract.phases.first(),
        Some(&LifecyclePhase::EnvironmentProbe)
    );
    assert_eq!(contract.phases.last(), Some(&LifecyclePhase::Teardown));
    assert!(contract.phases.last().is_some_and(|p| p.is_terminal()));
    assert!(!contract.phases.first().is_some_and(|p| p.is_terminal()));
}

#[test]
fn a_resource_contract_names_the_probed_input_and_the_decision_it_constrains() {
    let contracts = all_resource();
    assert!(!contracts.is_empty());
    for contract in contracts {
        assert_eq!(contract.coherence(), Ok(()));
        assert!(
            contract
                .core()
                .statement
                .contains(&contract.probed.to_string()),
            "{} must name the input it depends on",
            contract.core().id
        );
        let as_contract = contract.as_contract();
        assert_eq!(as_contract.subject, ContractSubject::ResourceCreation);
        assert!(as_contract.verdict_unchanged());
        // Both vocabulary members are reachable from a real contract, so neither
        // is a speculative variant.
        assert!(matches!(
            contract.probed,
            ProbedInput::DeviceMemoryBudget | ProbedInput::DeviceCapabilities
        ));
        assert!(matches!(
            contract.decision,
            ResourceDecision::GpuAllocationBudget | ResourceDecision::ResourceFormatChoice
        ));
    }
    let mut probed: Vec<ProbedInput> = contracts.iter().map(|c| c.probed).collect();
    probed.sort_unstable();
    probed.dedup();
    assert_eq!(probed.len(), 2, "both probed inputs should be constrained");
}

// ---------------------------------------------------------------------------
// The passport's silence, recomputed
// ---------------------------------------------------------------------------

#[test]
fn the_audit_distinguishes_silence_from_absence_from_an_answer() {
    let fixture = ledger_fixture();
    let ledger = ContractLedger::build(fixture.snapshot()).expect("the fixture must project");

    assert_eq!(
        ledger.stance(0x0040_1000),
        PassportStance::States,
        "a record with a name, an ABI record, a membership and a gate states plenty"
    );
    assert_eq!(
        ledger.stance(0x0040_1140),
        PassportStance::ExplicitlySilent,
        "a record exists and every group is an explicit absence"
    );
    assert_eq!(
        ledger.stance(0x0040_1070),
        PassportStance::Unplaced,
        "the gap between two bodies places nothing"
    );
    assert_eq!(
        ledger.stance(0x0040_1010),
        PassportStance::States,
        "an interior address inherits the stance of the record containing it"
    );
}

#[test]
fn auditing_the_catalog_reports_a_stance_for_every_declared_anchor() {
    let fixture = ledger_fixture();
    let ledger = ContractLedger::build(fixture.snapshot()).expect("the fixture must project");
    let audit = ledger.audit(all_invariants());
    assert_eq!(audit.len(), all_invariants().len());
    for entry in &audit {
        let declared = invariant(entry.id).expect("an audited id is a real id");
        match declared.core().anchors {
            None => assert!(
                entry.anchors.is_empty(),
                "{} claims no anchor, so none may be audited",
                entry.id
            ),
            Some(anchors) => assert_eq!(
                entry.anchors.len(),
                anchors.len(),
                "{} declares {} anchors and {} were audited",
                entry.id,
                anchors.len(),
                entry.anchors.len()
            ),
        }
        for anchor in &entry.anchors {
            assert_eq!(
                anchor.rule,
                Canonicalization::NonFunctionEntity,
                "this fixture places none of the catalog's anchors; another answer means the fixture \
                 stopped resembling the corpus, not that the corpus changed"
            );
            assert_eq!(anchor.canonical, None);
            assert_eq!(anchor.stance, PassportStance::Unplaced);
        }
        assert_eq!(entry.level, declared.core().level);
    }
}

// ---------------------------------------------------------------------------
// The envelope rules, checked directly
// ---------------------------------------------------------------------------

#[test]
fn a_contract_copied_from_a_fact_can_never_outrank_the_fact() {
    let fact = Fact::persisted(EvidenceLevel::Supported, "some/pack.json", 7u32);
    let contract = SemanticContract::from_fact(
        "id".to_owned(),
        ContractSubject::AbiStanding,
        Some(0x0040_1000),
        "statement".to_owned(),
        &fact,
        &[EngineComponent::DispatchTable],
    );
    assert_eq!(contract.level, fact.level());
    assert_eq!(contract.provenance, fact.provenance().to_vec());
    assert!(contract.to_fact().is_some());
}

#[test]
fn a_contract_with_no_provenance_yields_no_fact_rather_than_an_unknown_one() {
    let mut contract = SemanticContract::from_fact(
        "id".to_owned(),
        ContractSubject::AbiStanding,
        None,
        "statement".to_owned(),
        &Fact::persisted(EvidenceLevel::Inferred, "some/pack.json", 7u32),
        &[EngineComponent::DispatchTable],
    );
    contract.provenance.clear();
    assert!(
        contract.to_fact().is_none(),
        "'no source' is not the same statement as 'the claim is unknown'"
    );
    assert!(!contract.is_address_scoped());
    assert!(contract.describe().contains("[id/INFERRED]"));
}

// ---------------------------------------------------------------------------
// The document and the catalog cannot drift apart
// ---------------------------------------------------------------------------

/// The document every invariant's provenance points at.
fn document() -> Option<String> {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/SEMANTIC-CONTRACTS.md")
        .canonicalize()
        .ok()?;
    std::fs::read_to_string(path).ok()
}

#[test]
fn the_document_writes_out_every_catalog_entry_with_its_grade() {
    let Some(document) = document() else {
        eprintln!("SKIP: docs/SEMANTIC-CONTRACTS.md is not reachable from the crate directory");
        return;
    };
    for entry in all_invariants() {
        let core = entry.core();
        assert!(
            document.contains(&format!("**`{}`**", core.id)),
            "{} is in the catalog but not written out in docs/SEMANTIC-CONTRACTS.md",
            core.id
        );
    }
    // And the other way: the document must not describe an entry that was removed.
    for id in invariants::ids() {
        assert_eq!(
            document.matches(&format!("**`{id}`**")).count(),
            1,
            "{id} is written out {} times; the document and the catalog disagree",
            document.matches(&format!("**`{id}`**")).count()
        );
    }
}

#[test]
fn the_document_states_the_same_headline_counts_the_suite_pins() {
    let Some(document) = document() else {
        eprintln!("SKIP: docs/SEMANTIC-CONTRACTS.md is not reachable from the crate directory");
        return;
    };
    let fixture = ledger_fixture();
    let ledger = ContractLedger::build(fixture.snapshot()).expect("the fixture must project");
    // The fixture is synthetic, so only the catalog's own counts are comparable
    // here; the artifact's counts are pinned in contracts_real_snapshot.rs.
    assert_eq!(all_invariants().len(), invariants::ids().len());
    assert!(document.contains(&format!("| catalog entries | {} ", all_invariants().len())));
    assert!(
        document.contains("`UNKNOWN` on both rungs"),
        "the known unknown must be visible in the document, not only in the code"
    );
    assert!(document.contains(&format!("{}", ledger.len())));
}
