//! Semantic contracts: what the engine may **assert**, and on what evidence.
//!
//! # Why this layer exists
//!
//! A passport answers "what is at this address". It does not answer "what is
//! true about the renderer", and nothing in the 98 MB file states an ordering, a
//! capability or a dependency. An engine author still needs those three, and the
//! two honest ways to get them are very different:
//!
//! * a **committed contract** — a rule distilled from evidence, carrying its
//!   grade, its provenance and the component that would assert it
//!   ([`BehavioralInvariant`], [`LifecycleContract`], [`ResourceContract`]);
//! * a **projection** — a fact the snapshot already states, re-shaped for an
//!   engine question instead of for a research one ([`ContractLedger`]).
//!
//! Neither derives a new fact. The projection selects; the contract is written
//! down with its grade attached and its `not_stated` list beside it.
//!
//! # The rules that shaped every type here
//!
//! ## A runtime observation never upgrades a static verdict
//!
//! The bridge's own rule (`docs/tooling/semantic-exchange.md` §6.3) is that a
//! runtime observation settles nothing static. Every contract therefore
//! separates two grades and neither may borrow the other's strength:
//!
//! | field | meaning |
//! |---|---|
//! | [`InvariantCore::basis`] | how well the **observation** underneath is supported |
//! | [`InvariantCore::level`] | how well **this generalisation** is supported |
//!
//! `level` may never exceed `basis`, and [`SemanticContract::verdict_unchanged`]
//! is `true` for every contract in the crate — a property of construction rather
//! than a field anybody can set to the wrong value.
//!
//! ## A grade is never invented, and a role is never stronger than its evidence
//!
//! A lifecycle role is read off a name token, so it is
//! [`EvidenceLevel::Inferred`] **whatever** the name's own grade is. Of the
//! committed snapshot's 58 757 records, 1 171 carry an SDK name and 1 167 of
//! those are graded `CONFIRMED` — and a `CONFIRMED` name does not make "this
//! function is a teardown step" a confirmed claim.
//! [`SemanticContract::capped_from_fact`] is the only constructor that lowers a
//! grade and it is the one the ledger uses.
//!
//! ## The passport's silence is measured, not asserted
//!
//! `docs/SEMANTIC-CONTRACTS.md` has to say what the passport does *not* know
//! about a runtime observation, and a hand-written sentence about a 98 MB file is
//! not evidence. So [`ContractLedger::audit`] recomputes it —
//! [`PassportStance::States`], [`PassportStance::ExplicitlySilent`] or
//! [`PassportStance::Unplaced`], per anchor, on demand.
//!
//! # What this module refuses to encode
//!
//! **No platform API, and no interface name.** The engine-level invariant is
//! what a Bevy system or an asset pipeline needs; the observation's original
//! vocabulary is not something OpenSpore implements, and a contract that named
//! it would be an instruction to reproduce a graphics API this project does not
//! target. A test enforces that no statement mentions one — see
//! [`FORBIDDEN_IN_STATEMENTS`].
//!
//! **No runtime overlay.** Joining a static passport with a runtime overlay is
//! `spore-semantic runtime`'s job, and this crate does not re-implement it: see
//! [`crate::claims::claim::RUNTIME_OVERLAYS_NOT_READ`]. A contract's runtime side is
//! therefore a *reference* — [`RuntimeBasis`], a portable artifact id plus the
//! observation restated in engine terms — and never a parse of somebody else's
//! report.

use crate::address::Va;
use crate::error::BridgeError;
use crate::passport::{
    AddressResolution, Canonicalization, Passport, RuntimeGate, VTableMembership,
};
use crate::snapshot::Snapshot;
use spore_core::{EvidenceLevel, EvidenceState, Fact, Provenance, ProvenanceMode, SourceClass};

/// Bridge-local absence reason: no ABI record was recovered at all.
///
/// Marked bridge-local, like [`crate::REASON_NO_SDK_NAME`], because the format
/// publishes a reason for the *group* and this code says nothing more than the
/// group was unavailable.
pub const REASON_NO_ABI_RECORD: &str = "no_abi_record";

/// Tokens a test uses to keep platform API names out of the contract layer.
///
/// Matched case-insensitively against a contract statement, an invariant
/// statement and every evidence anchor's note. The list is deliberately short and
/// deliberately platform-shaped: `d3d` and `direct3d` are how this layer would
/// otherwise leak the observation's original vocabulary into an engine
/// requirement, and the test is what keeps that from happening by accident.
pub const FORBIDDEN_IN_STATEMENTS: &[&str] = &[
    "d3d",
    "direct3d",
    "idirect",
    "gdi32",
    "shell32",
    "user32",
    "kernel32",
    "msvcr",
    "hdc",
    "getprocaddress",
    "loadlibrary",
    "enumdisplay",
    "createwind",
    "getdevicecaps",
    "getadapter",
];

/// Which OpenSpore component a contract is addressed to.
///
/// An **identifier**, not a dependency. This crate is Bevy-free and depends on
/// nothing but `spore-core`, so it cannot name a Bevy type; what it can do is
/// name the place in the workspace where the assertion belongs, which is what
/// makes "a contract with no consumer is a note" checkable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum EngineComponent {
    /// `spore_engine::prepare` and `SporeAssetPlugin` — the step that turns
    /// decoded records into GPU resources. Owns every ordering question.
    RendererStartup,
    /// `spore_assets::{ContentStore, ResourceGraph}` — package priority, record
    /// identity, unresolved references.
    AssetResolution,
    /// The decoders' own policies: budgets, limits, what a record may assume
    /// about the machine that will run it.
    DecodePolicy,
    /// `spore-semantic export` and `reconstruction/evidence/` — which functions
    /// still owe runtime evidence, and which observations the universe cannot
    /// place.
    EvidencePipeline,
    /// A virtual-dispatch table assembled from the passport: (table, slot)
    /// targets, reached by byte displacement.
    DispatchTable,
}

impl EngineComponent {
    /// The stable spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::RendererStartup => "renderer_startup",
            Self::AssetResolution => "asset_resolution",
            Self::DecodePolicy => "decode_policy",
            Self::EvidencePipeline => "evidence_pipeline",
            Self::DispatchTable => "dispatch_table",
        }
    }

    /// The workspace path whose code would read the contract.
    pub const fn path_hint(self) -> &'static str {
        match self {
            Self::RendererStartup => "crates/spore-engine/src/loader.rs",
            Self::AssetResolution => "crates/spore-assets/src/store.rs",
            Self::DecodePolicy => "crates/spore-texture/src/",
            Self::EvidencePipeline => "crates/spore-semantic-bridge/src/snapshot.rs",
            Self::DispatchTable => "crates/spore-semantic-bridge/src/contracts.rs",
        }
    }
}

