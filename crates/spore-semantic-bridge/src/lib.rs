//! # `spore-semantic-bridge` — the read-only Rust consumer of the passport
//!
//! One JSON Lines file, `knowledge/semantic/function-passport-v1.jsonl` (58 757
//! records, ~98 MB), carries everything OpenSpore knows about every function in
//! `SporeApp.exe`. This crate is the Rust side of that boundary: it lets engine
//! code ask a passport *"what is at this address, how strongly is that known,
//! and where did the claim come from?"* with the provenance rules intact.
//!
//! ```text
//!   tools/spore-semantic (Go, the producer)
//!          │  exports facts copied out of the research corpus
//!          v
//!   function-passport-v1.jsonl      <-- the ONLY contract
//!          │
//!   spore-semantic-bridge          <-- THIS CRATE: strict streaming reader
//!          │
//!          ├── contracts             <-- engine-facing claims, graded and sourced
//!          └── invariants            <-- the committed rule catalog
//!          │
//!          v
//!   spore-engine (not wired yet: docs/SEMANTIC-CONTRACTS.md says what each
//!                rule would assert and where)
//! ```
//!
//! # Contracts: what an engine author reaches for
//!
//! The passport answers "what is at this address". An engine needs three more
//! answers, and [`contracts`] supplies them two honest ways: a **committed
//! contract** — a rule with a grade, a provenance and a named consuming
//! component — and a **projection** of already-graded facts into engine
//! questions ([`ContractLedger`]). Neither invents a fact the source did not
//! state, and [`SemanticInvariant::coherence`] is the machine-checked list of
//! what a committed rule must satisfy before it may exist: a consumer, a source,
//! a legal grade, and no platform API name.
//! # What this crate does not do
//!
//! **It derives nothing.** The rule the whole exchange rests on is: *if a fact
//! already exists in an OpenSpore artifact, copy it with its provenance; if it
//! does not exist, say `UNKNOWN` — never guess, never infer, never silently
//! drop.* So there is no ABI inference here, no vftable detection, no validation
//! recomputation, no frontier scoring. Each of those lives upstream, and a second
//! implementation here would be a second answer to the same question.
//!
//! A missing fact is therefore a **first-class answer**, not an absent field:
//! [`Fact`] from `spore-core` carries `unavailable` with the reason code the
//! exporter published, and this crate invents no zero values, no empty
//! collections and no `false`.
//!
//! # It writes nothing
//!
//! The crate opens files read-only and has no code path that creates, modifies or
//! deletes one. That is a contract, not an accident: OpenSpore is the producer
//! and this is a consumer, and a reader that can write is a reader whose answers
//! cannot be trusted about which bytes it read.
//!
//! # The artifact is optional
//!
//! Nothing in the workspace depends on this crate at runtime, and nothing in this
//! crate depends on the artifact existing. [`Snapshot::open_committed`] returns
//! [`BridgeError::SnapshotUnavailable`] when there is no snapshot, which is the
//! optionality expressed as a type. Every test in this crate that reads the
//! committed 98 MB file skips, loudly, if the file is absent.
//!
//! # No Bevy, no GPU, no game install
//!
//! Dependencies: `spore-core` and `thiserror`. `spore-core` is Bevy-free and
//! I/O-free by construction, so neither is this crate — and neither is anything
//! that uses it. `#![forbid(unsafe_code)]` holds crate-wide, including the
//! hand-rolled SHA-256 and the hand-rolled JSON reader.
//!
//! # Strictness at the boundary
//!
//! The consumer contract says a record carrying fields this build does not know
//! is **refused**, not half-read: *"a consumer must not read a future schema as
//! if it were today's."* So the reader is schema-driven and it refuses:
//!
//! * an unknown field, naming its dotted path (`vtable.value.memberships[3].slot`);
//! * a missing required field, rather than letting it become a zero value;
//! * a repeated JSON key, rather than letting the last one win silently;
//! * a `null` where a value is required;
//! * a canonical VA spelled anything but lowercase `0x%08x`;
//! * an off-scale evidence grade, rather than degrading it into a lower rung;
//! * a `source_class` outside the static vocabulary, `runtime` included.
//!
//! [`BridgeError`] keeps **corruption** and **unsupported schema** apart, which
//! is the format's own distinction (Go exit 5 versus exit 6).
//!
//! # Streaming, and why
//!
//! The file is read in a streaming fashion and never `read_to_string`-ed. See
//! [`snapshot`] for the index layout, the peak cost of `open` on the real file,
//! and why `lookup` returns an owned `Passport`.
//!
//! # Module map
//!
//! | module | role |
//! |---|---|
//! | [`address`] | `Va`, and the canonical spelling rule |
//! | [`snapshot`] | [`Snapshot`], the streaming index, canonicalization, [`Verification`] |
//! | [`passport`] | [`Passport`] and the engine-facing accessors |
//! | [`metadata`] | line 1: binary identity, input digests, counters |
//! | [`contracts`] | [`ContractLedger`] and the contract types an engine asserts |
//! | [`invariants`] | the committed rule catalog, each entry graded and sourced |
//! | [`claims`] | **what this crate does not know** — read it before trusting an answer |
//! | `envelope`, `json`, `schema`, `sha256` | the strict reader, private |

