//! The committed invariant catalog: engine-level rules, each with its grade.
//!
//! # What is in here and what is not
//!
//! Every entry is a **rule an engine author can act on**, distilled from evidence
//! and graded. None of them describes a Windows API, a graphics interface or any
//! other piece of the original program, because OpenSpore implements none of
//! those; [`crate::FORBIDDEN_IN_STATEMENTS`] is the machine-checked form of that
//! refusal, and [`crate::SemanticInvariant::coherence`] applies it to every entry.
//!
//! # Where the two grades come from
//!
//! A runtime run of the original binary reached a renderer that a static snapshot
//! says nothing about. That produced observations, and observations are not
//! engine requirements: turning "an adapter handle was opened" into "adapter
//! selection is a distinct step" is an inference. So every entry carries both:
//!
//! | | |
//! |---|---|
//! | `basis` | how well the **observation** is supported |
//! | `level` | how well **the engine-level rule** is supported |
//!
//! Nothing in this catalog is `CONFIRMED` or `VERIFIED`, and that is a statement
//! rather than an omission: no repeatable differential was run over the original
//! binary for any of these rules, and `VERIFIED` means exactly that. Eleven entries
//! are `OBSERVED` observations generalised to `INFERRED` rules; one is `UNKNOWN`
//! on both rungs, recorded precisely so that "we did not look" is in the catalog
//! instead of being indistinguishable from an oversight. Ten of the twelve are
//! backed by a runtime observation; the other two are counted out of the artifact
//! itself.
//!
//! # What the passport does not say
//!
//! Each entry's `not_stated` list is the honest boundary of the source it came
//! from, and it is **checked**, not asserted: [`crate::ContractLedger::audit`] recomputes
//! the passport's stance on every address-anchored entry, and
//! `tests/contracts_real_snapshot.rs` asserts the result against the committed
//! artifact. On the current snapshot every anchor of every rule is
//! `explicitly_silent` — a record exists, and all five of the groups a rule could
//! read are explicit absences — except the one anchor the universe cannot place
//! at all.
//!
//! # Consumers
//!
//! Every entry names at least one [`crate::EngineComponent`], which is what keeps this
//! from being a notes file. **None is wired yet**: this crate is Bevy-free and
//! depends on nothing but `spore-core`, so it can name the place an assertion
//! belongs without being able to install it. `docs/SEMANTIC-CONTRACTS.md` says
//! per entry what the assertion would look like.

use crate::contracts::{
    BehavioralInvariant, ContractSubject, EngineComponent, EvidenceAnchor, InvariantCore,
    LifecycleContract, LifecyclePhase, ProbedInput, ResourceContract, ResourceDecision,
    RuntimeBasis, SemanticInvariant,
};
use spore_core::EvidenceLevel;

/// Where every entry is written out in full.
pub const DOCS_REFERENCE: &str = "docs/SEMANTIC-CONTRACTS.md";

/// The producer's report that first walked the display-adapter path.
pub const ARTIFACT_ADAPTER_OPEN: &str = "work/reports/startup-recovery84.json";

/// The producer's report that reached the main loop.
pub const ARTIFACT_MAIN_LOOP: &str = "work/reports/startup-recovery88.json";

/// The runtime producer this catalog's runtime half is measured against.
pub const RUNTIME_PRODUCER: &str = "spore-recomp";

/// Builds the envelope every entry shares.
///
/// The argument list is the envelope field for field, in declaration order. That
/// is deliberate: an entry that skipped `not_stated` or `consumers` would not
/// compile, which is the cheapest way to keep "a rule must state its boundary and
/// name its consumer" true of every entry added next year.
#[allow(clippy::too_many_arguments)]
fn core(
    id: &'static str,
    subject: ContractSubject,
    statement: &'static str,
    basis: EvidenceLevel,
    level: EvidenceLevel,
    artifact: Option<&'static str>,
    runtime: Option<RuntimeBasis>,
    consumers: &'static [EngineComponent],
    anchors: Option<&'static [EvidenceAnchor]>,
    not_stated: &'static [&'static str],
) -> InvariantCore {
    InvariantCore {
        id,
        subject,
        statement,
        basis,
        level,
        provenance: InvariantCore::provenance(artifact),
        consumers,
        anchors,
        not_stated,
        runtime,
    }
}

/// An observation restated in engine terms, at `Observed`.
fn observed(what: &'static str, artifact: &'static str) -> RuntimeBasis {
    RuntimeBasis {
        observation: what,
        artifact,
        level: EvidenceLevel::Observed,
    }
}

