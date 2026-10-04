//! The resource dependency graph: real edges between real records.
//!
//! # Why this layer exists
//!
//! The engine so far resolves **one record at a time**: hand it a
//! [`ResourceKey`], get meshes or a raster. That is the right shape for a
//! viewport and the wrong shape for a content package, because real Spore
//! content is a graph. A creature model names its textures inside its own
//! bytes; those textures name nothing; the model also names four to twenty-four
//! `rw4` records it cannot be drawn without; and beside every model sit a
//! handful of metadata records that share its instance id without any record
//! anywhere naming them. Nothing in a single-record load can see that.
//!
//! So this module builds the edges, states **how each kind was established**,
//! grades every one, and reports the ones that dangle.
//!
//! ```text
//!   package -> resource key -> model -> geometry -> material -> raster
//!                        \-> animation \-> metadata \-> dependencies
//! ```
//!
//! # The edge kinds, and what backs each
//!
//! | kind | source -> target | established by | grade |
//! |---|---|---|---|
//! | [`EdgeKind::GmdlReference`] | `gmdl` -> `rw4` | the `refCount` key table, read verbatim | [`EvidenceLevel::Verified`] |
//! | [`EdgeKind::GmdlTexture`] | `gmdl` -> `raster` | the `0x20D` texture-set entries | [`EvidenceLevel::Observed`] |
//! | [`EdgeKind::ModelTextureSet`] | `gmdl` -> `raster` | category byte + instance co-membership | [`EvidenceLevel::Inferred`] |
//! | [`EdgeKind::SiblingRecord`] | any -> any | group-family + instance co-membership | [`EvidenceLevel::Inferred`] |
//!
//! The two `gmdl` kinds are **references the record itself makes**: they are
//! read out of the record's bytes, and the grade difference between them is
//! deliberate. `GmdlTexture` is `Observed` because the texture-set entry stores
//! only an `(instance, group)` pair and the *type* comes from context — the
//! `raster` type id is supplied here, not read. `GmdlReference` is `Verified`
//! because its entry stores a complete `(type, group, instance)` triple that
//! was read and then resolved against the real index.
//!
//! The bottom two kinds are **not references**. They are adjacency: two records
//! that share a naming convention, with no record anywhere naming the other.
//! They are graded `Inferred` for exactly that reason, and [`GraphOptions`]
//! leaves the weakest of them off by default.
//!
//! # The one thing to read before trusting `GmdlReference`
//!
//! **The gmdl reference-count key table is big-endian in all three words, and
//! `spore_gmdl` reads it little-endian.**
//!
//! The `refCount` word itself being big-endian is already established in this
//! repository. The *table behind it* is a separate claim, and the corpus settles
//! it outright. Measured over the 4 209 `gmdl` records of
//! `Spore_Content.package` (7 989 key entries, 1 510 records carrying at least
//! one key):
//!
//! * reading all three words **big-endian** resolves **7 989 of 7 989** declared
//!   keys (6 076 distinct edges once repeats collapse) to index rows that exist;
//! * reading them little-endian resolves **0 of 7 989**.
//!
//! A byte order that produces three exact `(type, group, instance)` matches per
//! entry, thousands of times, against real rows, is not an inference. So this
//! module reads the table itself, big-endian.
//!
//! **The divergence is closed.** This paragraph used to say that
//! `spore_gmdl::GmdlModel::referenced_files` carried byte-swapped keys because
//! that crate read the *count* word big-endian but the three *key* words
//! little-endian -- a second, quieter bug in the same structure, since the walk
//! consumed the right byte count and produced plausible keys addressing nothing.
//! `spore-gmdl` now reads the whole table big-endian and both readers agree. The
//! byte-reversed reading is pinned by a test rather than merely described, so the
//! plausible-looking wrong order cannot come back unnoticed.
//!
//! Note the asymmetry this creates, because it is real and not a slip: the
//! *reference* table is big-endian while the *texture-set* table in the same
//! record is little-endian. Both were measured independently; see
//! [`claims::ENDIANNESS_ASYMMETRY`].
//!
//! # A dangling edge is a finding, not an error
//!
//! Real content points at records that are not there. `Spore_Content.package`
//! holds 150 `gmdl` records whose texture-set names group `0x00000000` and
//! instance `0x552a5077` — an instance that appears **nowhere** in the 17 119-row
//! index. Those edges are kept, counted and reported by
//! [`ResourceGraph::missing_references`]. Dropping them would turn "the record
//! points at something we cannot find" into "the record points at nothing", and
//! that is the failure mode this repository's evidence rules exist to prevent.
//!
//! # Determinism
//!
//! Every container is a [`BTreeMap`] or a sorted `Vec`, so two builds over the
//! same packages produce byte-identical traversal output. Nothing iterates a
//! hash map.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use spore_core::record::type_id;
use spore_core::{EvidenceLevel, Provenance, ResourceKey};

use crate::store::ContentStore;

/// The claims this module makes that are **not** verified, with their grade.
///
/// Mirrors the `claims` module every format crate in this repository carries.
/// Kept here rather than in `spore-gmdl` because these are claims about *edges*,
/// which no format crate owns.
pub mod claims {
    use super::*;
    use spore_core::evidence::{EvidenceState, Fact};

    /// The byte order of the `refCount` key table, and the divergence from
    /// `spore_gmdl`.
    pub const REFERENCE_TABLE_ENDIANNESS: &str = "reference_table_endianness";

    /// Why the reference table is big-endian while the texture-set table is not.
    pub const ENDIANNESS_ASYMMETRY: &str = "endianness_asymmetry";

    /// What the type word of a `refCount` key entry names. Measured to be `rw4`
    /// on 7 989 of 7 989 entries, but the *role* is not established.
    pub const REFERENCE_TABLE_ROLE: &str = "reference_table_role";

    /// Whether "same category byte, same instance id" means the raster belongs
    /// to the model.
    pub const CO_MEMBERSHIP_MEANS_OWNERSHIP: &str = "co_membership_means_ownership";

    /// The type a `0x20D` texture-set entry names. The entry stores no type word.
    pub const TEXTURE_ENTRY_TYPE: &str = "texture_entry_type";

    /// Why `pollen_metadata` contributes no edge, despite carrying what looks like
    /// a resource key.
    pub const POLLEN_METADATA_SELF_IDENTITY: &str = "pollen_metadata_self_identity";

    /// Why `prop` contributes no edge.
    pub const PROP_LAYOUT_UNRESOLVED: &str = "prop_layout_unresolved";