impl core::fmt::Display for EngineComponent {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The engine-level concern a contract is about.
///
/// Closed, and expressed in OpenSpore's vocabulary rather than the binary's.
/// There is deliberately no variant for "some function at some address": that is
/// what [`Passport`] is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ContractSubject {
    /// Choosing which display/adaptor the renderer will talk to.
    AdapterSelection,
    /// Reading what a device can do, before relying on it.
    CapabilityProbing,
    /// The ordered steps of bringing a renderer up.
    RendererStartup,
    /// A GPU-side allocation and the budget behind it.
    ResourceCreation,
    /// Resolving a name to the module, package or record that owns it.
    ModuleIdentity,
    /// Calling through a table slot rather than a direct call.
    VirtualDispatch,
    /// Placing an address in the function universe, or failing to.
    StaticIdentity,
    /// How well a calling convention is known.
    AbiStanding,
    /// A function whose static half is done and whose runtime half is not.
    EvidenceDebt,
    /// Which lifecycle phase a function's name places it in.
    LifecycleRole,
}

impl ContractSubject {
    /// The stable spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AdapterSelection => "adapter_selection",
            Self::CapabilityProbing => "capability_probing",
            Self::RendererStartup => "renderer_startup",
            Self::ResourceCreation => "resource_creation",
            Self::ModuleIdentity => "module_identity",
            Self::VirtualDispatch => "virtual_dispatch",
            Self::StaticIdentity => "static_identity",
            Self::AbiStanding => "abi_standing",
            Self::EvidenceDebt => "evidence_debt",
            Self::LifecycleRole => "lifecycle_role",
        }
    }
}

impl core::fmt::Display for ContractSubject {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// What a runtime producer observed, kept as a **reference**.
///
/// Never a parse. The producer's reports are its own artifacts and its schema is
/// not OpenSpore's to read here; what crosses the boundary is a portable
/// identifier — the same rule the runtime overlay enforces, never an absolute
/// path — plus the observation restated in engine terms.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuntimeBasis {
    /// What was observed, in engine terms and without a platform API name.
    pub observation: &'static str,
    /// The producer's portable artifact identifier, e.g.
    /// `work/reports/startup-recovery88.json`.
    pub artifact: &'static str,
    /// How well the observation itself is supported.
    pub level: EvidenceLevel,
}

impl RuntimeBasis {
    /// Whether the artifact reference is portable: non-empty, relative, and not
    /// carrying a drive or scheme separator.
    pub fn is_portable(&self) -> bool {
        !self.artifact.is_empty() && !self.artifact.starts_with('/') && !self.artifact.contains(':')
    }
}

/// An address where a runtime observation happened.
///
/// The note describes what was seen **there**. It never asserts what the
/// passport says about the address; [`ContractLedger::audit`] computes that
/// separately, because the two disagree on every anchor in the catalog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EvidenceAnchor {
    /// The address, as a number. Spelled with [`Va::canonical`] on output.
    pub va: u32,
    /// What was observed at that address, in engine terms.
    pub note: &'static str,
}

impl EvidenceAnchor {
    /// The canonical spelling of [`EvidenceAnchor::va`].
    pub fn va_text(&self) -> String {
        Va::new(self.va).canonical()
    }
}

/// One graded, engine-facing claim: subject, statement, grade, provenance,
/// consumer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticContract {
    /// Stable identifier, unique within its family.
    pub id: String,
    /// Which engine concern this is about.
    pub subject: ContractSubject,
    /// The address it is about, when it is about exactly one.
    ///
    /// `None` is a positive statement about the contract's *shape* — "this rule
    /// is not about one address" — and never a claim that an address is unknown.
    pub va: Option<u32>,
    /// The claim, in one or two sentences.
    pub statement: String,
    /// How strongly the claim itself is supported.
    pub level: EvidenceLevel,
    /// Where it came from. Never empty for a contract built by this crate.
    pub provenance: Vec<Provenance>,
    /// Which engine component would assert it.
    pub consumers: Vec<EngineComponent>,
    /// The runtime observation underneath, when there is one.
    pub basis: Option<RuntimeBasis>,
}

impl SemanticContract {
    /// Builds a contract whose grade and provenance are **copied** from a fact.
    ///
    /// The only safe way to turn a passport fact into a contract: there is no
    /// level argument, so a contract can never be graded above the envelope it
    /// came from.
    pub fn from_fact<T>(
        id: String,
        subject: ContractSubject,
        va: Option<u32>,
        statement: String,
        source: &Fact<T>,
        consumers: &[EngineComponent],
    ) -> Self {
        Self {
            id,
            subject,
            va,
            statement,
            level: source.level(),
            provenance: source.provenance().to_vec(),
            consumers: consumers.to_vec(),
            basis: None,
        }
    }

    /// As [`SemanticContract::from_fact`], but **capping** the grade.
    ///
    /// For a claim that adds something to the fact it was read from — a role
    /// inferred from a name, for instance. `CONFIRMED` name plus an inference is
    /// an `INFERRED` claim, and this constructor is where that arithmetic
    /// happens.
    pub fn capped_from_fact<T>(
        id: String,
        subject: ContractSubject,
        va: Option<u32>,
        statement: String,
        source: &Fact<T>,
        cap: EvidenceLevel,
        consumers: &[EngineComponent],
    ) -> Self {
        let mut contract = Self::from_fact(id, subject, va, statement, source, consumers);
        contract.level = contract.level.min(cap);
        contract
    }

    fn from_invariant_core(
        core: &InvariantCore,
        subject: ContractSubject,
        statement: String,
    ) -> Self {
        Self {
            id: core.id.to_owned(),
            subject,
            va: None,
            statement,
            level: core.level,
            provenance: core.provenance.clone(),
            consumers: core.consumers.to_vec(),
            basis: core.runtime,
        }
    }

    /// Whether this contract is about exactly one address.
    pub const fn is_address_scoped(&self) -> bool {
        self.va.is_some()
    }

    /// Always `true`, for every contract this crate can build.
    ///
    /// The bridge's rule is that a runtime observation never settles a static
    /// question, and no contract here settles one either: a [`SemanticContract`]
    /// either copies a static envelope's grade or states an authored rule whose
    /// grade is bounded by its own basis. So this is a method returning a
    /// constant, not a field — there is no construction path that sets it wrongly.
    pub const fn verdict_unchanged(&self) -> bool {
        true
    }

    /// The claim as a graded [`Fact`], or `None` when it carries no provenance.
    ///
    /// `None` means "this contract has no source recorded", which is not the same
    /// statement as "the claim is unknown".
    pub fn to_fact(&self) -> Option<Fact<String>> {
        if self.provenance.is_empty() {
            return None;
        }
        Some(Fact::new(
            self.level,
            EvidenceState::Derived,
            self.provenance.clone(),
            self.statement.clone(),
        ))
    }

    /// A one-line description, for a log line or a test message.
    pub fn describe(&self) -> String {
        let site = match self.va {
            Some(va) => format!("{} ", Va::new(va).canonical()),
            None => String::new(),
        };
        format!(
            "[{}/{}] {}{} ({})",
            self.id,
            self.level,
            site,
            self.statement,
            self.consumers
                .iter()
                .map(|component| component.as_str())
                .collect::<Vec<_>>()
                .join("+")
        )
    }
}

