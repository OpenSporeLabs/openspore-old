//! What this crate does **not** know.
//!
//! The repository's convention is that a format crate carries a `claims` module
//! listing its non-findings, and that claims are extended rather than deleted
//! when something is inconvenient. This one is long, and every entry is either
//! a limit of the *format*, a limit of *this reader*, or a deliberate refusal to
//! repair something upstream.
//!
//! Read it before trusting an answer. A `Fact::unavailable` says OpenSpore
//! looked and found nothing; the entries below say what "nothing" can mean.

/// Claim ids are stable so a doc or a test can refer to one without quoting it.
///
/// They are plain constants rather than an enum because an enum would have to be
/// exhaustive, and the whole point of this module is that it is not: the next
/// thing someone learns belongs here too.
pub mod claim {
    /// `classification.evidence` is not the shared 7-rung scale.
    ///
    /// The current snapshot writes `"APPROX"` on 1 500 records, where
    /// `spore_core::EvidenceLevel` spells it `APPROXIMATION`. Every other grade
    /// in all 58 757 records — envelope `evidence_level`, `abi.*.confidence`,
    /// `vtable.abi_attributed.confidence` — is on the scale; this one field is
    /// the single exception, measured over the whole file.
    ///
    /// **What this crate does:** keeps the spelling verbatim
    /// (`Classification::evidence`) and maps it through
    /// `EvidenceLevel::parse`, which degrades an unknown label to
    /// [`spore_core::EvidenceLevel::Unknown`].
    ///
    /// **Why not refuse the record:** because refusing would make 1 500 records
    /// unreadable over a spelling. **Why not map `APPROX` to `APPROXIMATION`:**
    /// because that mapping is not stated anywhere in the repository, and
    /// inventing it here would be a second opinion about the shared scale — the
    /// exact thing this crate exists not to be.
    pub const CLASSIFICATION_EVIDENCE_VOCABULARY: &str = "classification.evidence_vocabulary";

    /// The literal string `"Unknown"` is the triage classifier's own sentinel for
    /// "no subsystem", and it is not the exchange format's absence vocabulary.
    ///
    /// 49 340 of 58 757 records carry it, while `metadata.counts.with_subsystem`
    /// counts 58 757 — every record has a *subsystem field*, and most of them say
    /// `Unknown` in it.
    ///
    /// **What this crate does:** reports it verbatim through
    /// `Classification::subsystem` and offers
    /// `Classification::has_named_subsystem()` as a predicate. **Why not fold it
    /// into `Fact::unavailable`:** because folding a source's chosen value into
    /// the format's explicit-absence vocabulary would be deriving, and the two
    /// statements are not the same: "the classifier said Unknown" and "the
    /// classifier stated no subsystem" are different observations.
    pub const SUBSYSTEM_UNKNOWN_SENTINEL: &str = "subsystem_unknown_sentinel";

    /// `evidence.promotion_record` and `evidence.validation_report` carry a
    /// literal `00000000` in their paths.
    ///
    /// All 92 `promotion_record` values and all 672 `validation_report` values
    /// spell the directory `reconstruction/evidence/00000000/…` — an unformatted
    /// zero VA in an exporter that formats every other path in the file. The
    /// sibling `pack` field on the same records is correct
    /// (`reconstruction/evidence/0040ccb0/evidence.json`).
    ///
    /// **What this crate does:** carries the path verbatim and never offers it as
    /// a locatable address. **Why not repair it:** repairing it would mean
    /// guessing which of 92 VAs each path was meant to be, which is precisely
    /// the "derive nothing" rule. A consumer that needs the real directory should
    /// use `evidence.pack` or the passport's own VA.
    pub const PROMOTION_RECORD_ZERO_PATH: &str = "promotion_record_zero_path";