    /// One recorded claim: what is claimed, how strongly, and on what basis.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct GraphClaim {
        /// Stable identifier, e.g. `"reference_table_endianness"`.
        pub subject: &'static str,
        /// The claim in one sentence, in the repository's evidence vocabulary.
        pub statement: &'static str,
        /// The envelope: grade, how it was obtained, and where it came from.
        pub fact: Fact<&'static str>,
    }

    fn claim_of(
        subject: &'static str,
        statement: &'static str,
        level: EvidenceLevel,
        basis: &'static str,
    ) -> GraphClaim {
        GraphClaim {
            subject,
            statement,
            fact: Fact::new(
                level,
                EvidenceState::Derived,
                vec![Provenance::derived(basis)],
                statement,
            ),
        }
    }

    /// All recorded claims, in declaration order.
    pub fn all_claims() -> &'static [GraphClaim] {
        static CLAIMS: std::sync::OnceLock<Vec<GraphClaim>> = std::sync::OnceLock::new();
        CLAIMS.get_or_init(|| {
            vec![
                claim_of(
                    REFERENCE_TABLE_ENDIANNESS,
                    "The gmdl refCount key table stores its three words big-endian, in the on-disk \
                     order {instance, group, type}. Measured over the 4209 gmdl records of \
                     Spore_Content.package: all 7989 declared keys (6076 distinct edges after \
                     repeats collapse) resolve to an existing index row read big-endian, and 0 of \
                     7989 resolve read little-endian. spore_gmdl reads this table little-endian and \
                     therefore yields byte-swapped keys on every record that has one; this module \
                     reads it correctly and does not treat the neighbour's output as evidence.",
                    EvidenceLevel::Verified,
                    "docs/RESOURCE-GRAPH.md",
                ),
                claim_of(
                    ENDIANNESS_ASYMMETRY,
                    "Within one gmdl record the refCount key table is big-endian and the 0x20D \
                     texture-set table is little-endian. Each was settled independently by resolving \
                     against the real index: 7989 of 7989 for the reference table read big-endian, \
                     and 11496 of 11628 distinct texture edges read little-endian with a type of \
                     raster (the remaining 132 name group 0x00000000, which resolves under no \
                     reading). The asymmetry is observed, not explained, and nothing in the record \
                     format accounts for it.",
                    EvidenceLevel::Verified,
                    "docs/RESOURCE-GRAPH.md",
                ),
                claim_of(
                    REFERENCE_TABLE_ROLE,
                    "Every one of the 7989 refCount key entries in Spore_Content.package names type \
                     0x2F4E681B (rw4), in four groups, and 7449 of the declared entries name group \
                     0x41606100 whose 591 records are named an average of 12.6 times each. What an \
                     rw4 record contributes to the model that names it is NOT established: spore_rw4 \
                     decodes only the section directory and no section payload, so nothing here \
                     claims a model is renderable from these edges, or that the reverse edge means \
                     anything at all.",
                    EvidenceLevel::Inferred,
                    "docs/RESOURCE-GRAPH.md",
                ),
                claim_of(
                    CO_MEMBERSHIP_MEANS_OWNERSHIP,
                    "For all 7 category bytes present in Spore_Content.package the set of instance ids \
                     under model groups equals the set under the matching raster groups exactly \
                     (44/44 cell, 97/97 creature, 164/164 building, 285/285 vehicle, 91/91 ufo, \
                     153/153 flora, 67/67 palette; 901 of 901 pairs). That equality is measured, and \
                     it yields 13682 model-to-raster edges because one instance carries several \
                     raster sub-blocks. That the raster sharing a model's instance is therefore \
                     *that model's* texture set is inferred from the naming convention alone: no \
                     record names the other. It is redundant wherever the model also carries \
                     explicit texture references, which is 4208 of the 4209 models.",
                    EvidenceLevel::Inferred,
                    "docs/RESOURCE-GRAPH.md",
                ),
                claim_of(
                    POLLEN_METADATA_SELF_IDENTITY,
                    "153 of the 1641 pollen_metadata records in Spore_Content.package -- exactly those \
                     in group 0x40666200 -- carry a big-endian 12-byte triple at byte offset 12 whose \
                     type word is flr (0x438F6347) and whose group and instance words are the \
                     record's OWN group and instance. 0 of the 153 resolve to a flr record in any \
                     installed package: Spore_Game and PatchData each hold exactly one flr, at \
                     0x40666200/0xDE727FA9, and no pollen record names it. A triple that identifies the \
                     record carrying it is not a reference, so no edge is derived. Reporting 153 \
                     dangling flr edges here would be a fabricated negative.",
                    EvidenceLevel::Observed,
                    "docs/RESOURCE-GRAPH.md",
                ),
                claim_of(
                    PROP_LAYOUT_UNRESOLVED,
                    "The 1162 prop records in Spore_Content.package are counted and known but contribute \
                     no edge. A 20-byte-entry reading of the key table fits 694 of them and resolves 0 \
                     of 1388 candidate keys, and the families the entries land in (0x4284b800 and \
                     neighbours) do not exist in the same package -- which is what a wrong stride looks \
                     like. No prop layout is established, so no reference is read out of one and no \
                     prop payload is walked at all.",
                    EvidenceLevel::Unknown,
                    "docs/RESOURCE-GRAPH.md",
                ),
                claim_of(
                    TEXTURE_ENTRY_TYPE,
                    "A 0x20D texture-set entry stores 16 skipped bytes then an instance and a group \
                     word, and no type word. The target type is supplied here as raster (0x2F4E681C) \
                     because 11496 of the 11628 distinct texture edges resolve to an existing raster \
                     row under that assumption. The 132 that resolve nowhere -- all of them naming \
                     group 0x00000000 and instance 0x552A5077, from three cell-stage model groups -- \
                     are reported as dangling rather than retyped or discarded.",
                    EvidenceLevel::Supported,
                    "docs/RESOURCE-GRAPH.md",
                ),
            ]
        })
    }

    /// One claim's envelope, or `None` when this build records no claim for
    /// `subject`.
    ///
    /// Absence is not `UNKNOWN`: it means "this build does not say", which is a
    /// different statement from "this build says it is unknown".
    pub fn claim(subject: &str) -> Option<Fact<&'static str>> {
        all_claims()
            .iter()
            .find(|entry| entry.subject == subject)
            .map(|entry| entry.fact.clone())
    }
}

/// What a graph edge asserts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum EdgeKind {
    /// A `gmdl` record's texture-set entry, naming a `raster`.
    ///
    /// Read out of the record's `0x20D` shader-data block. Grade:
    /// [`EvidenceLevel::Observed`] — the `(instance, group)` pair is read; the
    /// `raster` type is supplied by context.
    GmdlTexture,

    /// A `gmdl` record's `refCount` key table entry, naming a complete
    /// `(type, group, instance)` triple.
    ///
    /// Read out of the record, big-endian. Grade: [`EvidenceLevel::Verified`] —
    /// every entry over the real corpus resolves to an index row.
    GmdlReference,

    /// A `raster` record that shares a model record's category byte and instance
    /// id.
    ///
    /// Pure adjacency, never named by any record. Grade:
    /// [`EvidenceLevel::Inferred`].
    ModelTextureSet,

    /// A record that shares a group-family prefix and an instance id.
    ///
    /// Pure adjacency, never named by any record. Grade:
    /// [`EvidenceLevel::Inferred`]. Off by default; see [`GraphOptions`].
    SiblingRecord,
}