/// Every behavioural invariant, in declaration order.
pub fn all_behavioral() -> &'static [BehavioralInvariant] {
    static CATALOG: std::sync::OnceLock<Vec<BehavioralInvariant>> = std::sync::OnceLock::new();
    CATALOG.get_or_init(|| {
        vec![
            BehavioralInvariant {
                core: core(
                    "adapter_selection_is_a_distinct_step",
                    ContractSubject::AdapterSelection,
                    "Choosing a display target is a step of renderer setup with its own failure \
                     modes, and it is not part of creating a device. A runtime run reached an \
                     address that enumerated the available display devices, built a context from \
                     one of the device names it enumerated, and opened an adapter handle from that \
                     context -- all before any graphics device existed. OpenSpore must keep 'choose \
                     a target' and 'create a device' as separate steps, because a failure in the \
                     first is not a failure in the second and collapsing them reports the wrong \
                     one.",
                    EvidenceLevel::Observed,
                    EvidenceLevel::Inferred,
                    Some(ARTIFACT_ADAPTER_OPEN),
                    Some(observed(
                        "renderer startup enumerated the display devices and opened an adapter \
                         handle from one of them before any graphics device was created",
                        ARTIFACT_ADAPTER_OPEN,
                    )),
                    &[EngineComponent::RendererStartup],
                    Some(&[
                        EvidenceAnchor {
                            va: 0x011e_1d50,
                            note: "entered from renderer setup; resolved three library entry \
                                   points by name and opened an adapter handle through one of them",
                        },
                        EvidenceAnchor {
                            va: 0x011e_1ba0,
                            note: "entered from the same setup path; enumerated the display \
                                   devices and opened an adapter from a name it had enumerated",
                        },
                    ]),
                    &[
                        "the passport states no adapter, display device, monitor or handle at \
                         either address, and it has no field in which one could appear",
                        "neither address carries an SDK name, an ABI record, a table membership, a \
                         semantic record or a reconstruction record: all five are explicit absences",
                        "the subsystem both records carry is the triage classifier's own 'Runtime', \
                         graded INFERRED and APPROXIMATION respectively, which describes a triage \
                         row and not a renderer role",
                    ],
                ),
            },
            BehavioralInvariant {
                core: core(
                    "capability_probe_is_advisory",
                    ContractSubject::CapabilityProbing,
                    "A capability probe is advisory: the answer can be discarded and the renderer \
                     carries on. In the observed run one device query's answer was written into \
                     renderer state and the status of the next was thrown away, so a failed or \
                     absent capability answer is a degradation rather than a failure. An OpenSpore \
                     startup path must therefore be able to continue without a capability answer, \
                     and must not make device capability a precondition for standing a scene up.",
                    EvidenceLevel::Observed,
                    EvidenceLevel::Inferred,
                    Some(ARTIFACT_MAIN_LOOP),
                    Some(observed(
                        "a device capability query wrote into renderer state while a second query's \
                         status value was discarded and the renderer continued",
                        ARTIFACT_MAIN_LOOP,
                    )),
                    &[EngineComponent::RendererStartup, EngineComponent::DecodePolicy],
                    Some(&[EvidenceAnchor {
                        va: 0x011f_9250,
                        note: "queried the device twice from renderer setup, writing one answer \
                               into renderer state and discarding the status of the other",
                    }]),
                    &[
                        "the passport records no capability value, no status value and no error \
                         handling for any of it",
                        "the address's own record carries a category of UNKNOWN and a subsystem \
                         sentinel, so nothing in the file routes a reader to it",
                    ],
                ),
            },
            BehavioralInvariant {
                core: core(
                    "module_identity_can_be_ambiguous",
                    ContractSubject::ModuleIdentity,
                    "Identity resolved at run time can be ambiguous, and a wrong identity is worse \
                     than an unresolved one. In the producing tool a module-handle allocation \
                     collided with a hard-coded handle, so one system library answered under another \
                     library's name and every entry point reached through it resolved to nothing \
                     -- with no fault, no crash and no counter moving. Any OpenSpore lookup that \
                     maps a runtime token onto a module, a package or a record must be able to \
                     answer 'unresolved', and must never continue on a guessed identity.",
                    EvidenceLevel::Observed,
                    EvidenceLevel::Inferred,
                    Some(ARTIFACT_MAIN_LOOP),
                    Some(observed(
                        "a module-handle allocation collided with a hard-coded handle, so one \
                         library answered under another's name and every entry point reached \
                         through it resolved to nothing without a fault",
                        ARTIFACT_MAIN_LOOP,
                    )),
                    &[EngineComponent::AssetResolution],
                    None,
                    &[
                        "this is a defect in the evidence producer, not a property of the analysed \
                         binary, and the passport describes the binary only",
                        "the passport therefore cannot corroborate or refute it; what it does hold \
                         is the rule's own subject -- that identity resolution can answer wrongly \
                         -- nowhere in any of its 58,757 records",
                    ],
                ),
            },
            BehavioralInvariant {
                core: core(
                    "dynamic_resolution_is_invisible_to_import_analysis",
                    ContractSubject::ModuleIdentity,
                    "A dependency set derived from a static import table is incomplete by \
                     construction. Measured over the committed snapshot's 58,757 records: the \
                     renderer reached three library entry points by resolving their names at run \
                     time, and none of the three appears anywhere in the file -- not as a name, not \
                     as an external callee -- while the one entry point the same code reaches \
                     through the import table, a six-byte thunk at 0x0120bb50, is present with its \
                     caller recorded. Static analysis sees the imported sibling and is blind to the \
                     resolved trio, so no completeness claim about a program's dependencies may \
                     rest on import analysis alone.",
                    EvidenceLevel::Observed,
                    EvidenceLevel::Inferred,
                    Some(ARTIFACT_MAIN_LOOP),
                    Some(observed(
                        "three library entry points were resolved by name at run time, and the one \
                         reached through the import table was a separate, statically visible entry",
                        ARTIFACT_MAIN_LOOP,
                    )),
                    &[EngineComponent::AssetResolution, EngineComponent::EvidencePipeline],
                    Some(&[
                        EvidenceAnchor {
                            va: 0x011e_1d50,
                            note: "resolved three library entry points by name at run time; none of \
                                   the three appears anywhere in the snapshot",
                        },
                        EvidenceAnchor {
                            va: 0x0120_bb50,
                            note: "a six-byte import thunk for the device-creation entry point, the \
                                   snapshot's only record of the graphics-device surface, graded \
                                   IGNORE by the triage classifier",
                        },
                    ]),
                    &[
                        "the snapshot's external-callee vocabulary stops at the import table, so a \
                         name resolved at run time cannot appear in it by construction",
                        "the single thunk record is graded IGNORE, so a triage pass that honours the \
                         priority would skip the one place the file does see the device surface",
                        "the 25,088 table memberships in the file are about the program's own \
                         classes; none of them is a membership in a platform interface",
                    ],
                ),
            },
            BehavioralInvariant {
                core: core(
                    "a_missing_dependency_is_a_distinct_outcome",
                    ContractSubject::ModuleIdentity,
                    "A missing optional dependency is a distinct outcome, not a degradation. Every \
                     name the observed setup resolved was checked for null by its caller, so an \
                     absent one selects a different path rather than falling back to a default. A \
                     resolver that substitutes a default for an unresolved reference has collapsed \
                     two outcomes into one, and the collapse is invisible in whatever it returns.",
                    EvidenceLevel::Observed,
                    EvidenceLevel::Inferred,
                    Some(ARTIFACT_ADAPTER_OPEN),
                    Some(observed(
                        "every run-time-resolved name was null-checked by its caller, so an absent \
                         one selected a different path instead of a default",
                        ARTIFACT_ADAPTER_OPEN,
                    )),
                    &[EngineComponent::AssetResolution],
                    Some(&[EvidenceAnchor {
                        va: 0x011e_1d50,
                        note: "checked each name it had resolved and printed a distinct message \
                               when one was missing",
                    }]),
                    &[
                        "the passport records no optional-dependency resolution at all: it has no \
                         field for a name resolved at run time, only for one imported at link time",
                        "its graph block distinguishes a canonical callee from an external callee \
                         token, and an unresolved dependency is neither",
                    ],
                ),
            },
            BehavioralInvariant {
                core: core(
                    "dispatch_targets_are_table_and_slot_addressed",
                    ContractSubject::VirtualDispatch,
                    "A virtual call target is addressed by a (table, slot) pair, and the slot index \
                     is the fact while the byte displacement is only the access. 6,712 of the \
                     snapshot's records carry at least one table membership -- 25,088 memberships \
                     across 1,851 distinct tables, the widest at slot 281 -- and the sound scan \
                     that produces them is the only basis the ABI engine accepts. A dispatch table \
                     must therefore key on the slot index and reach the target by displacement from \
                     the table base; subscripting a table structure advances by a whole interface \
                     pair, lands on a different slot, and on this repository's own history produced \
                     a callable pointer and so a plausible wrong answer rather than a crash.",
                    EvidenceLevel::Observed,
                    EvidenceLevel::Inferred,
                    None,
                    None,
                    &[EngineComponent::DispatchTable],
                    None,
                    &[
                        "the passport says nothing about what any slot means: a membership names a \
                         table and an index and no method, so the engine cannot learn an \
                         interface's shape from it",
                        "the triage block records table addresses with no slot at all on some \
                         records, which is a weaker and differently-sourced association and must \
                         not be read as a membership",
                    ],
                ),
            },
            BehavioralInvariant {
                core: core(
                    "an_unplaced_runtime_address_is_data",
                    ContractSubject::StaticIdentity,
                    "An address reached at run time that the function universe cannot place is \
                     data, not a defect to repair. 0x00925050 was reached through a real virtual \
                     call and resolves to no entry, so it is reported as a static non-function \
                     entity with no canonical address; snapping it to the nearest function, or \
                     modelling it as an empty one, would destroy the only evidence the repository \
                     has that its universe is incomplete.",
                    EvidenceLevel::Observed,
                    EvidenceLevel::Inferred,
                    Some(ARTIFACT_MAIN_LOOP),
                    Some(observed(
                        "an address between two function bodies was reached through a real virtual \
                         call, and no entry covers it",
                        ARTIFACT_MAIN_LOOP,
                    )),
                    &[
                        EngineComponent::DispatchTable,
                        EngineComponent::EvidencePipeline,
                    ],
                    Some(&[EvidenceAnchor {
                        va: 0x0092_5050,
                        note: "reached through a virtual call the producer resolved; the function \
                               universe places no entry at or around it",
                    }]),
                    &[
                        "the passport holds no record for this address at all -- not an empty one, \
                         none -- so nothing about it can be graded",
                        "the two neighbouring entries are 0x00925000, whose body ends at 0x0092504a, \
                         and 0x009250c0, which is why the canonical-identity rule answers \
                         non_function_entity rather than picking one",
                    ],
                ),
            },
            BehavioralInvariant {
                core: core(
                    "runtime_gated_is_a_fourth_state",
                    ContractSubject::EvidenceDebt,
                    "'Owes runtime evidence' is a fourth state, not a pending pass and not a \
                     failure. 727 records carry a reconstruction record; 423 state an open runtime \
                     gate, 191 a closed one and 113 state nothing about it at all. A pipeline that \
                     renders those three as one boolean loses the difference between 'the gate is \
                     open, nothing was attempted, nothing failed' and 'this record says nothing', \
                     which is the difference between work to do and no answer at all.",
                    EvidenceLevel::Observed,
                    EvidenceLevel::Inferred,
                    None,
                    None,
                    &[EngineComponent::EvidencePipeline],
                    None,
                    &[
                        "the passport does not say which runtime observation would settle a gated \
                         function, only that one is owed",
                        "it does not record who is expected to produce it, so the rule cannot be \
                         turned into a work item from the file alone",
                    ],
                ),
            },
            BehavioralInvariant {
                core: core(
                    "display_state_meaning_unknown",
                    ContractSubject::AdapterSelection,
                    "What an enumerated display device's state fields mean is NOT established, \
                     and neither is whether a chosen adapter is ever re-selected once a device \
                     exists. The run reached the enumeration and the adapter open and then \
                     continued past device creation, so no observation bears on re-selection or on \
                     the meaning of those values. This is recorded at UNKNOWN on both rungs rather \
                     than left out, because 'we did not look' is the useful statement here and an \
                     absent clause would be indistinguishable from an oversight.",
                    EvidenceLevel::Unknown,
                    EvidenceLevel::Unknown,
                    Some(ARTIFACT_MAIN_LOOP),
                    Some(RuntimeBasis {
                        observation: "the run passed the enumeration and the adapter open and \
                                      continued, so nothing about re-selection or state semantics \
                                      was observed",
                        artifact: ARTIFACT_MAIN_LOOP,
                        level: EvidenceLevel::Unknown,
                    }),
                    &[EngineComponent::RendererStartup],
                    Some(&[EvidenceAnchor {
                        va: 0x011e_1ba0,
                        note: "entered during renderer setup; the values it read back from the \
                               enumeration were consumed without being interpreted",
                    }]),
                    &[
                        "the passport has no field for a display device, a state flag or a \
                         selection history, so this cannot be recovered from it at any grade",
                    ],
                ),
            },
        ]
    })
}