    /// `graph.fan_in` and `graph.fan_out` are declared by the schema and absent
    /// from every record in the current snapshot.
    ///
    /// The upstream docs call them triage's own counters and warn they "may
    /// legitimately differ" from the counts beside them, which come from the
    /// pinned xref export.
    ///
    /// **What this crate does:** declares both in the reader's schema (so a
    /// snapshot that starts emitting them is accepted) and exposes them as
    /// `Option<u32>`, *not* as a graded `Fact`. The format publishes no envelope
    /// and no `reason` for them, so publishing `Fact::unavailable` would invent an
    /// absence statement the artifact never made.
    pub const GRAPH_FAN_COUNTERS_ABSENT: &str = "graph_fan_counters_absent";

    /// `reconstruction.runtime_gated` is absent on 113 of the 727 records that
    /// carry a reconstruction block.
    ///
    /// **What this crate does:** reports [`crate::RuntimeGate::Unstated`]
    /// — a *stated* absence inside an available record, which is why it is not
    /// `Fact::unavailable`. Collapsing it into either `Open` or `Closed` would
    /// answer a question the file does not answer.
    pub const RUNTIME_GATE_UNSTATED: &str = "runtime_gate_unstated";

    /// Six fields are open-shaped by contract and are carried verbatim.
    ///
    /// `reconstruction.value.promotion_supersedes`,
    /// `reconstruction.value.runtime_blocking_reason`,
    /// `semantics.value.confidence`, `validation.static_rollup`,
    /// `names.identity_refuted.reason` and `names.identity_refuted.refuted`.
    /// Upstream these are `json.RawMessage` or `map[string]json.RawMessage`: the
    /// exporter stores them raw because their shape is not the format's to fix.
    ///
    /// **Consequence, stated plainly:** this reader validates that they are
    /// well-formed JSON and that their field *names* are known, but it cannot
    /// validate their contents. A schema change *inside* one of them is carried
    /// through untouched rather than refused. Everywhere the format is closed,
    /// the closure is total.
    pub const OPEN_SHAPED_SPANS: &str = "open_shaped_spans";

    /// The hand-authored ABI block is validated and then discarded.
    ///
    /// `abi.value.declared` is a human's prose: `"calling_convention":
    /// "__thiscall observed"`, `"return_type": "Transform*"`. The format's own
    /// rule is that it must never be mistaken for the machine-inferred
    /// `convention` beside it.
    ///
    /// **What this crate does:** declares all 20 of its fields, so an unknown
    /// field inside it is refused like any other, and then drops the values.
    /// Holding the prose at all would put a string like `"__thiscall observed"` one
    /// accessor away from the inferred `__thiscall`, and the only reliable way to
    /// keep the two apart is for one of them not to exist.
    pub const DECLARED_ABI_NOT_RETAINED: &str = "declared_abi_not_retained";

    /// Three absence reason codes are this crate's own, not the format's.
    ///
    /// The format publishes a closed vocabulary of ten codes for the six
    /// enveloped groups. It publishes **nothing** for its plain nullable fields,
    /// so grading them needs a code from somewhere. These three are marked
    /// bridge-local so no consumer mistakes them for format vocabulary:
    /// [`crate::REASON_NO_SDK_NAME`], [`crate::REASON_NO_SUBSYSTEM`] and
    /// [`crate::REASON_CONVENTION_UNDETERMINED`].
    ///
    /// A fourth, `"no_reconstruction_package"`, is *not* bridge-local: it is the
    /// format's own code, reused for a package that is `null` inside an available
    /// reconstruction record.
    pub const BRIDGE_LOCAL_REASON_CODES: &str = "bridge_local_reason_codes";

    /// A fourth bridge-local reason code, added with the contract layer.
    ///
    /// [`crate::REASON_NO_ABI_RECORD`] answers a question the format does not
    /// publish: the ABI group was unavailable, and the crate's own
    /// [`crate::AbiStanding`] wants to distinguish "no group" from "a group with
    /// no convention". The format's own code (`no_evidence_pack` and friends) is
    /// carried first in the reason a caller sees whenever the group published one;
    /// this code is the fallback.
    pub const BRIDGE_LOCAL_REASON_CODES_EXPANDED: &str = "bridge_local_reason_codes_expanded";