impl EdgeKind {
    /// Every kind, in declaration order. A closed vocabulary: a report iterates
    /// this, so an unlisted kind cannot hide.
    pub const ALL: [EdgeKind; 4] = [
        EdgeKind::GmdlTexture,
        EdgeKind::GmdlReference,
        EdgeKind::ModelTextureSet,
        EdgeKind::SiblingRecord,
    ];

    /// The canonical spelling used in reports and documents.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::GmdlTexture => "gmdl->texture",
            Self::GmdlReference => "gmdl->reference",
            Self::ModelTextureSet => "model->texture-set",
            Self::SiblingRecord => "group-sibling",
        }
    }

    /// Parses a canonical spelling. Unknown input becomes `None`.
    ///
    /// The vocabulary is closed, so an unrecognised name is a *refusal*, not a
    /// fallback: mapping it to some kind would let a typo silently build the
    /// wrong edges.
    pub fn parse(text: &str) -> Option<Self> {
        EdgeKind::ALL.into_iter().find(|kind| kind.as_str() == text)
    }

    /// The grade every edge of this kind carries.
    ///
    /// A kind has one grade. An edge does not get to be `Observed` on a good day
    /// and `Inferred` on another, because the grade describes *how the edge was
    /// established*, which is a property of the mechanism and not of the record.
    pub const fn evidence_level(self) -> EvidenceLevel {
        match self {
            Self::GmdlTexture => EvidenceLevel::Observed,
            Self::GmdlReference => EvidenceLevel::Verified,
            Self::ModelTextureSet => EvidenceLevel::Inferred,
            Self::SiblingRecord => EvidenceLevel::Inferred,
        }
    }

    /// Whether edges of this kind come from bytes inside the source record.
    ///
    /// `true` means some record names the target, so a dangling edge is a
    /// genuine missing reference. `false` means the edge is adjacency, so a
    /// dangling edge would mean the graph itself is inconsistent.
    pub const fn is_declared_reference(self) -> bool {
        matches!(self, Self::GmdlTexture | Self::GmdlReference)
    }
}

impl core::fmt::Display for EdgeKind {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One directed dependency: `from` needs `to`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Edge {
    /// What this edge asserts.
    pub kind: EdgeKind,
    /// The record that carries the dependency.
    pub from: ResourceKey,
    /// The record depended on.
    pub to: ResourceKey,
}

impl Edge {
    /// The grade this edge carries, which is its kind's grade.
    pub const fn level(&self) -> EvidenceLevel {
        self.kind.evidence_level()
    }

    /// A one-line rendering for reports: `kind from -> to`.
    pub fn describe(&self) -> String {
        format!("{} {} -> {}", self.kind, self.from, self.to)
    }
}

/// An edge whose target is in no loaded package.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DanglingReference {
    /// Which kind of reference failed to resolve.
    pub kind: EdgeKind,
    /// The record that named it.
    pub from: ResourceKey,
    /// The identity it named.
    pub to: ResourceKey,
}

/// Why a record that *might* have contributed edges contributed none.
///
/// This is the difference between "we looked and found nothing" and "we did not
/// look", and the graph keeps them apart because a decoder that collapses them
/// turns an unreadable record into a record with no dependencies.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkipReason {
    /// The record's bytes did not decode, so its reference tables were not
    /// reached. The message names the stage, as `spore_gmdl` reports it.
    WalkFailed {
        /// The decoding stage or error text, verbatim from the decoder.
        detail: String,
    },
    /// The record decoded, and the material-info table it reached declares no
    /// texture set at all. This is a *finding* -- but only when the walk reached
    /// the material info. A walk that stopped earlier cannot make this claim and
    /// reports [`SkipReason::WalkFailed`] instead.
    NoTextureSetDeclared,
    /// The material-info table was read but did not close, so the texture
    /// references emitted from this record are a prefix and not the whole set.
    ///
    /// Separate from [`SkipReason::WalkFailed`] because the edges *are* real --
    /// every one of them was bounds-checked out of the record -- and only their
    /// completeness is unknown. 619 records of `Spore_Content.package` stop at
    /// the undocumented shader-data id `0x218`, including every record of the
    /// creature-model group `0x40627100`.
    PartialTextureSet {
        /// The stage and error the walk stopped with, verbatim.
        detail: String,
    },
    /// The `refCount` key table could not be read, so the model contributed no
    /// [`EdgeKind::GmdlReference`] edges. The message names the condition.
    ///
    /// A separate variant from [`SkipReason::WalkFailed`] because the two are
    /// different failures with different fixes: one is the gmdl walk stopping
    /// inside the material-info table, the other is a header this crate reads on
    /// its own and could not.
    ReferenceTableUnreadable {
        /// The condition, verbatim from [`ReferenceTableError`].
        detail: String,
    },
}

impl SkipReason {
    /// The canonical spelling used in reports.
    pub fn as_str(&self) -> &str {
        match self {
            Self::WalkFailed { .. } => "walk-failed",
            Self::NoTextureSetDeclared => "no-texture-set-declared",
            Self::PartialTextureSet { .. } => "partial-texture-set",
            Self::ReferenceTableUnreadable { .. } => "reference-table-unreadable",
        }
    }
}

/// What the graph could not derive edges from, per record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnresolvedRecord {
    /// The record.
    pub key: ResourceKey,
    /// Why it contributed nothing.
    pub reason: SkipReason,
}

/// Which edge kinds to build.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GraphOptions {
    /// Build [`EdgeKind::GmdlTexture`] edges. On by default.
    pub texture_edges: bool,
    /// Build [`EdgeKind::GmdlReference`] edges. On by default.
    pub reference_edges: bool,
    /// Build [`EdgeKind::ModelTextureSet`] adjacency. On by default.
    pub model_texture_sets: bool,
    /// Build [`EdgeKind::SiblingRecord`] adjacency. **Off by default.**
    ///
    /// Off because it is the weakest edge in the vocabulary: on the real corpus
    /// it produces roughly 5 400 undirected adjacencies across 824 shared
    /// instances, none of them named by any record, and it cannot dangle. It is
    /// here because a family inspector needs it, not because a loader does.
    pub sibling_records: bool,
}

impl Default for GraphOptions {
    fn default() -> Self {
        Self {
            texture_edges: true,
            reference_edges: true,
            model_texture_sets: true,
            sibling_records: false,
        }
    }
}

impl GraphOptions {
    /// Every edge kind on.
    pub fn all() -> Self {
        Self {
            sibling_records: true,
            ..Self::default()
        }
    }