/// The fields every committed invariant carries, whatever its kind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvariantCore {
    /// Stable identifier, cited by `docs/SEMANTIC-CONTRACTS.md` and by tests.
    pub id: &'static str,
    /// The engine-level concern.
    pub subject: ContractSubject,
    /// The claim, in engine terms.
    pub statement: &'static str,
    /// How well the underlying **observation** is supported.
    ///
    /// [`EvidenceLevel::Unknown`] here means the observation did not reach the
    /// claim — see `display_state_meaning_unknown` in
    /// [`crate::invariants`], which is recorded rather than left out precisely so
    /// that "we did not look" is a statement in the catalog.
    pub basis: EvidenceLevel,
    /// How well **this generalisation** is supported. Never above `basis`.
    pub level: EvidenceLevel,
    /// Where it comes from: this document, plus the runtime artifact when there is
    /// one.
    pub provenance: Vec<Provenance>,
    /// Which engine component would assert it. Never empty.
    pub consumers: &'static [EngineComponent],
    /// The addresses the observation happened at.
    ///
    /// `None` means "this invariant is not about an address in the binary" — the
    /// module-identity and evidence-debt rules are about how evidence is produced,
    /// not about a place in the program. `Some(&[])` is refused by the catalog's
    /// coherence tests, because an empty anchor list would be indistinguishable
    /// from "looked and found no address".
    pub anchors: Option<&'static [EvidenceAnchor]>,
    /// What the passport does **not** say about this, one clause per entry.
    pub not_stated: &'static [&'static str],
    /// The runtime observation, when one was admitted.
    pub runtime: Option<RuntimeBasis>,
}

impl InvariantCore {
    /// Whether the grade arithmetic is legal.
    pub fn grade_is_coherent(&self) -> bool {
        self.level <= self.basis
    }

    /// Always `true`: admitting runtime evidence changes no static verdict.
    pub const fn verdict_unchanged(&self) -> bool {
        true
    }

    /// The committed reference every invariant carries, plus the runtime artifact
    /// when there is one.
    pub(crate) fn provenance(artifact: Option<&'static str>) -> Vec<Provenance> {
        // One spelling, held by the catalog module: a duplicated literal is a
        // second place to forget to update, and this one is a provenance
        // reference that has to match the document it names.
        let mut provenance = vec![Provenance::committed(crate::invariants::DOCS_REFERENCE)];
        if let Some(artifact) = artifact {
            provenance.push(Provenance::new(
                ProvenanceMode::Imported,
                artifact,
                SourceClass::Runtime,
            ));
        }
        provenance
    }
}

/// The shared behaviour of every committed invariant, whatever its kind.
///
/// The three invariant shapes differ only in what they add — a phase chain, a
/// probed input — so everything that must hold of *all* of them lives here, once.
#[allow(missing_debug_implementations)]
pub trait SemanticInvariant: core::fmt::Debug {
    /// The shared envelope.
    fn core(&self) -> &InvariantCore;

    /// The invariant as a uniform [`SemanticContract`].
    fn as_contract(&self) -> SemanticContract {
        let core = self.core();
        SemanticContract::from_invariant_core(core, core.subject, core.statement.to_owned())
    }

    /// Checks every rule a committed invariant must satisfy, naming the first
    /// failure.
    ///
    /// These are the checks that make the catalog trustworthy rather than merely
    /// present: a consumer with nowhere to be, an unsourced claim, an illegal
    /// grade, a leaked platform vocabulary, or an anchor list that is empty
    /// because nobody looked. `tests/contracts.rs` runs this over the whole
    /// catalog, so adding an entry that breaks any of them fails the suite.
    fn coherence(&self) -> Result<(), &'static str> {
        let core = self.core();
        if core.id.is_empty() {
            return Err("an invariant must be nameable");
        }
        if core.statement.trim().is_empty() {
            return Err("an invariant must state something");
        }
        if !core.grade_is_coherent() {
            return Err("level must not exceed basis");
        }
        if core.consumers.is_empty() {
            return Err("a contract with no consumer is a note, not infrastructure");
        }
        if core.provenance.is_empty() {
            return Err("an invariant must be sourced");
        }
        if core
            .provenance
            .iter()
            .all(|entry| entry.source_class != SourceClass::CommittedArtifact)
        {
            return Err("an invariant must cite the committed document that writes it out");
        }
        if let Some(runtime) = core.runtime {
            if !runtime.is_portable() {
                return Err("a runtime reference must be portable, never an absolute path");
            }
            if !core
                .provenance
                .iter()
                .any(|entry| entry.source_class == SourceClass::Runtime)
            {
                return Err("a runtime observation must appear in the provenance as `runtime`");
            }
        }
        if matches!(core.anchors, Some(anchors) if anchors.is_empty()) {
            return Err("an anchor list must be absent, not empty: `Some(&[])` cannot say why");
        }
        if core.not_stated.is_empty() {
            return Err("an invariant must say what the passport does not state about it");
        }
        for text in std::iter::once(core.statement)
            .chain(core.anchors.unwrap_or(&[]).iter().map(|anchor| anchor.note))
        {
            let lower = text.to_lowercase();
            if let Some(forbidden) = FORBIDDEN_IN_STATEMENTS
                .iter()
                .find(|token| lower.contains(**token))
            {
                return Err(if *forbidden == "createwind" {
                    "a window-handle API name leaked into a contract statement"
                } else {
                    "a platform API name leaked into a contract statement"
                });
            }
        }
        Ok(())
    }
}

/// A committed engine-level rule, with no ordering and no budget attached.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BehavioralInvariant {
    /// The shared envelope.
    pub core: InvariantCore,
}

impl SemanticInvariant for BehavioralInvariant {
    fn core(&self) -> &InvariantCore {
        &self.core
    }
}

/// Which half of a program's life a name states.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum LifecycleRoleKind {
    /// Bringing something up.
    Setup,
    /// Taking it down.
    Teardown,
}

impl LifecycleRoleKind {
    /// The stable spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Setup => "setup",
            Self::Teardown => "teardown",
        }
    }
}

impl core::fmt::Display for LifecycleRoleKind {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One step of a renderer's startup, in the order the steps must happen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum LifecyclePhase {
    /// Ask the environment what is available.
    EnvironmentProbe,
    /// Choose one of what was found and open a handle to it.
    AdapterSelection,
    /// Create the device that will do the drawing.
    DeviceCreation,
    /// Ask the device what it supports and how much it has.
    CapabilityProbe,
    /// Create the resources and renderer state those answers shape.
    ResourceCreation,
    /// Tear the above down.
    Teardown,
}

impl LifecyclePhase {
    /// The stable spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::EnvironmentProbe => "environment_probe",
            Self::AdapterSelection => "adapter_selection",
            Self::DeviceCreation => "device_creation",
            Self::CapabilityProbe => "capability_probe",
            Self::ResourceCreation => "resource_creation",
            Self::Teardown => "teardown",
        }
    }

    /// Whether nothing may follow this phase.
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Teardown)
    }
}