    /// The reader refuses an unsorted snapshot; the Go loader does not.
    ///
    /// `Load` accepts any order and leaves the diagnosis to `validate`, because
    /// it indexes by hash map. This crate's interior-resolution rule bisects on
    /// entry starts, so an unsorted index would answer a different question
    /// silently. It therefore fails closed at open.
    ///
    /// Same reasoning, same direction, for a **repeated JSON key** and for a
    /// **missing required field**: Go's decoder defaults them (`""`, `0`, last
    /// wins), and each of those defaults is a zero value standing in for something
    /// the file did not say.
    pub const STRICTER_THAN_THE_GO_LOADER: &str = "stricter_than_the_go_loader";

    /// The reader requires `len(callers) == caller_count` and
    /// `len(callees) + len(external_callees) == callee_count`.
    ///
    /// Both hold on all 58 757 records of the current snapshot. The format does
    /// not state the invariant, but without it a `caller_count` of 0 next to a
    /// non-empty list — or a count that no longer matches — would read as "no
    /// callers" when it means something else. Enforcing an unstated invariant is a
    /// judgement call; it is recorded here so a future snapshot that breaks it
    /// produces a diagnosis rather than a surprise.
    pub const COUNT_MATCHES_LIST_INVARIANT: &str = "count_matches_list_invariant";

    /// `graph.callers` / `graph.callees` / `vtable.triage_addresses` are read as
    /// canonical `"0x%08x"`.
    ///
    /// The upstream field table describes `vtable.triage_addresses` as "bare
    /// 8-hex". All 19 646 emitted values are `0x`-prefixed lowercase, so this
    /// reader follows the bytes rather than the prose. A bare `013f05d8` is
    /// refused, which is the same rule that keeps one address from becoming two
    /// entries.
    pub const VA_SPELLING_FOLLOWS_THE_BYTES: &str = "va_spelling_follows_the_bytes";

    /// Runtime overlays are **not read** by this crate.
    ///
    /// OpenSpore is the *static* side of the exchange. A runtime overlay is the
    /// consumer's own artifact (`spore-semantic-runtime-overlay-1`), the two are
    /// joined as a view, and the format has deliberately no `merge` command — a
    /// command named "merge" invites a caller to produce a combined artifact and
    /// trust it as one source of truth. See `docs/tooling/semantic-exchange.md`
    /// §6.3.
    ///
    /// The mirror of the runtime rule *is* enforced here: a static record whose
    /// document-level `provenance[].source_class` claims `runtime` is refused,
    /// because a static artifact may not wear a runtime file's vocabulary.
    pub const RUNTIME_OVERLAYS_NOT_READ: &str = "runtime_overlays_not_read";

    /// The record's own `identity.size` is the only size the resolution rule
    /// uses, and `size_known` is honoured.
    ///
    /// `size_known` is `true` on every record of the current snapshot, so the
    /// rule always has a size. If a future snapshot stated none, interior
    /// resolution returns `NonFunctionEntity` for everything rather than
    /// pretending a zero-length body contains nothing.
    pub const SIZE_GATING: &str = "size_gating";

    /// Nothing here is derived from the binary.
    ///
    /// There is no engine-side inference of any kind in this crate: no call-graph
    /// completion, no convention inference, no vftable reconstruction, no
    /// validation recomputation. Where the passport is silent, this crate is
    /// silent in the same way, and the *reason code* it reports is the exporter's.
    pub const DERIVES_NOTHING: &str = "derives_nothing";
    /// No input the reader did not itself construct makes it panic.
    ///
    /// Every parse path ends in a typed error: malformed JSON, a blank line, a
    /// truncated line, a scalar where an object is required, an unknown field, a
    /// missing required field, a repeated key, a non-canonical VA, an off-scale
    /// grade, out-of-range nesting, an oversized line, a duplicate canonical VA,
    /// unsorted records, a digest mismatch. The three library panics that do exist
    /// are unreachable by construction: two `expect`s over eight validated hex
    /// digits, and `spore_core::Fact::new`'s assertion against a provenance list that
    /// is a non-empty constant at every call site in this crate.
    pub const NO_PANICS_ANY_INPUT: &str = "no_panics_any_input";