/// Every lifecycle-ordering contract.
pub fn all_lifecycle() -> &'static [LifecycleContract] {
    static CATALOG: std::sync::OnceLock<Vec<LifecycleContract>> = std::sync::OnceLock::new();
    CATALOG.get_or_init(|| vec![LifecycleContract {
        core: core(
            "capability_probe_precedes_resource_creation",
            ContractSubject::RendererStartup,
            "Renderer startup is ordered: find out what the environment offers, choose and open an \
             adapter, create the device, ask the device what it can do and how much it has, and \
             only then create resources and the renderer state those answers shape. A runtime run \
             walked exactly that order. For OpenSpore the consequence is a dependency rather than a \
             sequence to imitate: a resource whose size or format depends on a device answer cannot \
             be built by a step that runs before the probe.",
            EvidenceLevel::Observed,
            EvidenceLevel::Inferred,
            Some(ARTIFACT_MAIN_LOOP),
            Some(observed(
                "an adapter was opened, a device created, the device queried for capabilities and \
                 an available-memory figure, and only then was renderer state sized from the \
                 answers",
                ARTIFACT_MAIN_LOOP,
            )),
            &[EngineComponent::RendererStartup, EngineComponent::DecodePolicy],
            Some(&[
                EvidenceAnchor {
                    va: 0x011f_9250,
                    note: "queried the device's capabilities from renderer setup, after a device \
                           existed",
                },
                EvidenceAnchor {
                    va: 0x007c_b590,
                    note: "consumed a probed available-memory figure and sized renderer state from \
                           it",
                },
            ]),
            &[
                "the passport records no ordering between the two anchored addresses and no \
                 capability value for either: one is not in the other's call list in either \
                 direction",
                "both addresses carry a category of UNKNOWN, so a subsystem query finds neither",
                "the chain has no recorded duration and no failure path; only the order was \
                 observed",
            ],
        ),
        phases: &[
            LifecyclePhase::EnvironmentProbe,
            LifecyclePhase::AdapterSelection,
            LifecyclePhase::DeviceCreation,
            LifecyclePhase::CapabilityProbe,
            LifecyclePhase::ResourceCreation,
            LifecyclePhase::Teardown,
        ],
    }])
}