impl core::fmt::Display for LifecyclePhase {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The phases a runtime run actually walked, as an assertion a `Startup` system
/// can make about itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LifecycleContract {
    /// The shared envelope.
    pub core: InvariantCore,
    /// The phases, in the order they must occur.
    pub phases: &'static [LifecyclePhase],
}

/// The phase that was required to come later than one that did not.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PhaseOrderViolation {
    /// The phase that happened too early.
    pub early: LifecyclePhase,
    /// The phase it was required to follow.
    pub late: LifecyclePhase,
    /// Where in the observed sequence the early phase was found.
    pub observed_at: usize,
}

impl core::fmt::Display for PhaseOrderViolation {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "{} was required to follow {}, but it occurs {} step(s) earlier in the sequence",
            self.early, self.late, self.observed_at
        )
    }
}

impl LifecycleContract {
    /// Checks that `observed` respects this contract's ordering.
    ///
    /// The rule in full: every pair drawn from [`LifecycleContract::phases`]
    /// requires the earlier phase to occur before the later one. A phase absent
    /// from `observed` is **not** a violation and **not** a pass — the run may
    /// simply not have reached it — so this returns `Ok` for a partial sequence
    /// and says nothing about the phases it did not see. A caller that needs "did
    /// it happen?" must ask that separately; this answers only "is what happened
    /// in the right order?".
    ///
    /// A repeated phase violates nothing here: the contract constrains order, not
    /// multiplicity.
    pub fn check_order(&self, observed: &[LifecyclePhase]) -> Result<(), PhaseOrderViolation> {
        let rank = |phase: LifecyclePhase| {
            self.phases
                .iter()
                .position(|candidate| *candidate == phase)
                .unwrap_or(usize::MAX)
        };
        for (index, phase) in observed.iter().enumerate() {
            let here = rank(*phase);
            if here == usize::MAX {
                continue;
            }
            for earlier in &observed[..index] {
                let there = rank(*earlier);
                if there != usize::MAX && there > here {
                    return Err(PhaseOrderViolation {
                        early: *phase,
                        late: *earlier,
                        observed_at: index,
                    });
                }
            }
        }
        Ok(())
    }
}

impl SemanticInvariant for LifecycleContract {
    fn core(&self) -> &InvariantCore {
        &self.core
    }
}

/// An input whose value only exists at runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ProbedInput {
    /// How much memory the graphics device reports as available.
    DeviceMemoryBudget,
    /// What the device reports it can do.
    DeviceCapabilities,
}

impl ProbedInput {
    /// The stable spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::DeviceMemoryBudget => "device_memory_budget",
            Self::DeviceCapabilities => "device_capabilities",
        }
    }
}

impl core::fmt::Display for ProbedInput {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A decision that consumes a [`ProbedInput`] and must not be frozen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ResourceDecision {
    /// How much GPU memory OpenSpore is willing to allocate.
    GpuAllocationBudget,
    /// Which format a resource is created with.
    ResourceFormatChoice,
}

impl ResourceDecision {
    /// The stable spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::GpuAllocationBudget => "gpu_allocation_budget",
            Self::ResourceFormatChoice => "resource_format_choice",
        }
    }
}

impl core::fmt::Display for ResourceDecision {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A rule saying *this* decision depends on *that* runtime input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceContract {
    /// The shared envelope.
    pub core: InvariantCore,
    /// The input that only exists at runtime.
    pub probed: ProbedInput,
    /// The decision that must therefore be a policy input.
    pub decision: ResourceDecision,
}

impl SemanticInvariant for ResourceContract {
    fn core(&self) -> &InvariantCore {
        &self.core
    }
}

// ---------------------------------------------------------------------------
// The address-level projection
// ---------------------------------------------------------------------------

/// What is known about a calling convention at one address.
///
/// Three outcomes, and the difference between the last two is the whole point:
/// "the ABI engine looked and declined" is not "no ABI was recovered".
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AbiStanding {
    /// A record exists and names a convention.
    Known {
        /// The convention, verbatim: `__cdecl`, `__stdcall`, `__thiscall` or
        /// `__fastcall`.
        convention: String,
        /// The convention's own grade. `INFERRED` for every inferred record;
        /// `OBSERVED` only where a listing fixed it.
        level: EvidenceLevel,
        /// The engine's verdict, verbatim, e.g. `ABI_INFERRED`.
        verdict: Option<String>,
        /// The record's provenance.
        provenance: Vec<Provenance>,
    },
    /// A record exists and its `convention` is `null`: undetermined.
    ///
    /// This is a machine property rather than a gap in the work — a `__thiscall`
    /// and a `__fastcall` with one register argument and no stack arguments emit
    /// identical code — and it carries the record's provenance because a record
    /// *was* read.
    Undetermined {
        /// The record's own confidence rung, `UNKNOWN` on every undetermined
        /// record.
        level: EvidenceLevel,
        /// The engine's verdict, verbatim, e.g. `ABI_UNKNOWN`.
        verdict: Option<String>,
        /// The record's provenance.
        provenance: Vec<Provenance>,
    },
    /// No ABI record was recovered. `reason` is the format's own absence code.
    NotRecorded {
        /// The closed-vocabulary reason the group published.
        reason: String,
    },
}

impl AbiStanding {
    /// The grade of the answer. Always [`EvidenceLevel::Unknown`] for
    /// [`AbiStanding::NotRecorded`], because a non-finding carries no grade.
    pub fn level(&self) -> EvidenceLevel {
        match self {
            Self::Known { level, .. } | Self::Undetermined { level, .. } => *level,
            Self::NotRecorded { .. } => EvidenceLevel::Unknown,
        }
    }

    /// The convention, when one was determined.
    pub fn convention(&self) -> Option<&str> {
        match self {
            Self::Known { convention, .. } => Some(convention),
            Self::Undetermined { .. } | Self::NotRecorded { .. } => None,
        }
    }

    /// Whether a convention was determined. `false` covers both abstentions;
    /// [`AbiStanding::reason`] is what distinguishes them.
    pub fn is_determined(&self) -> bool {
        matches!(self, Self::Known { .. })
    }

    /// The record's provenance, or an empty slice when no record was read.
    pub fn provenance(&self) -> &[Provenance] {
        match self {
            Self::Known { provenance, .. } | Self::Undetermined { provenance, .. } => provenance,
            Self::NotRecorded { .. } => &[],
        }
    }

    /// Why there is no convention: the format's reason code, or
    /// `convention_undetermined`.
    pub fn reason(&self) -> &str {
        match self {
            Self::Known { .. } => "",
            Self::Undetermined { .. } => crate::REASON_CONVENTION_UNDETERMINED,
            Self::NotRecorded { reason } => reason,
        }
    }