    /// One digest implementation, one definition of "the record lines".
    ///
    /// `metadata.content_sha256` is SHA-256 over the concatenation of the **function
    /// record lines only**, each including its `\n`. Line 1 is excluded, which is what
    /// makes the field non-self-referential and lets a consumer check binary identity
    /// before parsing 58 757 records.
    pub const SINGLE_CONNECTION_DIGEST: &str = "single_connection_digest";

    /// Two upstream maps are genuinely open and are read as such.
    ///
    /// `validation.coverage` is `map[string]ValidationCheck` and
    /// `semantics.value.confidence` is `map[string]json.RawMessage`. A new validation
    /// dimension therefore appears in [`crate::ValidationSummary`]
    /// instead of being refused — while every verdict inside it is still validated in
    /// full, and a wrong shape in one is still a schema error.
    pub const OPEN_MAPS_NOT_CLOSED: &str = "open_maps_not_closed";

    /// The contract layer states **no platform API and no interface name**.
    ///
    /// The rules in [`crate::invariants`] were distilled from a runtime run of the
    /// original binary, and that run went through a graphics API OpenSpore does
    /// not implement. Naming one would turn an engine requirement into an
    /// instruction to reproduce an API this project does not target.
    ///
    /// **What this crate does:** states each rule in engine terms, and refuses a
    /// leaked name mechanically — [`crate::FORBIDDEN_IN_STATEMENTS`] is matched
    /// against every statement and every anchor note by
    /// [`crate::SemanticInvariant::coherence`]. The vocabulary those observations
    /// arrived in is recorded in `docs/SEMANTIC-CONTRACTS.md` as *provenance*, in
    /// the place that is allowed to be specific.
    pub const CONTRACTS_ENCODE_NO_PLATFORM_API: &str = "contracts_encode_no_platform_api";

    /// A runtime observation enters as a **reference**, never as a parse.
    ///
    /// [`crate::RuntimeBasis`] carries a portable artifact id and the observation
    /// restated in engine terms. It does not open a producer's report, and this
    /// crate compiles and tests with no runtime producer present at all: the
    /// correct flow is producer → runtime evidence → `spore-semantic` → semantic
    /// contract → OpenSpore, and the contract is the last step that still depends
    /// on nothing but a documented artifact id.
    ///
    /// The overlay *join* stays where it belongs, in `spore-semantic runtime`
    /// (see [`RUNTIME_OVERLAYS_NOT_READ`]): re-implementing it here would be a
    /// second answer to the same question in a second language.
    pub const RUNTIME_BASIS_IS_A_REFERENCE_NOT_A_PARSE: &str =
        "runtime_basis_is_a_reference_not_a_parse";

    /// A contract's consumer is an **identifier**, and nothing is wired.
    ///
    /// [`crate::EngineComponent`] names a workspace path, because this crate is
    /// Bevy-free and cannot name a Bevy type. Every entry in the catalog has at
    /// least one consumer, which is what makes "a contract with no consumer is a
    /// note" checkable — but no engine system reads any of it yet, and
    /// `docs/SEMANTIC-CONTRACTS.md` says per rule what the assertion would be.
    /// Reporting otherwise would be a second claim with no consumer.
    pub const CONTRACT_CONSUMERS_ARE_NOT_YET_WIRED: &str = "contract_consumers_are_not_yet_wired";