#![forbid(unsafe_code)]
#![warn(missing_docs, missing_debug_implementations)]

pub mod address;
pub mod claims;
pub mod contracts;
pub mod error;
pub mod invariants;
pub mod metadata;
pub mod passport;
pub mod snapshot;

mod envelope;
mod json;
mod schema;
mod sha256;

pub use address::{format_va, AddressError, Va};
pub use contracts::{
    AbiStanding, AddressContracts, AuditedAnchor, BehavioralInvariant, ContractLedger,
    ContractSubject, DispatchContract, EngineComponent, EvidenceAnchor, InvariantAudit,
    InvariantCore, LifecycleContract, LifecyclePhase, LifecycleRole, LifecycleRoleKind,
    PassportStance, PhaseOrderViolation, ProbedInput, ResourceContract, ResourceDecision,
    RuntimeBasis, RuntimeGateContract, SemanticContract, SemanticInvariant,
    FORBIDDEN_IN_STATEMENTS, REASON_NO_ABI_RECORD,
};
pub use error::BridgeError;
pub use invariants::{all_invariants, invariant, ARTIFACT_MAIN_LOOP, DOCS_REFERENCE};
pub use metadata::{AbsentInput, Counts, ExportNotes, Generator, InputDigest, Metadata};
pub use passport::{
    AbiRecord, AdditionalPack, AddressResolution, Canonicalization, Classification, Cleanup,
    EvidenceLocations, Graph, Identity, Passport, RawJson, Receiver, ReconstructionRecord,
    RecordedResolution, RefutedIdentity, RuntimeGate, SemanticClaim, SemanticsRecord, Sret,
    TriState, VTableMembership, VTableRecord, ValidationCheck, ValidationSummary,
    REASON_CONVENTION_UNDETERMINED, REASON_NO_SDK_NAME, REASON_NO_SUBSYSTEM, SNAPSHOT_FILE_NAME,
    SNAPSHOT_REFERENCE, SNAPSHOT_SCHEMA,
};
pub use snapshot::{
    committed_snapshot_path, BinaryIdentity, PassportCursor, Snapshot, Verification,
};

/// The evidence vocabulary, re-exported so a consumer does not have to name
/// `spore-core` itself to hold a graded fact.
///
/// These are **not** a second vocabulary: they are the repository's shared types,
/// and this crate derives none of its own.
pub use spore_core::{
    EvidenceLevel, EvidenceState, Fact, FactState, Provenance, ProvenanceMode, SourceClass,
};