    /// Reads a passport's ABI group into a standing.
    pub fn from_passport(passport: &Passport) -> Self {
        let fact = passport.abi();
        let Some(record) = fact.value() else {
            return Self::NotRecorded {
                reason: fact.reason().unwrap_or(REASON_NO_ABI_RECORD).to_owned(),
            };
        };
        let provenance = fact.provenance().to_vec();
        match record.convention.as_deref() {
            Some(convention) => Self::Known {
                convention: convention.to_owned(),
                level: record.convention_confidence,
                verdict: record.verdict.clone(),
                provenance,
            },
            None => Self::Undetermined {
                level: record.convention_confidence,
                verdict: record.verdict.clone(),
                provenance,
            },
        }
    }
}

/// The lifecycle role a function's name states, and how weakly.
///
/// # The role is inferred; the name is not
///
/// Of the committed snapshot's 1 171 SDK-named records, 1 167 are graded
/// `CONFIRMED` (one `SUPPORTED`, three `UNKNOWN`), and that is the *name's* grade.
/// Reading "this is a teardown step" off the tail of that name is an inference
/// about the program's structure, so the role is [`EvidenceLevel::Inferred`]
/// regardless — including for the three names the file itself could not grade. The
/// token that fired is carried so the rule can be audited.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LifecycleRole {
    /// Which half of the program's life the name places the function in.
    pub kind: LifecycleRoleKind,
    /// The name token that fired, verbatim.
    pub token: &'static str,
    /// The SDK name, verbatim.
    pub name: String,
    /// The name's own grade, which is **not** the role's grade.
    pub name_level: EvidenceLevel,
    /// Where the name came from.
    pub provenance: Vec<Provenance>,
}

impl LifecycleRole {
    /// The ceiling a role claim may ever reach, however strong the name is.
    pub const ROLE_LEVEL: EvidenceLevel = EvidenceLevel::Inferred;

    /// The role's grade: [`Self::ROLE_LEVEL`], always.
    pub const fn role_level(&self) -> EvidenceLevel {
        Self::ROLE_LEVEL
    }

    /// Reads a passport's SDK name for a lifecycle token.
    ///
    /// `None` when the record states no SDK name, when the name carries no `::`
    /// (there is no namespace to attribute a phase to), or when the final segment
    /// is not one of [`LIFECYCLE_TOKENS`].
    ///
    /// **No prefix matching.** A `Create\w*` prefix would add 38 records on the
    /// committed snapshot, among them `CreateSpaceCommEvent` and
    /// `CreateCivCommEvent`, which create no resource at all; a role read off a
    /// prefix would be wrong for them and would read exactly as confidently as
    /// the ones it got right. The closed token set matches 55 records.
    pub fn from_passport(passport: &Passport) -> Option<Self> {
        let fact = passport.sdk_name();
        let name = fact.value()?;
        let (_namespace, token) = name.rsplit_once("::")?;
        let (token, kind) = lifecycle_token(token)?;
        Some(Self {
            kind,
            token,
            name: (*name).to_owned(),
            name_level: fact.level(),
            provenance: fact.provenance().to_vec(),
        })
    }
}

/// The closed set of name tokens that state a lifecycle role.
///
/// Exact matches only, on the segment after the last `::`. Two of the seven
/// (`Init`, `Initialize`) are a prefix pair, so the rule is stated as whole-segment
/// equality and *not* derived from any prefix: nothing is added to the set
/// because a shorter token happens to be there.
pub const LIFECYCLE_TOKENS: &[(&str, LifecycleRoleKind)] = &[
    ("Initialize", LifecycleRoleKind::Setup),
    ("Init", LifecycleRoleKind::Setup),
    ("OnExit", LifecycleRoleKind::Teardown),
    ("Unload", LifecycleRoleKind::Teardown),
    ("Release", LifecycleRoleKind::Teardown),
    ("Dispose", LifecycleRoleKind::Teardown),
    ("Shutdown", LifecycleRoleKind::Teardown),
];

/// The token and the role it names, when the closed set names one.
pub fn lifecycle_token(token: &str) -> Option<(&'static str, LifecycleRoleKind)> {
    LIFECYCLE_TOKENS
        .iter()
        .find(|(candidate, _)| *candidate == token)
        .map(|(candidate, kind)| (*candidate, *kind))
}

/// One vftable contract: this address is reachable by calling through a slot.
///
/// The slot index is the fact and the byte displacement is the *access*, which
/// is why [`SemanticContract::from_fact`] copies the vtable group's own grade:
/// the sound scan is the only source the ABI engine accepts, and this crate does
/// not re-derive it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DispatchContract {
    /// The address that is the target.
    pub va: u32,
    /// Every (table, slot) pair naming it.
    pub memberships: Vec<VTableMembership>,
    /// The vtable group's grade, copied.
    pub level: EvidenceLevel,
    /// The vtable group's provenance, copied.
    pub provenance: Vec<Provenance>,
}

impl DispatchContract {
    /// The slot displacement of the first membership, or `None` when the group
    /// was available with no membership.
    pub fn first_displacement(&self) -> Option<u32> {
        self.memberships
            .first()
            .map(VTableMembership::byte_displacement)
    }

    /// As a uniform contract.
    pub fn to_contract(&self) -> SemanticContract {
        let mut tables: Vec<u32> = self.memberships.iter().map(|m| m.table_va).collect();
        tables.sort_unstable();
        tables.dedup();
        let first = self.memberships.first();
        let source = Fact::<u32>::new(
            self.level,
            EvidenceState::Derived,
            self.provenance.clone(),
            self.va,
        );
        SemanticContract::from_fact(
            format!("dispatch_target:{}", Va::new(self.va).canonical()),
            ContractSubject::VirtualDispatch,
            Some(self.va),
            format!(
                "{} is a virtual-dispatch target: {} membership(s) across {} distinct table(s), \
                 first at slot {} (byte displacement 0x{:x}). Reach a slot by displacement from \
                 the table base, never by subscripting a table structure.",
                Va::new(self.va).canonical(),
                self.memberships.len(),
                tables.len(),
                first.map_or(0, |m| m.slot),
                self.first_displacement().unwrap_or(0)
            ),
            &source,
            &[EngineComponent::DispatchTable],
        )
    }
}

/// One runtime-gate contract: this function's behaviour is gated on a value that
/// only exists at runtime.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeGateContract {
    /// The gated address.
    pub va: u32,
    /// The gate's state, including [`RuntimeGate::Unstated`].
    pub gate: RuntimeGate,
    /// The reconstruction record's grade, copied.
    pub level: EvidenceLevel,
    /// The reconstruction record's provenance, copied.
    pub provenance: Vec<Provenance>,
    /// The record's blocking reason, verbatim, when it published one.
    pub blocking_reason: Option<String>,
}