/// Every probed-input contract.
pub fn all_resource() -> &'static [ResourceContract] {
    static CATALOG: std::sync::OnceLock<Vec<ResourceContract>> = std::sync::OnceLock::new();
    CATALOG.get_or_init(|| {
        vec![
            ResourceContract {
                core: core(
                    "device_memory_budget_is_a_policy_input",
                    ContractSubject::ResourceCreation,
                    "An available-memory figure is a probed input to an allocation budget, never a \
                     constant inside a decoder. The observed renderer read a byte count of \
                     available graphics memory and consumed it as MiB while sizing state; the \
                     value it obtained in that environment was a saturated 32-bit wrap, which the \
                     producing tool refused to copy for exactly this reason, since adopting it \
                     would encode one machine's arithmetic as the program's behaviour. What the \
                     measurement established is the shape -- a byte count, consumed as MiB, after \
                     the device exists -- and OpenSpore must take its budget the same way: as \
                     policy input, with the probed value attached to it as evidence rather than as \
                     a default. In this crate's vocabulary the dependency is \
                     device_memory_budget -> gpu_allocation_budget.",
                    EvidenceLevel::Observed,
                    EvidenceLevel::Inferred,
                    Some(ARTIFACT_MAIN_LOOP),
                    Some(observed(
                        "a probed available-memory byte count was consumed as MiB while sizing \
                         renderer state, and the value seen in one environment was a saturated \
                         32-bit wrap that the producer deliberately did not copy",
                        ARTIFACT_MAIN_LOOP,
                    )),
                    &[EngineComponent::DecodePolicy, EngineComponent::RendererStartup],
                    Some(&[EvidenceAnchor {
                        va: 0x007c_b590,
                        note: "consumed a probed available-memory figure and sized renderer state \
                               from it",
                    }]),
                    &[
                        "the passport records no memory figure, no budget and no sizing decision \
                         for this address",
                        "its body is 1,890 bytes with one external callee and no available fact \
                         group, so a decoder policy cannot be recovered from the file either",
                    ],
                ),
                probed: ProbedInput::DeviceMemoryBudget,
                decision: ResourceDecision::GpuAllocationBudget,
            },
            ResourceContract {
                core: core(
                    "device_capabilities_feed_renderer_state",
                    ContractSubject::ResourceCreation,
                    "Device capability answers are written into renderer state and read back from \
                     there, so a resource decision taken from them happens after that state exists \
                     rather than at load time. In the observed run a capability answer was written \
                     into the renderer's own state block and a second query's parameters were \
                     written beside it. OpenSpore must therefore treat a format or budget choice \
                     derived from capabilities as a renderer-startup decision with the probed \
                     answer attached, and not as a property of the asset it is loading. In this \
                     crate's vocabulary the dependency is device_capabilities -> \
                     resource_format_choice.",
                    EvidenceLevel::Observed,
                    EvidenceLevel::Inferred,
                    Some(ARTIFACT_MAIN_LOOP),
                    Some(observed(
                        "a device capability answer and a second query's parameters were written \
                         into the renderer's own state block and read back afterwards",
                        ARTIFACT_MAIN_LOOP,
                    )),
                    &[EngineComponent::DecodePolicy, EngineComponent::RendererStartup],
                    Some(&[EvidenceAnchor {
                        va: 0x011f_9250,
                        note: "wrote a capability answer into renderer state and read renderer \
                               state back afterwards",
                    }]),
                    &[
                        "the passport records no capability field, no renderer state block and no \
                         format decision for this address",
                        "the record's only caller is a single function, so even the call context of \
                         the write is one address wide",
                    ],
                ),
                probed: ProbedInput::DeviceCapabilities,
                decision: ResourceDecision::ResourceFormatChoice,
            },
        ]
    })
}