    /// No edge kinds at all. A graph with zero edges is a valid graph, not an
    /// error: it says the index was read and nothing was derived from it.
    pub fn none() -> Self {
        Self {
            texture_edges: false,
            reference_edges: false,
            model_texture_sets: false,
            sibling_records: false,
        }
    }

    /// Whether `kind` will be built under these options.
    pub const fn builds(&self, kind: EdgeKind) -> bool {
        match kind {
            EdgeKind::GmdlTexture => self.texture_edges,
            EdgeKind::GmdlReference => self.reference_edges,
            EdgeKind::ModelTextureSet => self.model_texture_sets,
            EdgeKind::SiblingRecord => self.sibling_records,
        }
    }
}

/// One record reached by a traversal, with the depth at which it was reached.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct TraversalEntry {
    /// The record.
    pub key: ResourceKey,
    /// Hops from the traversal root; `0` for the root itself.
    pub depth: u32,
}

/// The resource dependency graph over one [`ContentStore`].
///
/// # Determinism
///
/// `outgoing` and `incoming` are keyed by [`ResourceKey`], whose ordering is
/// strict lexicographic on `(type, group, instance)`; each key's edge list is
/// sorted by `(kind, to)` and de-duplicated. Two builds over the same packages
/// therefore produce identical traversal output, which is the same discipline the
/// asset manifest is built on.
#[derive(Debug, Clone, Default)]
pub struct ResourceGraph {
    outgoing: BTreeMap<ResourceKey, Vec<Edge>>,
    incoming: BTreeMap<ResourceKey, Vec<Edge>>,
    /// Every record identity any loaded package holds.
    known: BTreeSet<ResourceKey>,
    /// Edges whose target is in no loaded package, keyed by kind.
    dangling: BTreeMap<EdgeKind, Vec<DanglingReference>>,
    /// Records that could not be read, and records that legitimately had nothing
    /// to declare. Kept apart on purpose.
    unresolved: Vec<UnresolvedRecord>,
    /// How many references were *declared* by records, per kind, counted before
    /// de-duplication.
    ///
    /// This is a different number from [`ResourceGraph::edge_count`] and the
    /// difference is a fact about the corpus, not an error: a record may name the
    /// same texture twice, and the graph stores one edge. Measured over
    /// `Spore_Content.package`: 16 628 texture-set entries collapse to 11 363
    /// distinct edges, and 7 989 reference-table keys to 6 076.
    declared_entries: BTreeMap<EdgeKind, usize>,
    /// Which edge kinds were built.
    options: GraphOptions,
}

impl ResourceGraph {
    /// Builds a graph over `store`, deriving every edge kind
    /// [`GraphOptions::default`] enables.
    ///
    /// Extracts and decompresses every `gmdl` record in the store -- 4209 of them
    /// out of the 17 119-row `Spore_Content.package`, whose image is memory-mapped,
    /// so only the touched pages become resident. No other payload is decoded:
    /// the `rw4` records this graph names are never opened, because no RW4 section
    /// payload is understood anywhere in this workspace.
    pub fn build(store: &ContentStore) -> Self {
        Self::build_with(store, GraphOptions::default())
    }

    /// Builds a graph over `store` with explicit options.
    pub fn build_with(store: &ContentStore, options: GraphOptions) -> Self {
        let mut graph = Self {
            options,
            ..Self::default()
        };
        graph.ingest_index(store);
        if options.texture_edges || options.reference_edges {
            graph.ingest_models(store);
        }
        if options.model_texture_sets {
            graph.ingest_model_texture_sets(store);
        }
        if options.sibling_records {
            graph.ingest_siblings();
        }
        graph.finish();
        graph
    }

    /// An empty graph, for tests and for a store with nothing in it.
    pub fn new() -> Self {
        Self::default()
    }

    /// Records every identity the store holds, without deriving edges.
    ///
    /// This is what makes a dangling edge meaningful: the set of things that
    /// *could* have resolved a reference, established from the index rather than
    /// from the edges that happened to resolve.
    fn ingest_index(&mut self, store: &ContentStore) {
        for package in store.packages() {
            for entry in package.index().entries() {
                self.known.insert(entry.key);
            }
        }
    }

    fn ingest_models(&mut self, store: &ContentStore) {
        for key in self.model_keys(store) {
            let bytes = match store.read(&key) {
                Ok(bytes) => bytes,
                Err(error) => {
                    self.skip(
                        key,
                        SkipReason::WalkFailed {
                            detail: error.to_string(),
                        },
                    );
                    continue;
                }
            };
            // The RECOVERING parse. 619 real records name a shader-data id with
            // no known size (0x218) and their material-info table stops there,
            // which means their texture refs are partial. That is reported, not
            // smoothed over.
            let recovered = match spore_gmdl::parse_recovering(&bytes) {
                Ok(recovered) => recovered,
                Err(error) => {
                    self.skip(
                        key,
                        SkipReason::WalkFailed {
                            detail: error.to_string(),
                        },
                    );
                    continue;
                }
            };
            let stopped_in_material_info = recovered
                .stop
                .as_ref()
                .is_some_and(|stop| stop.stage == spore_gmdl::WalkStage::MaterialInfo);
            if self.options.reference_edges {
                match read_reference_table(&bytes) {
                    Ok(references) => {
                        self.declare(EdgeKind::GmdlReference, references.len());
                        for target in references {
                            self.push(Edge {
                                kind: EdgeKind::GmdlReference,
                                from: key,
                                to: target,
                            });
                        }
                    }
                    // A table that could not be read is a non-finding, and it is
                    // recorded with the reason. Defaulting it to "this model
                    // references nothing" is the one answer that would be wrong.
                    Err(error) => self.skip(
                        key,
                        SkipReason::ReferenceTableUnreadable {
                            detail: error.to_string(),
                        },
                    ),
                }
            }

            if self.options.texture_edges {
                // Whatever texture refs were read are real: each entry is
                // bounds-checked as it is read. They are emitted even when the
                // walk stopped, because dropping validated edges loses something
                // known in exchange for something unknown.
                self.declare(EdgeKind::GmdlTexture, recovered.model.texture_refs.len());
                for reference in &recovered.model.texture_refs {
                    self.push(Edge {
                        kind: EdgeKind::GmdlTexture,
                        from: key,
                        // The type is supplied, not read: the entry stores no
                        // type word. See `claims::TEXTURE_ENTRY_TYPE`.
                        to: ResourceKey::new(
                            type_id::RASTER,
                            reference.group_id,
                            reference.instance_id,
                        ),
                    });
                }
                // How complete they are is a separate question from whether they
                // exist, and only a walk that closed on the record end can answer
                // it. 619 real records stop at the undocumented shader-data id
                // 0x218 with a partial material-info table behind them; calling
                // their neighbour list complete would be the exact collapse this
                // repository's evidence rules forbid.
                match (&recovered.stop, stopped_in_material_info) {
                    (None, _) => {
                        if recovered.model.texture_refs.is_empty() {
                            self.skip(key, SkipReason::NoTextureSetDeclared);
                        }
                    }
                    (Some(stop), true) => self.skip(
                        key,
                        SkipReason::PartialTextureSet {
                            detail: format!("{}: {}", stop.stage.as_str(), stop.error),
                        },
                    ),
                    (Some(stop), false) => self.skip(
                        key,
                        SkipReason::WalkFailed {
                            detail: format!("{}: {}", stop.stage.as_str(), stop.error),
                        },
                    ),
                }
            }
        }
    }