impl RuntimeGateContract {
    /// As a uniform contract, or `None` for [`RuntimeGate::Unstated`].
    ///
    /// `Unstated` yields no contract: "this record says nothing about a runtime
    /// gate" is a non-finding, and a contract is a claim. The state itself is
    /// still readable on [`RuntimeGateContract::gate`], and
    /// [`ContractLedger::open_gate_count`] counts only the open ones.
    pub fn to_contract(&self) -> Option<SemanticContract> {
        let statement = match self.gate {
            // `Open` is neither a pass nor a failure: the gate is open, nothing
            // was attempted and nothing failed. Calling it a failure would be the
            // exact collapse the four-state gate exists to prevent.
            RuntimeGate::Open => format!(
                "{} is runtime-gated: its static reconstruction is done and its runtime half is \
                 outstanding. The gate is open, nothing was attempted and nothing failed.",
                Va::new(self.va).canonical()
            ),
            RuntimeGate::Closed => format!(
                "{}'s runtime gate is closed: its reconstruction does not depend on a runtime \
                 observation.",
                Va::new(self.va).canonical()
            ),
            RuntimeGate::Unstated => return None,
        };
        let source = Fact::<u32>::new(
            self.level,
            EvidenceState::Derived,
            self.provenance.clone(),
            self.va,
        );
        Some(SemanticContract::from_fact(
            format!("runtime_gate:{}", Va::new(self.va).canonical()),
            ContractSubject::EvidenceDebt,
            Some(self.va),
            statement,
            &source,
            &[EngineComponent::EvidencePipeline],
        ))
    }
}

/// Every engine-facing contract one address carries.
///
/// Obtained from [`ContractLedger::for_va`], which never fails: an address the
/// universe cannot place gets an empty contract set and a
/// [`Canonicalization::NonFunctionEntity`] resolution, which is a finding rather
/// than an error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddressContracts {
    /// Where the address sits in the function universe.
    pub resolution: AddressResolution,
    /// The dispatch contract, when the vtable group was available.
    pub dispatch: Option<DispatchContract>,
    /// The ABI standing, always present as a three-way answer.
    pub abi: AbiStanding,
    /// The lifecycle role, when the SDK name states one.
    pub lifecycle: Option<LifecycleRole>,
    /// The runtime gate, when a reconstruction record was recovered.
    pub runtime_gate: Option<RuntimeGateContract>,
}

impl AddressContracts {
    /// Whether the universe placed this address at all.
    pub fn is_placed(&self) -> bool {
        self.resolution.is_resolved()
    }

    /// Whether any contract exists here. `false` for an unplaced address, and that
    /// absence is the finding — nothing is manufactured to fill it.
    pub fn is_empty(&self) -> bool {
        self.dispatch.is_none()
            && self.lifecycle.is_none()
            && self.runtime_gate.is_none()
            && !self.abi.is_determined()
    }

    /// The address a contract is about: the canonical entry, or the requested
    /// address when the universe placed nothing.
    fn subject_va(&self) -> u32 {
        self.resolution
            .canonical
            .unwrap_or(self.resolution.requested)
    }

    /// Every contract, as the uniform envelope.
    ///
    /// A `NotRecorded` or `Undetermined` ABI standing contributes nothing: a
    /// non-finding is not a claim, and the caller can still read
    /// [`AddressContracts::abi`] to see which of the two it was.
    pub fn contracts(&self) -> Vec<SemanticContract> {
        let mut out = Vec::new();
        if let Some(dispatch) = &self.dispatch {
            out.push(dispatch.to_contract());
        }
        if let Some(lifecycle) = &self.lifecycle {
            let source = Fact::<&str>::new(
                lifecycle.name_level,
                EvidenceState::Persisted,
                lifecycle.provenance.clone(),
                lifecycle.name.as_str(),
            );
            out.push(SemanticContract::capped_from_fact(
                format!("lifecycle_role:{}", Va::new(self.subject_va()).canonical()),
                ContractSubject::LifecycleRole,
                Some(self.subject_va()),
                format!(
                    "{} is named {} in the SDK and the final segment of that name states a {} \
                     step; the role is read off the token, so it is an inference and no stronger.",
                    Va::new(self.subject_va()).canonical(),
                    lifecycle.name,
                    lifecycle.kind
                ),
                &source,
                LifecycleRole::ROLE_LEVEL,
                &[
                    EngineComponent::RendererStartup,
                    EngineComponent::EvidencePipeline,
                ],
            ));
        }
        if let Some(gate) = &self.runtime_gate {
            if let Some(contract) = gate.to_contract() {
                out.push(contract);
            }
        }
        if let AbiStanding::Known {
            convention,
            level,
            provenance,
            ..
        } = &self.abi
        {
            let source = Fact::<&str>::new(
                *level,
                EvidenceState::Derived,
                provenance.clone(),
                convention.as_str(),
            );
            out.push(SemanticContract::from_fact(
                format!("abi_convention:{}", Va::new(self.subject_va()).canonical()),
                ContractSubject::AbiStanding,
                Some(self.subject_va()),
                format!(
                    "{} is called through {} on the strength of a machine-derived ABI record graded \
                     {}; that grade is the record's own, not an OpenSpore endorsement.",
                    Va::new(self.subject_va()).canonical(),
                    convention,
                    level
                ),
                &source,
                &[EngineComponent::DispatchTable],
            ));
        }
        out
    }
}

/// What the passport states about an address an invariant is anchored on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PassportStance {
    /// A record sits at the resolved address and states at least one of the things
    /// a rule could be about: a name, an ABI record, a vtable membership, a
    /// semantic record or a reconstruction record.
    States,
    /// A record sits at the resolved address and **every one of those groups is an
    /// explicit absence**. Computed by [`ContractLedger::states_anything`]; it is a
    /// statement about the file, not a claim that the fact is false.
    ExplicitlySilent,
    /// The function universe cannot place the address at all.
    Unplaced,
}

impl PassportStance {
    /// The stable spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::States => "states",
            Self::ExplicitlySilent => "explicitly_silent",
            Self::Unplaced => "unplaced",
        }
    }
}

impl core::fmt::Display for PassportStance {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One anchor, with the passport's stance recomputed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AuditedAnchor {
    /// The address.
    pub va: u32,
    /// The canonical entry, when the universe placed it.
    pub canonical: Option<u32>,
    /// Which rule fired.
    pub rule: Canonicalization,
    /// What the passport says about it.
    pub stance: PassportStance,
}

/// One invariant, with every anchor audited against the snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvariantAudit {
    /// The invariant's id.
    pub id: &'static str,
    /// Its grade, copied.
    pub level: EvidenceLevel,
    /// One entry per anchor. Empty when the invariant is not address-anchored.
    pub anchors: Vec<AuditedAnchor>,
}

/// One snapshot, projected into the engine's questions.
///
/// # Cost
///
/// [`ContractLedger::build`] streams every record once and keeps a small row per
/// function — a few memberships and at most one ABI name, for the records that
/// have them — so it costs one extra full parse of the 98 MB file plus a few MiB
/// of rows. It borrows the [`Snapshot`] rather than owning it, which is what lets
/// [`ContractLedger::for_va`] answer with the snapshot's own canonical-identity
/// rule instead of a second copy of it: a third mapping algorithm would be a
/// third answer to the same question.
///
/// # What it does not do
///
/// It does not read a runtime overlay and it does not infer anything the
/// passport does not state. Every row is a projection of graded facts, and
/// [`ContractLedger::audit`] recomputes the file's silences rather than asserting
/// them.
#[derive(Debug)]
pub struct ContractLedger<'s> {
    snapshot: &'s Snapshot,
    rows: Vec<LedgerRow>,
    dispatch_record_count: usize,
    lifecycle_record_count: usize,
    open_gate_count: usize,
}