    /// A lifecycle role read off a name token is `INFERRED`, whatever the name's
    /// own grade is.
    ///
    /// 1 171 of the committed snapshot's records carry an SDK name and 1 167 of
    /// those are graded `CONFIRMED` (one `SUPPORTED`, three `UNKNOWN`). A
    /// `CONFIRMED` *name* does not make "this function is a teardown step" a
    /// confirmed claim, so
    /// [`crate::LifecycleRole::ROLE_LEVEL`] caps the role at
    /// [`spore_core::EvidenceLevel::Inferred`] and
    /// [`crate::SemanticContract::capped_from_fact`] is the only constructor that
    /// lowers a grade.
    ///
    /// The token set is **exact**, closed at seven tokens over 55 records, with no
    /// prefix matching. A `Create\w*` prefix would add 38 records, among them
    /// `CreateSpaceCommEvent` and `CreateCivCommEvent`, which create no resource
    /// at all — a role read off a prefix would be wrong for them and would read
    /// exactly as confidently as the ones it got right.
    pub const LIFECYCLE_ROLE_IS_INFERRED_FROM_A_NAME: &str =
        "lifecycle_role_is_inferred_from_a_name";

    /// "The passport is silent about this address" is computed by a **stated
    /// rule**, not asserted in prose.
    ///
    /// [`crate::ContractLedger::states_anything`] defines it: a record *states*
    /// something when it carries an SDK name, an ABI record, a table membership, a
    /// semantic record or a reconstruction record; all five absent is an explicit
    /// silence, which is a different statement from having no record at all. It is
    /// this crate's rule, not the format's, and it is the predicate behind
    /// [`crate::PassportStance::ExplicitlySilent`] — which is why the rule is a
    /// public function rather than a private detail.
    pub const PASSPORT_SILENCE_IS_COMPUTED_BY_A_STATED_RULE: &str =
        "passport_silence_is_computed_by_a_stated_rule";

    /// A measurement from one environment is evidence of a **shape**, not of a
    /// value.
    ///
    /// The run behind `device_memory_budget_is_a_policy_input` read an
    /// available-memory figure that, in that environment, was a saturated 32-bit
    /// wrap. The producer deliberately did not copy it, because adopting it would
    /// encode one machine's arithmetic as the program's behaviour. This crate
    /// records the shape — a byte count, consumed after the device exists — and
    /// states no number, so the same rule holds whatever the next run sees.
    pub const ENVIRONMENT_MEASUREMENTS_ARE_NOT_ENGINE_BEHAVIOUR: &str =
        "environment_measurements_are_not_engine_behaviour";

    /// No contract in the catalog is `CONFIRMED` or `VERIFIED`.
    ///
    /// Every entry's `level` is bounded by its `basis`
    /// ([`crate::InvariantCore::grade_is_coherent`], checked by
    /// [`crate::SemanticInvariant::coherence`]), and no observation in this
    /// catalog came from a repeatable differential against the original binary —
    /// which is what `VERIFIED` means in this repository's vocabulary. Nine
    /// entries are `OBSERVED` observations generalised to `INFERRED` rules and one
    /// is `UNKNOWN` on both rungs. Raising a grade here would need a differential,
    /// not an argument.
    pub const CONTRACT_GRADES_ARE_CAPPED_AND_LOW: &str = "contract_grades_are_capped_and_low";
}