    /// Every model record identity in the store, ascending.
    fn model_keys(&self, store: &ContentStore) -> Vec<ResourceKey> {
        let mut keys: BTreeSet<ResourceKey> = BTreeSet::new();
        for package in store.packages() {
            for entry in package.index().of_type(spore_gmdl::GMDL_TYPE) {
                keys.insert(entry.key);
            }
        }
        let _ = store;
        keys.into_iter().collect()
    }

    /// `gmdl` -> `raster` adjacency by category byte and instance id.
    ///
    /// Measured on the real corpus: for all 7 category bytes the model instance
    /// set equals the raster instance set exactly (901 of 901). The equality is
    /// a measurement; treating it as ownership is an inference, which is why
    /// the edge is graded `Inferred` and why the direct
    /// [`EdgeKind::GmdlTexture`] edges exist alongside it.
    fn ingest_model_texture_sets(&mut self, store: &ContentStore) {
        // (category byte, instance) -> every raster identity under it.
        let mut rasters: BTreeMap<(u8, u32), Vec<ResourceKey>> = BTreeMap::new();
        for package in store.packages() {
            for entry in package.index().of_type(type_id::RASTER) {
                rasters
                    .entry((entry.key.category_byte(), entry.instance_id))
                    .or_default()
                    .push(entry.key);
            }
        }
        for model in self.model_keys(store) {
            let Some(matches) = rasters.get(&(model.category_byte(), model.instance_id)) else {
                continue;
            };
            self.declare(EdgeKind::ModelTextureSet, matches.len());
            for target in matches {
                self.push(Edge {
                    kind: EdgeKind::ModelTextureSet,
                    from: model,
                    to: *target,
                });
            }
        }
    }

    /// Same-family, same-instance adjacency in both directions.
    ///
    /// Family is `group_id & 0xFFFF_FF00`: the low byte of a group id is a
    /// sub-block index, so two records sharing the rest of the group id are in
    /// the same block family. Only instances that appear in more than one type
    /// produce an edge, and adjacency edges can never dangle — both endpoints
    /// came out of the index — which is why the dangling count for this kind is
    /// structurally zero and the tests assert exactly that.
    fn ingest_siblings(&mut self) {
        // family -> instance -> identities.
        let mut families: BTreeMap<u32, BTreeMap<u32, Vec<ResourceKey>>> = BTreeMap::new();
        for key in &self.known {
            families
                .entry(key.group_id & 0xFFFF_FF00)
                .or_default()
                .entry(key.instance_id)
                .or_default()
                .push(*key);
        }
        for (_, by_instance) in families {
            for identities in by_instance.values() {
                if identities.len() < 2 {
                    continue;
                }
                self.declare(EdgeKind::SiblingRecord, identities.len());
                for source in identities {
                    for target in identities {
                        if source == target || source.type_id == target.type_id {
                            continue;
                        }
                        self.push(Edge {
                            kind: EdgeKind::SiblingRecord,
                            from: *source,
                            to: *target,
                        });
                    }
                }
            }
        }
    }

    fn skip(&mut self, key: ResourceKey, reason: SkipReason) {
        self.unresolved.push(UnresolvedRecord { key, reason });
    }

    /// Records that `count` references of `kind` were declared by records, before
    /// de-duplication.
    fn declare(&mut self, kind: EdgeKind, count: usize) {
        *self.declared_entries.entry(kind).or_insert(0) += count;
    }

    /// Inserts one edge, keeping each adjacency list sorted and unique.
    fn push(&mut self, edge: Edge) {
        let bucket = self.outgoing.entry(edge.from).or_default();
        if bucket.binary_search(&edge).is_err() {
            bucket.push(edge);
            bucket.sort_unstable();
        }
        let bucket = self.incoming.entry(edge.to).or_default();
        if bucket.binary_search(&edge).is_err() {
            bucket.push(edge);
            bucket.sort_unstable();
        }
    }

    /// Resolves dangling references and sorts the unresolved list.
    fn finish(&mut self) {
        for edges in self.outgoing.values() {
            for edge in edges {
                if !self.known.contains(&edge.to) {
                    self.dangling
                        .entry(edge.kind)
                        .or_default()
                        .push(DanglingReference {
                            kind: edge.kind,
                            from: edge.from,
                            to: edge.to,
                        });
                }
            }
        }
        for list in self.dangling.values_mut() {
            list.sort_unstable();
            list.dedup();
        }
        self.unresolved.sort_by(|a, b| {
            a.key
                .cmp(&b.key)
                .then(a.reason.as_str().cmp(b.reason.as_str()))
        });
    }

    /// Which edge kinds this graph was built with.
    pub const fn options(&self) -> &GraphOptions {
        &self.options
    }

    /// How many record identities the loaded packages hold.
    ///
    /// This is the denominator for every dangling count: a reference is
    /// dangling because it is absent from *this* set, which is a statement about
    /// the loaded packages and not about the game.
    pub fn known_records(&self) -> usize {
        self.known.len()
    }

    /// Whether any loaded package holds `key`.
    pub fn contains(&self, key: &ResourceKey) -> bool {
        self.known.contains(key)
    }