/// One function's projected row.
#[derive(Debug, Clone)]
struct LedgerRow {
    va: u32,
    dispatch: Option<DispatchContract>,
    abi: AbiStanding,
    lifecycle: Option<LifecycleRole>,
    runtime_gate: Option<RuntimeGateContract>,
}

impl<'s> ContractLedger<'s> {
    /// Projects every record in `snapshot`.
    pub fn build(snapshot: &'s Snapshot) -> Result<Self, BridgeError> {
        let mut rows = Vec::with_capacity(snapshot.function_count().min(1 << 20));
        let mut dispatch_record_count = 0usize;
        let mut lifecycle_record_count = 0usize;
        let mut open_gate_count = 0usize;

        snapshot.for_each_passport(|passport| {
            let va = passport.canonical_va();
            let dispatch = {
                let fact = passport.vtable_memberships();
                fact.value().map(|memberships| {
                    dispatch_record_count += 1;
                    DispatchContract {
                        va,
                        memberships: memberships.to_vec(),
                        level: fact.level(),
                        provenance: fact.provenance().to_vec(),
                    }
                })
            };
            let lifecycle = match LifecycleRole::from_passport(passport) {
                Some(role) => {
                    lifecycle_record_count += 1;
                    Some(role)
                }
                None => None,
            };
            let runtime_gate = {
                let fact = passport.runtime_gate();
                match (fact.value(), passport.reconstruction().value()) {
                    (Some(gate), Some(record)) => {
                        if *gate == RuntimeGate::Open {
                            open_gate_count += 1;
                        }
                        Some(RuntimeGateContract {
                            va,
                            gate: *gate,
                            level: fact.level(),
                            provenance: fact.provenance().to_vec(),
                            blocking_reason: record.runtime_blocking_reason.clone(),
                        })
                    }
                    // A gate fact without a record behind it, or a record without
                    // a gate, is a shape this crate does not invent a value for.
                    _ => None,
                }
            };
            rows.push(LedgerRow {
                va,
                dispatch,
                abi: AbiStanding::from_passport(passport),
                lifecycle,
                runtime_gate,
            });
            Ok(())
        })?;

        Ok(Self {
            snapshot,
            rows,
            dispatch_record_count,
            lifecycle_record_count,
            open_gate_count,
        })
    }

    /// The snapshot this ledger projects.
    pub fn snapshot(&self) -> &Snapshot {
        self.snapshot
    }

    /// How many functions the ledger holds a row for.
    pub fn len(&self) -> usize {
        self.rows.len()
    }

    /// Whether the ledger is empty. A snapshot always has records, so this is
    /// `false` for any ledger this crate can build.
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// How many records carry at least one vftable membership.
    pub fn dispatch_record_count(&self) -> usize {
        self.dispatch_record_count
    }

    /// How many records carry an SDK name whose final segment states a lifecycle
    /// role.
    pub fn lifecycle_record_count(&self) -> usize {
        self.lifecycle_record_count
    }

    /// How many records state an **open** runtime gate.
    ///
    /// Not "how many need runtime evidence": [`RuntimeGate::Unstated`] is a
    /// fourth state and is not counted here.
    pub fn open_gate_count(&self) -> usize {
        self.open_gate_count
    }

    /// Every dispatch contract, in canonical-VA order.
    pub fn dispatch_contracts(&self) -> impl Iterator<Item = &DispatchContract> {
        self.rows.iter().filter_map(|row| row.dispatch.as_ref())
    }

    /// Every lifecycle role, in canonical-VA order.
    pub fn lifecycle_roles(&self) -> impl Iterator<Item = &LifecycleRole> {
        self.rows.iter().filter_map(|row| row.lifecycle.as_ref())
    }

    /// Every record whose runtime gate is open, in canonical-VA order.
    pub fn runtime_gated(&self) -> impl Iterator<Item = &RuntimeGateContract> {
        self.rows
            .iter()
            .filter_map(|row| row.runtime_gate.as_ref())
            .filter(|gate| gate.gate == RuntimeGate::Open)
    }

    /// Every record with a determined convention, in canonical-VA order.
    pub fn known_conventions(&self) -> impl Iterator<Item = (u32, &str)> {
        self.rows
            .iter()
            .filter_map(|row| row.abi.convention().map(|convention| (row.va, convention)))
    }

    /// Everything the engine may assert about `va`.
    ///
    /// Non-failing by design. An interior address resolves to the entry containing
    /// it and inherits that entry's contracts; an address the universe cannot
    /// place gets [`AddressContracts::is_placed`] `false` and no contracts, which
    /// is the finding.
    pub fn for_va(&self, va: u32) -> AddressContracts {
        let resolution = self.snapshot.resolve(va);
        let row = resolution
            .canonical_va()
            .and_then(|canonical| self.row_at(canonical));
        match row {
            Some(row) => AddressContracts {
                resolution,
                dispatch: row.dispatch.clone(),
                abi: row.abi.clone(),
                lifecycle: row.lifecycle.clone(),
                runtime_gate: row.runtime_gate.clone(),
            },
            None => AddressContracts {
                resolution,
                dispatch: None,
                abi: AbiStanding::NotRecorded {
                    reason: "no_passport_record".to_owned(),
                },
                lifecycle: None,
                runtime_gate: None,
            },
        }
    }

    /// The row for an exact canonical entry.
    fn row_at(&self, canonical: u32) -> Option<&LedgerRow> {
        let index = self.rows.partition_point(|row| row.va < canonical);
        self.rows.get(index).filter(|row| row.va == canonical)
    }

    /// Whether a passport states anything an engine rule could be about.
    ///
    /// The rule behind [`PassportStance::ExplicitlySilent`], exposed so a caller
    /// can see it: a record *states* something when it carries an SDK name, an ABI
    /// record, a vtable membership, a semantic record or a reconstruction record.
    /// All five absent is an explicit silence, which is a different statement from
    /// having no record at all.
    pub fn states_anything(passport: &Passport) -> bool {
        passport.sdk_name().is_available()
            || passport.abi().is_available()
            || passport.vtable_memberships().is_available()
            || passport.semantics().is_available()
            || passport.reconstruction().is_available()
    }

    /// The passport's stance on one address.
    pub fn stance(&self, va: u32) -> PassportStance {
        let Some(canonical) = self.snapshot.resolve(va).canonical_va() else {
            return PassportStance::Unplaced;
        };
        match self.snapshot.lookup(canonical) {
            Ok(passport) if Self::states_anything(&passport) => PassportStance::States,
            Ok(_) => PassportStance::ExplicitlySilent,
            // A canonical entry whose line cannot be re-read is corruption, and
            // `Snapshot::open` already proved the file intact. Reporting "unplaced"
            // would be a fabricated reading of a file this ledger holds.
            Err(_) => PassportStance::Unplaced,
        }
    }