/// Every claim id, for a caller that wants to enumerate this crate's non-findings
/// (a `--list-claims` style call, or a report that cites them by id).
pub const ALL_CLAIMS: [&str; 24] = [
    claim::CLASSIFICATION_EVIDENCE_VOCABULARY,
    claim::SUBSYSTEM_UNKNOWN_SENTINEL,
    claim::PROMOTION_RECORD_ZERO_PATH,
    claim::GRAPH_FAN_COUNTERS_ABSENT,
    claim::RUNTIME_GATE_UNSTATED,
    claim::OPEN_SHAPED_SPANS,
    claim::DECLARED_ABI_NOT_RETAINED,
    claim::BRIDGE_LOCAL_REASON_CODES,
    claim::STRICTER_THAN_THE_GO_LOADER,
    claim::COUNT_MATCHES_LIST_INVARIANT,
    claim::VA_SPELLING_FOLLOWS_THE_BYTES,
    claim::RUNTIME_OVERLAYS_NOT_READ,
    claim::SIZE_GATING,
    claim::DERIVES_NOTHING,
    claim::NO_PANICS_ANY_INPUT,
    claim::SINGLE_CONNECTION_DIGEST,
    claim::OPEN_MAPS_NOT_CLOSED,
    claim::CONTRACTS_ENCODE_NO_PLATFORM_API,
    claim::RUNTIME_BASIS_IS_A_REFERENCE_NOT_A_PARSE,
    claim::CONTRACT_CONSUMERS_ARE_NOT_YET_WIRED,
    claim::LIFECYCLE_ROLE_IS_INFERRED_FROM_A_NAME,
    claim::PASSPORT_SILENCE_IS_COMPUTED_BY_A_STATED_RULE,
    claim::ENVIRONMENT_MEASUREMENTS_ARE_NOT_ENGINE_BEHAVIOUR,
    claim::CONTRACT_GRADES_ARE_CAPPED_AND_LOW,
];

#[cfg(test)]
mod tests {
    use super::{claim, ALL_CLAIMS};

    #[test]
    fn every_claim_is_listed_exactly_once_and_names_something() {
        let mut seen = std::collections::BTreeSet::new();
        for id in ALL_CLAIMS {
            assert!(!id.is_empty(), "a claim id must be nameable");
            assert!(seen.insert(id), "{id} is listed twice in ALL_CLAIMS");
        }
        // Every module-level constant is reachable and non-empty.
        for id in [
            claim::CLASSIFICATION_EVIDENCE_VOCABULARY,
            claim::SUBSYSTEM_UNKNOWN_SENTINEL,
            claim::PROMOTION_RECORD_ZERO_PATH,
            claim::GRAPH_FAN_COUNTERS_ABSENT,
            claim::RUNTIME_GATE_UNSTATED,
            claim::OPEN_SHAPED_SPANS,
            claim::DECLARED_ABI_NOT_RETAINED,
            claim::BRIDGE_LOCAL_REASON_CODES,
            claim::STRICTER_THAN_THE_GO_LOADER,
            claim::COUNT_MATCHES_LIST_INVARIANT,
            claim::VA_SPELLING_FOLLOWS_THE_BYTES,
            claim::RUNTIME_OVERLAYS_NOT_READ,
            claim::SIZE_GATING,
            claim::DERIVES_NOTHING,
            claim::NO_PANICS_ANY_INPUT,
            claim::SINGLE_CONNECTION_DIGEST,
            claim::OPEN_MAPS_NOT_CLOSED,
            claim::CONTRACTS_ENCODE_NO_PLATFORM_API,
            claim::RUNTIME_BASIS_IS_A_REFERENCE_NOT_A_PARSE,
            claim::CONTRACT_CONSUMERS_ARE_NOT_YET_WIRED,
            claim::LIFECYCLE_ROLE_IS_INFERRED_FROM_A_NAME,
            claim::PASSPORT_SILENCE_IS_COMPUTED_BY_A_STATED_RULE,
            claim::ENVIRONMENT_MEASUREMENTS_ARE_NOT_ENGINE_BEHAVIOUR,
            claim::CONTRACT_GRADES_ARE_CAPPED_AND_LOW,
        ] {
            assert!(!id.is_empty());
            assert!(
                ALL_CLAIMS.contains(&id),
                "{id} exists but is not in ALL_CLAIMS, so a caller enumerating the list would \
                 miss it"
            );
        }
        assert_eq!(ALL_CLAIMS.len(), 24);
    }
}