    /// Every distinct identity the loaded packages hold, ascending.
    pub fn known_keys(&self) -> impl Iterator<Item = ResourceKey> + '_ {
        self.known.iter().copied()
    }

    /// The edges leaving `key`, ascending by `(kind, target)`.
    ///
    /// Empty for a key no loaded package holds — and empty is *not* "has no
    /// dependencies", it is "is not here". Use
    /// [`ResourceGraph::contains`] to tell the two apart.
    pub fn neighbours(&self, key: &ResourceKey) -> &[Edge] {
        self.outgoing.get(key).map_or(&[], Vec::as_slice)
    }

    /// The edges leaving `key` of one kind, ascending by target.
    pub fn neighbours_of_kind(&self, key: &ResourceKey, kind: EdgeKind) -> Vec<Edge> {
        self.neighbours(key)
            .iter()
            .copied()
            .filter(|edge| edge.kind == kind)
            .collect()
    }

    /// The edges arriving at `key`, ascending by `(kind, source)`.
    pub fn dependants(&self, key: &ResourceKey) -> &[Edge] {
        self.incoming.get(key).map_or(&[], Vec::as_slice)
    }

    /// Breadth-first walk from `key`, in deterministic order.
    ///
    /// * `depth` is a hard bound: nothing beyond it is returned. A depth of `0`
    ///   returns the root alone.
    /// * Cycle safety is a visited set, so a cycle yields its members once. The
    ///   graph contains cycles — two `gmdl` variants of one asset share
    ///   textures, and sibling adjacency is symmetric by construction — and a
    ///   traversal that could loop forever would be unusable.
    /// * `kinds` filters which edge kinds are followed. Empty follows every
    ///   kind the graph holds.
    ///
    /// Ordering is breadth-first, and within one depth level ascending by
    /// [`ResourceKey`]. Two runs over the same graph produce identical output.
    pub fn traverse(
        &self,
        key: &ResourceKey,
        depth: u32,
        kinds: &[EdgeKind],
    ) -> Vec<TraversalEntry> {
        let follows = |edge: &Edge| kinds.is_empty() || kinds.contains(&edge.kind);
        let mut seen: BTreeSet<ResourceKey> = BTreeSet::new();
        seen.insert(*key);
        let mut out = vec![TraversalEntry {
            key: *key,
            depth: 0,
        }];
        let mut queue: VecDeque<(ResourceKey, u32)> = VecDeque::new();
        queue.push_back((*key, 0));
        while let Some((current, current_depth)) = queue.pop_front() {
            if current_depth >= depth {
                continue;
            }
            let mut frontier: Vec<ResourceKey> = self
                .neighbours(&current)
                .iter()
                .filter(|edge| follows(edge))
                .map(|edge| edge.to)
                .collect();
            frontier.sort_unstable();
            frontier.dedup();
            for next in frontier {
                if !seen.insert(next) {
                    continue;
                }
                let next_depth = current_depth + 1;
                out.push(TraversalEntry {
                    key: next,
                    depth: next_depth,
                });
                queue.push_back((next, next_depth));
            }
        }
        out
    }

    /// Total edges, counted once, regardless of how many lists hold them.
    pub fn edge_count(&self) -> usize {
        self.outgoing.values().map(Vec::len).sum()
    }

    /// How many references of `kind` the records declared, counted **before**
    /// de-duplication.
    ///
    /// Distinct from [`ResourceGraph::edge_count_of_kind`], which counts stored
    /// edges. The gap between the two is the number of times a record named the
    /// same target more than once, and it is a property of the content: 5 265 of
    /// the 16 628 texture-set entries in `Spore_Content.package` are repeats.
    /// `0` for a kind that was not built means "not looked at", which the caller
    /// can distinguish by asking [`ResourceGraph::options`].
    pub fn declared_entry_count(&self, kind: EdgeKind) -> usize {
        self.declared_entries.get(&kind).copied().unwrap_or(0)
    }

    /// Edge count for one kind.
    pub fn edge_count_of_kind(&self, kind: EdgeKind) -> usize {
        self.outgoing
            .values()
            .map(|edges| edges.iter().filter(|edge| edge.kind == kind).count())
            .sum()
    }

    /// Records that contributed no edges, with the reason, ascending by key.
    pub fn unresolved(&self) -> &[UnresolvedRecord] {
        &self.unresolved
    }

    /// Edges of one kind whose target is in no loaded package.
    ///
    /// Empty for a kind that was not built, and for a kind that cannot dangle.
    /// Both are non-findings, not zeroes meaning "we looked and found none".
    pub fn dangling_of_kind(&self, kind: EdgeKind) -> &[DanglingReference] {
        self.dangling.get(&kind).map_or(&[], Vec::as_slice)
    }

    /// Every dangling reference, ascending by `(kind, from, to)`.
    pub fn missing_references(&self) -> impl Iterator<Item = &DanglingReference> {
        self.dangling.values().flatten()
    }

    /// How many dangling references of each kind exist.
    pub fn missing_reference_counts(&self) -> BTreeMap<EdgeKind, usize> {
        let mut counts = BTreeMap::new();
        for kind in EdgeKind::ALL {
            if self.options.builds(kind) {
                counts.insert(kind, self.dangling_of_kind(kind).len());
            }
        }
        counts
    }

    /// A summary line per built edge kind, in declaration order.
    pub fn edge_summary(&self) -> Vec<EdgeSummary> {
        EdgeKind::ALL
            .iter()
            .filter(|kind| self.options.builds(**kind))
            .map(|kind| EdgeSummary {
                kind: *kind,
                declared: self.declared_entry_count(*kind),
                edges: self.edge_count_of_kind(*kind),
                dangling: self.dangling_of_kind(*kind).len(),
                level: kind.evidence_level(),
            })
            .collect()
    }
}

/// One row of [`ResourceGraph::edge_summary`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EdgeSummary {
    /// The edge kind.
    pub kind: EdgeKind,
    /// How many references of this kind the records declared, before
    /// de-duplication. Never less than [`EdgeSummary::edges`].
    pub declared: usize,
    /// How many distinct edges of this kind were built.
    pub edges: usize,
    /// How many of them name a record in no loaded package.
    pub dangling: usize,
    /// The grade every edge of this kind carries.
    pub level: EvidenceLevel,
}

/// Why the `refCount` key table could not be read.
///
/// Each variant is a *non-finding*: the record did not yield keys, and the
/// reason is recorded rather than the empty result being passed off as "this
/// model references nothing".
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReferenceTableError {
    /// The record is shorter than the 8-byte header the count word sits in.
    TooShort {
        /// Record length in bytes.
        len: usize,
    },
    /// The version word is not the one this build decodes.
    UnsupportedVersion {
        /// The version word as stored.
        version: u32,
    },
    /// The big-endian count is larger than the bytes the record has left.
    CountOverrunsRecord {
        /// The count word as stored.
        count: u32,
        /// Bytes the table would need.
        needed: u64,
        /// Bytes actually left in the record.
        available: u64,
    },
}

impl ReferenceTableError {
    /// The canonical spelling used in reports.
    pub fn as_str(&self) -> &str {
        match self {
            Self::TooShort { .. } => "reference-table-too-short",
            Self::UnsupportedVersion { .. } => "reference-table-unsupported-version",
            Self::CountOverrunsRecord { .. } => "reference-table-count-overruns-record",
        }
    }
}

impl core::fmt::Display for ReferenceTableError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::TooShort { len } => {
                write!(f, "reference-table-too-short: {len} bytes is under the 8-byte header")
            }
            Self::UnsupportedVersion { version } => write!(
                f,
                "reference-table-unsupported-version: version {version} is not {}",
                spore_gmdl::SUPPORTED_VERSION
            ),
            Self::CountOverrunsRecord {
                count,
                needed,
                available,
            } => write!(
                f,
                "reference-table-count-overruns-record: {count} keys need {needed} bytes, {available} left"
            ),
        }
    }
}