    /// Recomputes what the passport says about every anchor of every invariant.
    ///
    /// This is what makes the "what the passport does not say" section of
    /// `docs/SEMANTIC-CONTRACTS.md` evidence rather than prose.
    pub fn audit<'a, I>(&self, invariants: I) -> Vec<InvariantAudit>
    where
        I: IntoIterator<Item = &'a dyn SemanticInvariant>,
    {
        invariants
            .into_iter()
            .map(|invariant| InvariantAudit {
                id: invariant.core().id,
                level: invariant.core().level,
                anchors: invariant
                    .core()
                    .anchors
                    .unwrap_or(&[])
                    .iter()
                    .map(|anchor| {
                        let resolution = self.snapshot.resolve(anchor.va);
                        AuditedAnchor {
                            va: anchor.va,
                            canonical: resolution.canonical_va(),
                            rule: resolution.rule,
                            stance: self.stance(anchor.va),
                        }
                    })
                    .collect(),
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_contract_never_changes_a_static_verdict() {
        let contract = SemanticContract::from_fact(
            String::from("x"),
            ContractSubject::AbiStanding,
            Some(0x0040_ccb0),
            String::from("graded"),
            &Fact::persisted(EvidenceLevel::Inferred, "p", 1u8),
            &[EngineComponent::DispatchTable],
        );
        assert!(contract.verdict_unchanged());
    }

    #[test]
    fn a_capped_contract_never_outranks_its_cap_and_leaves_the_source_alone() {
        let source = Fact::persisted(EvidenceLevel::Confirmed, "p", 1u8);
        let contract = SemanticContract::capped_from_fact(
            String::from("x"),
            ContractSubject::LifecycleRole,
            Some(1),
            String::from("role"),
            &source,
            LifecycleRole::ROLE_LEVEL,
            &[EngineComponent::RendererStartup],
        );
        assert_eq!(contract.level, EvidenceLevel::Inferred);
        assert_eq!(
            source.level(),
            EvidenceLevel::Confirmed,
            "the source envelope is untouched"
        );
    }

    #[test]
    fn a_contract_with_no_provenance_has_no_fact_rather_than_an_unknown_one() {
        let mut contract = SemanticContract::from_fact(
            String::from("x"),
            ContractSubject::AbiStanding,
            None,
            String::from("graded"),
            &Fact::persisted(EvidenceLevel::Inferred, "p", 1u8),
            &[EngineComponent::DispatchTable],
        );
        assert!(contract.to_fact().is_some());
        contract.provenance.clear();
        assert!(
            contract.to_fact().is_none(),
            "no source is not the same statement as an unknown claim"
        );
    }

    #[test]
    fn the_lifecycle_token_set_is_closed_and_matched_by_whole_segment() {
        for (token, kind) in LIFECYCLE_TOKENS {
            assert_eq!(
                lifecycle_token(token),
                Some((*token, *kind)),
                "{token} must resolve to itself"
            );
        }
        // `Init` and `Initialize` both sit in the set, so the set does contain a
        // prefix pair. Matching is whole-segment equality and nothing is derived
        // from a prefix: which is why these four name no role at all, and why
        // putting `Create` in the set would be a decision someone has to make
        // rather than a consequence of `Init` being there.
        for rejected in [
            "CreateResource",
            "CreateSpaceCommEvent",
            "InitializeRenderer",
            "Initialized",
            "OnExitNow",
            "Releases",
            "Get",
            "",
        ] {
            assert_eq!(
                lifecycle_token(rejected),
                None,
                "{rejected:?} must not resolve to a lifecycle role"
            );
        }
    }

    #[test]
    fn only_teardown_is_a_terminal_phase() {
        assert!(LifecyclePhase::Teardown.is_terminal());
        for phase in [
            LifecyclePhase::EnvironmentProbe,
            LifecyclePhase::AdapterSelection,
            LifecyclePhase::DeviceCreation,
            LifecyclePhase::CapabilityProbe,
            LifecyclePhase::ResourceCreation,
        ] {
            assert!(!phase.is_terminal(), "{phase} is not terminal");
        }
    }

    #[test]
    fn a_runtime_basis_must_be_portable() {
        let good = RuntimeBasis {
            observation: "an adapter was opened",
            artifact: "work/reports/startup-recovery88.json",
            level: EvidenceLevel::Observed,
        };
        assert!(good.is_portable());
        for artifact in ["/home/x/report.json", "C:\\reports\\r.json", ""] {
            assert!(
                !RuntimeBasis { artifact, ..good }.is_portable(),
                "{artifact:?} is not a portable reference"
            );
        }
    }

    #[test]
    fn a_not_recorded_abi_has_no_grade_no_provenance_and_a_reason() {
        let standing = AbiStanding::NotRecorded {
            reason: "no_evidence_pack".to_owned(),
        };
        assert_eq!(standing.level(), EvidenceLevel::Unknown);
        assert!(standing.provenance().is_empty());
        assert!(!standing.is_determined());
        assert_eq!(standing.convention(), None);
        assert_eq!(standing.reason(), "no_evidence_pack");
    }

    #[test]
    fn the_two_abi_abstentions_stay_distinct() {
        let undetermined = AbiStanding::Undetermined {
            level: EvidenceLevel::Unknown,
            verdict: Some("ABI_UNKNOWN".to_owned()),
            provenance: vec![Provenance::committed("pack")],
        };
        assert!(!undetermined.is_determined());
        assert_eq!(undetermined.reason(), crate::REASON_CONVENTION_UNDETERMINED);
        assert_eq!(
            undetermined.provenance().len(),
            1,
            "a record was read, so its provenance is carried"
        );
        let not_recorded = AbiStanding::NotRecorded {
            reason: "no_evidence_pack".to_owned(),
        };
        assert_ne!(undetermined.reason(), not_recorded.reason());
        assert!(
            undetermined.provenance().len() > not_recorded.provenance().len(),
            "one abstention read a record and the other did not"
        );
    }

    #[test]
    fn the_forbidden_token_list_catches_the_vocabulary_it_names() {
        for phrase in [
            "the renderer calls IDirect3D9::Release during teardown",
            "enumerate with EnumDisplayDevicesA before creating a device",
            "resolve the export with GetProcAddress first",
            "the device interface answers GetDeviceCaps at slot 6",
            "load the user32 module at startup",
            "create a window with CreateWindowExA, then a device context",
        ] {
            let lower = phrase.to_lowercase();
            assert!(
                FORBIDDEN_IN_STATEMENTS
                    .iter()
                    .any(|token| lower.contains(token)),
                "{phrase:?} should have been caught by the forbidden-token list"
            );
        }
        for phrase in [
            "an adapter handle is opened before any device is created",
            "capability answers shape the allocation budget",
            "a virtual call through slot 4 needs no ABI record",
        ] {
            let lower = phrase.to_lowercase();
            assert!(
                !FORBIDDEN_IN_STATEMENTS
                    .iter()
                    .any(|token| lower.contains(token)),
                "{phrase:?} is engine-level and must stay allowed"
            );
        }
    }
}