/// Every entry, as trait objects, in declaration order: behavioural, then
/// lifecycle, then resource.
pub fn all_invariants() -> Vec<&'static dyn SemanticInvariant> {
    let mut out: Vec<&'static dyn SemanticInvariant> = Vec::new();
    out.extend(
        all_behavioral()
            .iter()
            .map(|entry| entry as &dyn SemanticInvariant),
    );
    out.extend(
        all_lifecycle()
            .iter()
            .map(|entry| entry as &dyn SemanticInvariant),
    );
    out.extend(
        all_resource()
            .iter()
            .map(|entry| entry as &dyn SemanticInvariant),
    );
    out
}

/// One entry by id, or `None` when this build records none.
///
/// Absence here means "this catalog does not say", which is a different
/// statement from "this catalog says it is unknown" — see
/// `display_state_meaning_unknown`, which *is* recorded, at `UNKNOWN`.
pub fn invariant(id: &str) -> Option<&'static dyn SemanticInvariant> {
    all_invariants()
        .into_iter()
        .find(|entry| entry.core().id == id)
}

/// Every id in the catalog, for a report that cites them.
pub fn ids() -> Vec<&'static str> {
    all_invariants()
        .into_iter()
        .map(|entry| entry.core().id)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn every_entry_is_coherent_and_the_ids_are_unique() {
        let mut seen = BTreeSet::new();
        for entry in all_invariants() {
            let core = entry.core();
            assert!(seen.insert(core.id), "{} is declared twice", core.id);
            assert_eq!(entry.coherence(), Ok(()), "{} is not coherent", core.id);
        }
        assert_eq!(
            seen.len(),
            12,
            "the catalog grew; update this and the document"
        );
    }

    #[test]
    fn every_entry_keeps_its_generalisation_at_or_below_its_observation() {
        for entry in all_invariants() {
            let core = entry.core();
            assert!(
                core.level <= core.basis,
                "{} generalises to {} from an observation graded {}",
                core.id,
                core.level,
                core.basis
            );
        }
    }

    #[test]
    fn a_lookup_by_id_finds_every_declared_id_and_nothing_else() {
        for id in ids() {
            assert!(invariant(id).is_some(), "{id} is declared but not findable");
        }
        assert!(invariant("no_such_rule").is_none());
        assert!(
            invariant("display_state_meaning_unknown").map(|entry| entry.core().level)
                == Some(EvidenceLevel::Unknown),
            "a known unknown must stay findable and stay UNKNOWN"
        );
    }
}