/// Bytes one reference-key entry occupies: `{instance, group, type}`.
const REFERENCE_KEY_SIZE: u64 = 12;

/// Reads the `refCount` key table of a gmdl record **big-endian**.
///
/// # Why this function exists at all
///
/// `spore_gmdl::GmdlModel::referenced_files` reads the same table
/// little-endian. Over the real corpus that yields a byte-swapped key for every
/// entry, none of which resolves — see [`claims::REFERENCE_TABLE_ENDIANNESS`]
/// for the measurement. This crate does not own `spore-gmdl`'s file, so it does
/// not fix it here; it reads the bytes correctly and states the divergence.
///
/// # Safety of the byte order claim
///
/// It is not a guess, and it is not a symmetry assumption. Two independent
/// measurements fix it, over 7 989 entries:
///
/// * the `refCount` word immediately before the table is big-endian — already
///   established in this repository, and a big-endian count in front of a
///   little-endian table would be a strange thing for a writer to emit;
/// * every one of the 7 989 entries, read big-endian, is a triple that exists in
///   the package index, and no entry read little-endian is.
///
/// # Errors
///
/// [`ReferenceTableError`], one variant per condition. A record that fails is
/// reported, never silently given zero references.
pub fn read_reference_table(data: &[u8]) -> Result<Vec<ResourceKey>, ReferenceTableError> {
    let header = data
        .get(..8)
        .ok_or(ReferenceTableError::TooShort { len: data.len() })?;
    let version = u32::from_le_bytes([header[0], header[1], header[2], header[3]]);
    if version != spore_gmdl::SUPPORTED_VERSION {
        return Err(ReferenceTableError::UnsupportedVersion { version });
    }
    // BIG-ENDIAN, like the count word this repository has already established.
    // Written out rather than routed through a `from_be_bytes` helper so that a
    // future edit has to delete the word `BIG-ENDIAN` to get it wrong.
    let count = u32::from_be_bytes([header[4], header[5], header[6], header[7]]);
    let needed = u64::from(count) * REFERENCE_KEY_SIZE;
    let available = (data.len() - 8) as u64;
    if needed > available {
        return Err(ReferenceTableError::CountOverrunsRecord {
            count,
            needed,
            available,
        });
    }
    let mut keys = Vec::with_capacity(count as usize);
    for index in 0..count as usize {
        let at = 8 + index * 12;
        // On-disk order is instance, group, type -- the same order the C++
        // reference documents. Only the byte order differs from every other word
        // in the record, which is why this is a separate reader rather than a
        // flag on the shared one.
        let instance = be_word(data, at);
        let group = be_word(data, at + 4);
        let type_id = be_word(data, at + 8);
        keys.push(ResourceKey::new(type_id, group, instance));
    }
    Ok(keys)
}

/// One big-endian `u32` at `at`, or zero when the four bytes are absent.
///
/// The caller has already proved `count * 12` bytes exist, so the `None` arm is
/// unreachable. It maps to a zero rather than a panic because this function must
/// stay total: a library that panics on a malformed record is a library that
/// decides, on the operator's behalf, that a bad byte is a crash.
fn be_word(data: &[u8], at: usize) -> u32 {
    match data.get(at..at + 4) {
        Some(bytes) => u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
        None => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_claim_carries_a_grade_and_provenance() {
        for entry in claims::all_claims() {
            assert!(!entry.subject.is_empty());
            assert!(
                entry.statement.len() > 80,
                "{}: statement too thin to be evidence",
                entry.subject
            );
            assert!(entry.fact.is_available());
            assert!(
                !entry.fact.provenance().is_empty(),
                "{}: unsourced claim",
                entry.subject
            );
            assert_eq!(entry.fact.value(), Some(&entry.statement));
        }
    }

    #[test]
    fn claim_subjects_are_unique_and_resolvable() {
        let mut subjects: Vec<&str> = claims::all_claims()
            .iter()
            .map(|entry| entry.subject)
            .collect();
        subjects.sort_unstable();
        let count = subjects.len();
        subjects.dedup();
        assert_eq!(subjects.len(), count, "two claims share a subject");
        for subject in subjects {
            assert!(claims::claim(subject).is_some(), "{subject} must resolve");
        }
        assert!(claims::claim("no_such_claim").is_none());
    }

    #[test]
    fn the_two_measured_byte_orders_are_graded_verified_and_the_two_adjacencies_inferred() {
        for subject in [
            claims::REFERENCE_TABLE_ENDIANNESS,
            claims::ENDIANNESS_ASYMMETRY,
        ] {
            let fact = claims::claim(subject).expect("recorded");
            assert_eq!(
                fact.level(),
                EvidenceLevel::Verified,
                "{subject} is settled by exact index resolution and cannot be weaker"
            );
        }
        for subject in [
            claims::REFERENCE_TABLE_ROLE,
            claims::CO_MEMBERSHIP_MEANS_OWNERSHIP,
            claims::POLLEN_METADATA_SELF_IDENTITY,
        ] {
            let fact = claims::claim(subject).expect("recorded");
            assert!(
                fact.level() < EvidenceLevel::Verified,
                "{subject} must not be graded Verified"
            );
        }
        // The one claim that is explicitly Unknown, and it says so without
        // pretending to be a claim about something.
        let unknown = claims::claim(claims::PROP_LAYOUT_UNRESOLVED).expect("recorded");
        assert_eq!(unknown.level(), EvidenceLevel::Unknown);
        assert!(unknown.value().is_some_and(|text| text.contains("no edge")));
    }

    #[test]
    fn every_kind_has_a_unique_name_and_a_closed_all() {
        let mut names: Vec<&str> = EdgeKind::ALL.iter().map(|kind| kind.as_str()).collect();
        assert_eq!(names.len(), EdgeKind::ALL.len());
        names.sort_unstable();
        let count = names.len();
        names.dedup();
        assert_eq!(names.len(), count, "two edge kinds share a name");
        for kind in EdgeKind::ALL {
            assert_eq!(EdgeKind::parse(kind.as_str()), Some(kind));
        }
    }

    #[test]
    fn declared_references_outrank_adjacency_in_grade() {
        // A reference read out of a record is at least `Observed`; adjacency is
        // `Inferred` and never better. This is the ordering the whole evidence
        // discipline rests on, so it is pinned rather than assumed.
        for kind in EdgeKind::ALL {
            if kind.is_declared_reference() {
                assert!(
                    kind.evidence_level() >= EvidenceLevel::Observed,
                    "{kind} is a declared reference and cannot be Inferred"
                );
            } else {
                assert_eq!(
                    kind.evidence_level(),
                    EvidenceLevel::Inferred,
                    "{kind} is adjacency and must stay Inferred"
                );
            }
        }
    }

    #[test]
    fn a_short_record_is_refused_rather_than_read_as_zero_references() {
        for length in 0..8usize {
            let error = read_reference_table(&vec![0u8; length]).unwrap_err();
            assert_eq!(
                error.as_str(),
                "reference-table-too-short",
                "length {length}"
            );
        }
    }

    #[test]
    fn the_count_word_is_big_endian_and_the_table_behind_it_is_too() {
        // A synthetic record with version 8 and one key, written so that a
        // little-endian reader of either the count word or the table produces a
        // *different, wrong* answer. That is the only way the difference is
        // observable from inside this crate, which owns neither `spore-gmdl` nor
        // the corpus.
        //
        // `TYPE` is byte-reversal-stable under neither reading: 0xA1B2C3D4 read
        // big-endian is 0xA1B2C3D4 and read little-endian is 0xD4C3B2A1.
        const TYPE: u32 = 0xA1B2_C3D4;
        const GROUP: u32 = 0x1122_3344;
        const INSTANCE: u32 = 0xAABB_CCDD;

        let mut record = Vec::new();
        record.extend_from_slice(&spore_gmdl::SUPPORTED_VERSION.to_le_bytes());
        // A little-endian reader of this word would see 0x01000000 and ask for
        // 201 million bytes of keys.
        record.extend_from_slice(&1u32.to_be_bytes());
        // On-disk order is instance, group, type; every word big-endian.
        record.extend_from_slice(&INSTANCE.to_be_bytes());
        record.extend_from_slice(&GROUP.to_be_bytes());
        record.extend_from_slice(&TYPE.to_be_bytes());
        record.extend_from_slice(&[0u8; 32]);

        assert_eq!(
            read_reference_table(&record).unwrap(),
            vec![ResourceKey::new(TYPE, GROUP, INSTANCE)]
        );

        // The little-endian reading of the same bytes -- what spore_gmdl does --
        // lands on a different triple. Not "the same value spelled differently":
        // a different value, which is the whole point.
        let le = ResourceKey::new(
            u32::from_le_bytes(TYPE.to_be_bytes()),
            u32::from_le_bytes(GROUP.to_be_bytes()),
            u32::from_le_bytes(INSTANCE.to_be_bytes()),
        );
        assert_ne!(
            le,
            ResourceKey::new(TYPE, GROUP, INSTANCE),
            "the two byte orders must disagree, or this test proves nothing"
        );
        // And the disagreement is a full byte reversal of each word, which is
        // what "read little-endian instead of big-endian" means on a 4-byte word.
        assert_eq!(le.type_id ^ TYPE, 0x7571_7175);
        assert_eq!(le.group_id ^ GROUP, 0x5511_1155);
    }

    #[test]
    fn a_little_endian_count_word_would_ask_for_sixteen_million_keys() {
        // The consequence spelled out, so the byte order is not just a different
        // number: it is a walk that leaves the record immediately. This is the
        // bug AGENTS.md calls the most expensive one in this format's history,
        // applied to the word *behind* the word it already names.
        let count = 1u32;
        assert_eq!(le_of_be(count), 16_777_216);
        assert_eq!(u64::from(le_of_be(count)) * REFERENCE_KEY_SIZE, 201_326_592);
    }

    fn le_of_be(value: u32) -> u32 {
        u32::from_le_bytes(value.to_be_bytes())
    }

    #[test]
    fn a_count_that_overruns_the_record_is_refused_by_name() {
        let mut record = Vec::new();
        record.extend_from_slice(&spore_gmdl::SUPPORTED_VERSION.to_le_bytes());
        record.extend_from_slice(&1000u32.to_be_bytes());
        record.extend_from_slice(&[0u8; 8]);
        let error = read_reference_table(&record).unwrap_err();
        assert_eq!(error.as_str(), "reference-table-count-overruns-record");
        assert!(error.to_string().contains("12000"));
    }

    #[test]
    fn an_unsupported_version_is_named_rather_than_walked() {
        let mut record = vec![0u8; 64];
        record[0..4].copy_from_slice(&9u32.to_le_bytes());
        assert_eq!(
            read_reference_table(&record).unwrap_err(),
            ReferenceTableError::UnsupportedVersion { version: 9 }
        );
    }

    #[test]
    fn zero_keys_is_an_empty_table_not_a_refusal() {
        let mut record = Vec::new();
        record.extend_from_slice(&spore_gmdl::SUPPORTED_VERSION.to_le_bytes());
        record.extend_from_slice(&0u32.to_be_bytes());
        record.extend_from_slice(&[0u8; 32]);
        // "This record declares no referenced files" and "we could not read the
        // table" must not produce the same value.
        assert_eq!(read_reference_table(&record).unwrap(), Vec::new());
    }

    #[test]
    fn options_gate_each_kind_independently() {
        let all = GraphOptions::all();
        for kind in EdgeKind::ALL {
            assert!(all.builds(kind));
        }
        for kind in EdgeKind::ALL {
            assert!(!GraphOptions::none().builds(kind));
        }
        assert!(GraphOptions::default().builds(EdgeKind::GmdlTexture));
        assert!(
            !GraphOptions::default().builds(EdgeKind::SiblingRecord),
            "the weakest edge is off unless asked for"
        );
    }

    #[test]
    fn an_empty_graph_traverses_to_its_root_and_nothing_else() {
        let graph = ResourceGraph::new();
        let key = ResourceKey::new(1, 2, 3);
        assert_eq!(graph.known_records(), 0);
        assert!(!graph.contains(&key));
        assert!(graph.neighbours(&key).is_empty());
        assert!(graph.dependants(&key).is_empty());
        assert_eq!(
            graph.traverse(&key, 8, &[]),
            vec![TraversalEntry { key, depth: 0 }]
        );
        assert!(graph.missing_references().next().is_none());
    }

    #[test]
    fn zero_dangling_and_kind_not_built_are_different_answers() {
        // A default graph has looked at three kinds and found nothing dangling:
        // the counts say `0`, which is a *finding*.
        let looked = ResourceGraph::new().missing_reference_counts();
        assert_eq!(looked.get(&EdgeKind::GmdlTexture), Some(&0));
        assert_eq!(looked.get(&EdgeKind::GmdlReference), Some(&0));
        // A graph built with nothing enabled has no entry at all for any kind,
        // because "we did not look" is not "we looked and found none". The
        // absent sibling entry here is the same distinction one level up.
        assert!(!looked.contains_key(&EdgeKind::SiblingRecord));

        let blind = ResourceGraph::build_with(&ContentStore::new(), GraphOptions::none());
        assert!(blind.missing_reference_counts().is_empty());
        assert!(blind.edge_summary().is_empty());
        assert_eq!(blind.edge_count(), 0);
        assert_eq!(blind.unresolved(), &[]);
    }
}
