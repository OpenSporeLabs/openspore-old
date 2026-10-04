//! Every claim this crate records, with the grade it is recorded at.
//!
//! # Why this module exists
//!
//! This crate sits between a gmdl record and a renderer, and the gap it walks
//! over is *naming*: a gmdl record carries a 32-bit material id and a list of
//! `{instance, group}` texture references, and neither of those things says
//! what anything is called. If a resolver returns a bare `Vec<u8>` of RGBA and
//! a `material_id: u32`, then by the time the renderer sees the texture the
//! three questions a caller actually has — *what is this material?*, *which
//! sampler is this texture bound to?*, *how do you know the record was a
//! raster at all?* — have been answered silently, or not at all.
//!
//! So each of those answers is recorded here **as a graded statement**, and the
//! grades are deliberately not flattering:
//!
//! * a **material id has no known meaning** in this build and no name is
//!   invented from it (`no_material_registry`);
//! * the **16 bytes** a gmdl texture-set entry skips before `{instance, group}`
//!   are not decoded, so **no sampler role** is claimed
//!   (`texture_entry_header_not_decoded`);
//! * a **texture reference carries no type word**, so the record type is an
//!   explicit caller-supplied assumption, graded [`EvidenceLevel::Inferred`]
//!   rather than observed.
//!
//! The reference implementation never learned the material mapping either:
//! `src/assets/MaterialRegistry.cpp` keeps a registry the engine populates from
//! outside, which is why "no registry" is a statement about the *format*, not
//! about a gap in this crate.
//!
//! # Grading, stated per claim
//!
//! [`ALL_CLAIMS`] is the single table of (subject, grade, why). The resolution
//! code builds its [`Fact`]s through the helpers here, and a test asserts that
//! every recorded fact's level equals [`grade_of`] for its subject — so the
//! grade a caller reads and the grade this module documents cannot drift apart.
//!
//! Absent answers are [`Fact::unavailable`], which by construction carries
//! `EvidenceLevel::Unknown`, `EvidenceState::Missing` and **no provenance**. A
//! non-finding is not a zero, not an empty string and not a default colour; see
//! `spore_core::evidence`.
//!
//! # Refuted subjects, kept under their original names
//!
//! Three subjects here once asserted something the corpus has since refuted:
//! [`LAYER_COUNT_IS_DERIVED`], [`DXT5_CARRIES_NO_COLOUR`] and
//! [`DXT5_LAYERS_ARE_DISTINCT`]. **None of them was deleted.** Each is recorded
//! as [`ClaimKind::Refuted`] under its original spelling, carrying the
//! measurement that refuted it and naming, through
//! [`ClaimSpec::kind`][k], the *other* crate's subject that measured it.
//!
//! Two reasons that is the right shape and not bookkeeping:
//!
//! 1. A refutation that deletes its subject leaves the next reader with nothing
//!    to contradict, which is how a wrong belief gets re-derived. The commit
//!    message, the pull request and the changelog all age out; this does not.
//! 2. Every one of the three was *load-bearing* — two of them decided which
//!    records this crate could classify at all — so deleting them would have
//!    left the change invisible in the place a reader looks first.
//!
//! # Delegation, and the ceiling it produces
//!
//! A statement about what a decoded texel **contains** cannot be stronger than
//! the codec that decoded it. `spore-texture` grades BC3 as
//! [`spore_texture::claims::BC3_CODEC`] — `INFERRED`, because the stride is a
//! byte count but the codec inside a block has never been compared against the
//! game's own output. [`bc3_claim_ceiling`] returns that grade, and
//! [`COLOUR_AND_MASK_CONTENT`] is pinned at or below it by a test.
//!
//! This is the second time this crate has delegated a fact it previously
//! restated. The first time it did not, `spore-texture` corrected the `0x15`
//! family's description, this crate asserted the opposite of its own decoder,
//! and **five** tests failed rather than one because the stale claim was
//! load-bearing for the batch error handling.

use spore_core::evidence::{EvidenceLevel, EvidenceState, Fact, Provenance};

// ---------------------------------------------------------------------------
// Subjects
// ---------------------------------------------------------------------------

/// The gmdl texture reference carries no type word, so nothing observed says
/// what kind of record it points at.
///
/// Recorded as a non-finding: this is an absence *in the format*, not a value
/// this build failed to look up.
pub const TEXTURE_REFERENCE_TYPE_WORD: &str = "texture_reference_type_word";

/// The record type that was used for the lookup, and the fact that it was
/// assumed by the caller rather than read from the reference.
pub const ASSUMED_RECORD_TYPE: &str = "assumed_record_type";

/// The canonical name of the assumed type, when `spore-core` has one.
pub const ASSUMED_RECORD_TYPE_NAME: &str = "assumed_record_type_name";

/// The canonical name of the resolved record's group.
///
/// [`spore_core::record::GROUP_NAMES`] has no entry for the three groups the
/// documented asset references (`0x40632900`, `0x40632901`, `0x40632902`), so
/// this is a non-finding for every texture of that asset. Names are **not**
/// added to `spore-core` on the strength of "it is obviously the base colour
/// map": see the crate documentation, "The group-name gap".
pub const RESOLVED_GROUP_NAME: &str = "resolved_group_name";

/// A raster record stores no layer count; the count is derived by division.
///
/// **REFUTED, 2026-10-05.** There is no layer count to derive *and no division
/// to perform*: every raster record in the corpus is exactly **one** chain of mip
/// levels, and `spore-texture` verifies the record length against that identity
/// rather than dividing a payload. The old "derived 2" was a double error that
/// cancelled — 8 bytes per 4x4 block instead of 16 made one picture look like two
/// half-pictures. The subject keeps its spelling so the refutation is findable
/// from the old name.
pub const LAYER_COUNT_IS_DERIVED: &str = "layer_count_is_derived";

/// Re-publication of [`spore_texture::claims::DXT5_SPEC_DEVIATIONS`], so a
/// caller reading only this crate's claims still learns that the *previous*
/// decode was not BC3 and why.
///
/// The subject string is unchanged because the upstream subject it republishes
/// is unchanged, and the upstream claim is now itself the register of the
/// refutation. Delegated, never restated: see the module documentation.
pub const DXT5_SPEC_DEVIATIONS: &str = "dxt5_spec_deviations";

/// Re-publication of [`spore_texture::claims::ENVELOPE_FIELD_10_MEANING`].
pub const ENVELOPE_FIELD_10_MEANING: &str = "envelope_field_10_meaning";

/// Re-publication of [`spore_texture::claims::ENVELOPE_FIELD_18_MEANING`].
///
/// **Delegated, not a local non-finding.** It used to be republished here as
/// `meaning_not_established`, which was already wrong when the field's real state
/// was measured upstream: it is the base level's own byte count in **all three**
/// formats, verified over 4 386 records, and `spore-texture` reads it as a
/// cross-check. That is the second time this crate would have contradicted its own
/// decoder if it had kept a local copy.
pub const ENVELOPE_FIELD_18_MEANING: &str = "envelope_field_18_meaning";

/// Re-publication of [`spore_texture::claims::ENVELOPE_FIELD_1C_MEANING`].
pub const ENVELOPE_FIELD_1C_MEANING: &str = "envelope_field_1c_meaning";

/// Re-publication of [`spore_texture::claims::LUMINANCE_FOURCC`].
pub const LUMINANCE_FOURCC_FAMILY: &str = "luminance_fourcc_family";

/// What the material id names. Nothing, in this build.
pub const MATERIAL_NAME: &str = "material_name";

/// What the material id *means*. Nothing, in this build.
pub const MATERIAL_ID_MEANING: &str = "material_id_meaning";

/// Which sampler stage a texture binding feeds. Unknown, and **not** because
/// the entry's header was never read — it has now been read (see
/// [`TEXTURE_SET_ENTRY_LAYOUT`]) and holds no role field.
pub const SAMPLER_ROLE: &str = "sampler_role";

/// What a resolved texture's own bytes are: a tangent-space normal map, a colour
/// image with a fourth plane, or unidentified.
///
/// Distinct from [`SAMPLER_ROLE`] on purpose. This one is answerable from the
/// record and is checked per record; the sampler stage is a property of a
/// shader this workspace has never read.
pub const TEXTURE_CONTENT_ROLE: &str = "texture_content_role";

/// The `0x15` family's lanes 0-2 are a tangent-space normal map stored **Z
/// first**.
///
/// **Re-published from `spore_texture::claims`, never restated.** The
/// measurement is that crate's: 901 of 901 records of the family in
/// `Spore_Content.package`. See `docs/LUMINANCE-RASTER.md`.
pub const NORMAL_MAP_CONTENT: &str = "normal_map_content";

/// A `DXT5`-family record read as BC3 carries a three-channel colour image in
/// lanes 0-2, over a fourth plane that does not vary with them.
///
/// **Graded at the ceiling, not above it.** The stride is proven by a byte
/// count and the alpha half is measured, but the BC3 codec itself is
/// `spore_texture`'s `INFERRED` reading — no decode of a real record has been
/// compared against the game's own output — so this claim inherits that grade.
/// See [`bc3_claim_ceiling`], which a test pins it against.
pub const COLOUR_AND_MASK_CONTENT: &str = "colour_and_mask_content";

/// The fourth lane of a `DXT5` record does not track its three colour lanes.
///
/// **This crate's own measurement**, and graded at the BC3 ceiling rather than
/// `Observed`: over all 1 747 `DXT5` records of `Spore_Content.package` the mean
/// absolute colour-to-alpha correlation has median **0.067** and maximum
/// **0.539**, against a median mutual correlation of **0.510** among the colour
/// lanes themselves. The *measurement* is exact, but a claim that compares the
/// colour lanes with the alpha lane depends on the colour half, and that half's
/// codec is `INFERRED` upstream — so the claim is too. Published per record on
/// [`MipStatistics`](crate::MipStatistics), which is where a caller re-derives
/// it, rather than as a number in this sentence. NOT used as a gate: 467 of 1 747
/// records fall below a 0.30 mean colour correlation, so any correlation
/// threshold would be a statistic rather than a property.
pub const MASK_PLANE_INDEPENDENT_OF_COLOUR: &str = "mask_plane_independent_of_colour";

/// A `DXT5`-family record carries no colour: the byte-pinned codec reduces its
/// colour field to two values across every record in the corpus.
///
/// **REFUTED, 2026-10-05 — and kept under its original spelling.** The claim was
/// a tautology of a broken codec, not a measurement of any record: green and
/// blue read as zero under R5G5B5 on a **6-bit** word *by construction*, because
/// a 6-bit value cannot set bits 10..19. Read as BC3 the same records carry at
/// least **32 distinct reds, 64 greens and 32 blues** each, and 0 of 1 747 have a
/// colour lane of two values or fewer.
pub const DXT5_CARRIES_NO_COLOUR: &str = "dxt5_carries_no_colour";

/// A `DXT5` record's two layers are two distinct surfaces, not one image and a
/// copy.
///
/// **REFUTED, 2026-10-05 — and kept under its original spelling.** There are no
/// two layers. Every raster record in the corpus is one chain of mip levels, so
/// `layers.len()` is 1 and the question has no subject. The 1 233 / 1 233
/// measurement this claim was built on compared the two *halves of one picture*
/// that an 8-byte block stride had split in two, which is why they differed.
pub const DXT5_LAYERS_ARE_DISTINCT: &str = "dxt5_layers_are_distinct";

/// What the fourth plane of a `DXT5` record is **for**. Nothing, in this build.
///
/// The subject keeps the word "layer" from the refuted two-layer model so the
/// non-finding stays findable; the plane in question is lane 3 of the record's
/// single chain.
pub const MASK_LAYER_PURPOSE: &str = "mask_layer_purpose";

/// The shapes a gmdl texture set occurs in, measured over the whole corpus.
pub const TEXTURE_SET_SHAPES: &str = "texture_set_shapes";

/// A texture-set entry's first header word is the entry's own ordinal within
/// its texture set.
pub const SAMPLER_WORD_IS_ENTRY_ORDINAL: &str = "sampler_word_is_entry_ordinal";

/// A referenced texture's group id has the entry's ordinal in its low byte.
pub const GROUP_LOW_BYTE_IS_SLOT: &str = "group_low_byte_is_slot";

/// How a model's texture set was cut into per-mesh pieces, and how far that
/// cut is evidence rather than arithmetic.
pub const PER_MESH_TEXTURE_PARTITION: &str = "per_mesh_texture_partition";

/// That material-info block `i` is mesh `i`. Not established.
pub const MATERIAL_BLOCK_IS_MESH: &str = "material_block_is_mesh";

/// The record's own group-id ordinals contradict the per-mesh cut the division
/// produces, which is what a **short** reference list looks like from here.
///
/// Graded when at least one slot is contradicted and a non-finding otherwise.
/// This is the observation the third [`BindingScope`](crate::BindingScope)
/// variant is built on: the gmdl material-info walk stops inside the table on
/// the undocumented shader-data id `0x218` in 619 of 4 209 gmdl records of
/// `Spore_Content.package`, and in 100 of those it stops before the last
/// declared block, so the flat list covers fewer blocks than there are meshes.
/// All 100 divide evenly by the mesh count, which is why the arithmetic alone
/// cannot be trusted and the record has to be asked instead.
pub const CUT_CONTRADICTED_BY_RECORD: &str = "per_mesh_cut_contradicted_by_record_ordinals";

/// The observed layout of one gmdl texture-set entry: 16 skipped bytes, then the
/// referenced record's instance and group ids.
pub const TEXTURE_SET_ENTRY_LAYOUT: &str = "texture_set_entry_layout";

/// Which texture-set block belongs to which material id. Not established, so a
/// [`MeshMaterial`](crate::MeshMaterial)'s slots are the whole model's
/// texture set rather than one material's share of it.
pub const MATERIAL_TEXTURE_OWNERSHIP: &str = "material_texture_ownership";

/// The observation that the material id was read and nothing more: one
/// little-endian word per mesh, verbatim.
///
/// This is the counterpart of [`MATERIAL_ID_MEANING`]. Keeping the observation
/// and the meaning apart is the point: "the id is 0x407dfddb" is an observation,
/// and "the id is *a thing called* Leather" is a claim nobody can support.
pub const MATERIAL_ID_READ_VERBATIM: &str = "material_id_read_verbatim";

/// How many texture bindings a material carries, and what that count covers.
pub const MATERIAL_BINDING_COUNT: &str = "material_binding_count";

// ---------------------------------------------------------------------------
// Reason codes — the closed vocabulary of `Fact::unavailable`
// ---------------------------------------------------------------------------

/// No mapping from a material id to a name exists in this build, or in the
/// reference implementation (whose registry is populated from outside).
pub const NO_MATERIAL_REGISTRY: &str = "no_material_registry";

/// A `0x20D` texture-set entry stores no type word for the record it names.
pub const TEXTURE_REF_HAS_NO_TYPE_WORD: &str = "texture_ref_has_no_type_word";

/// The 16 bytes preceding `{instance, group}` in a texture-set entry are
/// skipped, not decoded, so no sampler role follows from them.
///
/// **Superseded as a statement about *decoding*, and still exactly right as a
/// statement about the *role*.** The first of the 16 bytes has now been read
/// and is the entry's ordinal ([`SAMPLER_WORD_IS_ENTRY_ORDINAL`]); the other 12
/// take four distinct values across 6684 references and do not vary with the
/// ordinal. So the header carries no role field, which is a stronger and better
/// supported reason than "nobody decoded it".
pub const NO_ROLE_FIELD_IN_TEXTURE_ENTRY: &str = "no_role_field_in_texture_entry";

/// `spore-core`'s canonical type table has no name for this id.
pub const NO_CANONICAL_TYPE_NAME: &str = "no_canonical_type_name";

/// `spore-core`'s canonical group table has no name for this id.
pub const NO_GROUP_NAME_IN_CANONICAL_TABLE: &str = "no_group_name_in_canonical_table";

/// A field whose value has been observed but whose *meaning* nobody
/// established. The value being constant is not evidence about what it is.
pub const MEANING_NOT_ESTABLISHED: &str = "meaning_not_established";

/// The gmdl walk flattens every texture-set block into one list and never
/// records which block belonged to which material id.
pub const MATERIAL_BLOCKS_NOT_ASSOCIATED: &str =
    "material_info_blocks_not_associated_with_material_ids";

/// Nothing measured establishes that material-info block `i` is mesh `i`, and
/// over this corpus nothing could.
///
/// Separate from [`MATERIAL_BLOCKS_NOT_ASSOCIATED`] on purpose: that one says
/// the *walk* discards the block boundaries, and this one says the *ordering* is
/// an inference. The partition this crate offers is therefore graded `INFERRED`
/// even when the arithmetic divides exactly.
///
/// The stronger half is measured: all 1 685 cut records with more than one mesh
/// have every per-mesh slice naming the *same* texture set, so a permutation of
/// the blocks is invisible. See the spec's basis.
pub const BLOCK_ORDER_NOT_ESTABLISHED: &str = "material_info_block_order_not_established";

/// No per-mesh cut was attempted, so every mesh holds the whole collected
/// reference list.
///
/// Two situations reach it, and *which* one happened is stated by
/// [`BindingScope`](crate::BindingScope) and by
/// [`CUT_CONTRADICTED_BY_RECORD`] rather than by this code: the list did not
/// divide evenly by the mesh count, or it did and the record's own group-id
/// ordinals contradicted the result. The second is the dangerous one -- it is a
/// short list that divides -- so it must never be reported under a reason that
/// says "did not divide".
pub const TEXTURE_SET_NOT_CUT_PER_MESH: &str = "texture_set_not_cut_per_mesh";

/// Every slot stated an ordinal and every one of them agreed with the cut.
///
/// The non-finding arm of [`CUT_CONTRADICTED_BY_RECORD`], and the reason it earns
/// its keep: a record carrying it has been checked against its own bytes and the
/// bytes agreed, which is neither the same as a record nobody looked at nor the
/// same as one whose slots stated nothing.
pub const NO_CUT_CONTRADICTION: &str = "no_ordinal_contradiction_between_record_and_derived_cut";

// ---------------------------------------------------------------------------
// Provenance references — the paths a graded claim is sourced from
// ---------------------------------------------------------------------------

/// The gmdl walk that skips the texture-set entry header.
pub const PROV_GMDL_WALK: &str = "crates/spore-gmdl/src/parse.rs";

/// This crate's own resolution step, for statements derived here.
pub const PROV_RESOLVE: &str = "crates/spore-material/src/resolve.rs";

/// The raster envelope byte-pinned against real records.
pub const PROV_MATERIALS_DESIGN: &str = "docs/MATERIALS-DESIGN.md §1";

/// The role census: every table in this crate's role assignments, measured over
/// `SPORE/Data/Spore_Content.package` on 2026-10-04.
pub const PROV_MATERIAL_ROLES: &str = "docs/MATERIAL-ROLES.md";

/// The stdlib-only Python oracle for the block codec.
pub const PROV_DXT5_ORACLE: &str = "tools/spore/dxt5/dxt5.py";

/// `spore-texture`'s claim table, the source of the re-published envelope and
/// codec claims.
pub const PROV_TEXTURE_CLAIMS: &str = "crates/spore-texture/src/claims.rs";

/// The canonical type-name table, transcribed from the community SDK.
pub const PROV_TYPE_NAMES: &str = "tools/spore/types/typenames.json";

/// The canonical group-name table, transcribed from the community SDK plus the
/// per-package prop directory record.
pub const PROV_GROUP_NAMES: &str = "tools/spore/types/groupnames.json";

/// The C++ reference's material registry, which the engine populates from
/// outside and which therefore never learned the mapping either.
pub const PROV_MATERIAL_REGISTRY: &str = "src/assets/MaterialRegistry.cpp";

/// What a recorded claim is: a value with a grade, or a non-finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClaimKind {
    /// A value that was established, with its grade.
    Graded(EvidenceLevel),
    /// "We looked and found nothing", with the closed reason code that says so.
    NonFinding(&'static str),
    /// A value that is established **when the canonical tables have it** and a
    /// non-finding when they do not.
    ///
    /// The two name claims are of this kind: a type or group id that
    /// `spore-core` names gets a [`spore_core::evidence::Fact::persisted`] value
    /// at `VERIFIED`, and one it does not name gets a non-finding with the
    /// stated reason. Both are honest; the difference is whether a committed
    /// table supports the name, not how this build feels about it.
    GradedOrNonFinding {
        /// The grade of the value, when there is one.
        level: EvidenceLevel,
        /// The reason code of the non-finding, when the table has no name.
        reason: &'static str,
    },
    /// A subject whose former claim the corpus **refuted**. The value recorded
    /// is the refutation; `refuted_by` names the *other* crate's subject that
    /// measured it.
    ///
    /// Three subjects here are in this state, and none of them was deleted — see
    /// the module documentation for why a refutation that removes its subject
    /// is a defect.
    Refuted {
        /// The grade of the **refutation**, which is not the grade of the thing
        /// that was refuted.
        level: EvidenceLevel,
        /// The subject in `spore_texture::claims` that measured the refutation.
        ///
        /// A subject string rather than a level, because the point is not that
        /// the refutation is well supported but that the *evidence lives in the
        /// crate that did the measuring*. A caller can look it up; a bare
        /// `EvidenceLevel` would let this crate re-state the finding and drift.
        refuted_by: &'static str,
    },
}

impl ClaimKind {
    /// The grade this claim is recorded at.
    ///
    /// For [`Self::Refuted`] this is the grade of the **refutation**, so a
    /// subject that once carried `VERIFIED` for a false statement now carries
    /// `VERIFIED` for its falsity. That is deliberate: the grade describes the
    /// thing this crate records today, and today it records that the old claim
    /// is false.
    pub const fn level(self) -> EvidenceLevel {
        match self {
            Self::Graded(level) | Self::GradedOrNonFinding { level, .. } => level,
            Self::Refuted { level, .. } => level,
            Self::NonFinding(_) => EvidenceLevel::Unknown,
        }
    }

    /// The reason code, when this claim can be a non-finding.
    pub const fn reason(self) -> Option<&'static str> {
        match self {
            Self::Graded(_) | Self::Refuted { .. } => None,
            Self::NonFinding(reason) | Self::GradedOrNonFinding { reason, .. } => Some(reason),
        }
    }

    /// Whether this claim may carry a value.
    pub const fn can_carry_value(self) -> bool {
        !matches!(self, Self::NonFinding(_))
    }

    /// The subject that measured this subject's refutation, when it has one.
    pub const fn refuted_by(self) -> Option<&'static str> {
        match self {
            Self::Refuted { refuted_by, .. } => Some(refuted_by),
            _ => None,
        }
    }

    /// Whether this subject's recorded value is a refutation.
    pub const fn is_refuted(self) -> bool {
        matches!(self, Self::Refuted { .. })
    }
}

/// One claim this crate can record: its subject, its grade, and why.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClaimSpec {
    /// The stable subject string.
    pub subject: &'static str,
    /// Graded value, or non-finding.
    pub kind: ClaimKind,
    /// Why the grade is what it is. Part of the contract, not a comment.
    pub basis: &'static str,
}

/// Every claim this crate can record, in declaration order.
pub const ALL_CLAIMS: &[ClaimSpec] = &[
    ClaimSpec {
        subject: TEXTURE_REFERENCE_TYPE_WORD,
        kind: ClaimKind::NonFinding(TEXTURE_REF_HAS_NO_TYPE_WORD),
        basis: "spore-gmdl's GmdlTextureRef holds only {instance_id, group_id}; the \
                0x20D entry has no type word, so what the record IS comes from context.",
    },
    ClaimSpec {
        subject: ASSUMED_RECORD_TYPE,
        kind: ClaimKind::Graded(EvidenceLevel::Inferred),
        basis: "The caller passed assumed_type and the record decoded as that type. A \
                successful decode confirms the bytes fit; it does not make the reference \
                declare the type, so this is inference from context, not observation.",
    },
    ClaimSpec {
        subject: ASSUMED_RECORD_TYPE_NAME,
        kind: ClaimKind::GradedOrNonFinding {
            level: EvidenceLevel::Verified,
            reason: NO_CANONICAL_TYPE_NAME,
        },
        basis: "transcribed verbatim from the SDK-sourced typenames.json, and checked by \
                spore-tools against spore-core's table. An id the table does not name is a \
                non-finding, never a hex spelling dressed up as a name.",
    },
    ClaimSpec {
        subject: RESOLVED_GROUP_NAME,
        kind: ClaimKind::GradedOrNonFinding {
            level: EvidenceLevel::Verified,
            reason: NO_GROUP_NAME_IN_CANONICAL_TABLE,
        },
        basis: "transcribed verbatim from groupnames.json (SDK + the per-package prop \
                directory record). Unknown ids are a non-finding, not a guess -- which is \
                the case for all three groups the documented asset references.",
    },
    ClaimSpec {
        subject: LAYER_COUNT_IS_DERIVED,
        kind: ClaimKind::Refuted {
            level: EvidenceLevel::Verified,
            refuted_by: spore_texture::claims::LAYER_COUNT,
        },
        basis: "REFUTED, and the subject is kept under its old name so the \
                refutation is findable from the thing it refutes. There is no \
                layer count and nothing to divide for one: every raster record in \
                the corpus is exactly ONE chain of mip levels, verified by an \
                exact-consume record-length identity in 4386 of 4386 records \
                across Spore_Content.package and Spore_EP1_Content_01.package. \
                The old 'derived 2' was a double error that cancelled exactly -- \
                8 bytes per 4x4 block instead of 16 makes one picture look like \
                two half-pictures -- which is why 1374 records decoded and 373 \
                did not. RasterImage::layers is therefore always length 1.",
    },
    ClaimSpec {
        subject: DXT5_SPEC_DEVIATIONS,
        kind: ClaimKind::Refuted {
            level: EvidenceLevel::Verified,
            refuted_by: spore_texture::claims::DXT5_SPEC_DEVIATIONS,
        },
        basis: "REFUTED, 2026-10-05. The departures from published BC3 -- an \
                inverted palette-size test, R5G5B5 channel reads, a 6/2 alpha blend, \
                no alpha0 == alpha1 collapse -- were real, byte-pinned departures, and \
                that is exactly the problem: two implementations agreeing with each \
                other on synthetic input produced a 'specification' that no record \
                supports. Every recorded level prefix in every DXT5 record is the \
                16-byte-per-block figure, so the stride was wrong. Spore's blocks ARE \
                BC3. This crate delegates rather than restating, for the second time \
                it has had the chance to get this wrong.",
    },
    ClaimSpec {
        subject: ENVELOPE_FIELD_10_MEANING,
        kind: ClaimKind::NonFinding(MEANING_NOT_ESTABLISHED),
        basis: "re-published from spore_texture::claims; no decoder reads the word.",
    },
    ClaimSpec {
        subject: ENVELOPE_FIELD_18_MEANING,
        kind: ClaimKind::Graded(EvidenceLevel::Verified),
        basis: "re-published from spore_texture::claims, which measured it over 4386 \
                of 4386 raster records in Spore_Content.package and \
                Spore_EP1_Content_01.package. It is the base level's OWN byte count in \
                all three formats -- 0x40000 for a 512x512 DXT5 record, 0x10000 for a \
                256x256, 0x8000 for a 256x256 DXT1, w*h*4 for the 0x15 family -- and the \
                decoder READS it as a cross-check, refusing a record whose header \
                disagrees with its own geometry. This crate previously republished it as \
                'meaning not established', which was a local copy contradicting the \
                decoder above it. Delegated, never restated.",
    },
    ClaimSpec {
        subject: ENVELOPE_FIELD_1C_MEANING,
        kind: ClaimKind::NonFinding(MEANING_NOT_ESTABLISHED),
        basis: "re-published from spore_texture::claims; it varies with the format, which \
                is the whole of what is known about it.",
    },
    ClaimSpec {
        subject: LUMINANCE_FOURCC_FAMILY,
        // This was a NonFinding and is now VERIFIED. `spore-texture` established
        // the framing over 901/901 records in Spore_Content, and this claim
        // previously republished the old, refuted statement that the family is
        // "a different format this build refuses rather than decodes". A
        // downstream claim that contradicts the decoder is worse than no claim.
        kind: ClaimKind::Graded(EvidenceLevel::Verified),
        basis: "The 0x15 family is DECODED, not refused. `spore-texture` verified its \
                framing over 901/901 records in Spore_Content: a 28-byte header, NO layer \
                headers, a u32 byte-count prefix before every mip past the base, and \
                uncompressed 4-bytes-per-texel -- so it is not the DXT5 layer walk at all. \
                Lanes 0-2 are a tangent-space normal map stored Z FIRST. See \
                docs/LUMINANCE-RASTER.md.",
    },
    ClaimSpec {
        subject: MATERIAL_NAME,
        kind: ClaimKind::NonFinding(NO_MATERIAL_REGISTRY),
        basis: "The C++ reference keeps a registry the engine populates from outside, so \
                the mapping is not in the game data this build can read. Inventing a name \
                from the id would be a fabrication, so `name` is None.",
    },
    ClaimSpec {
        subject: MATERIAL_ID_MEANING,
        kind: ClaimKind::NonFinding(NO_MATERIAL_REGISTRY),
        basis: "Same non-finding as MATERIAL_NAME, stated about the id rather than its \
                spelling: the id is carried verbatim and nothing interprets it.",
    },
    ClaimSpec {
        subject: SAMPLER_ROLE,
        kind: ClaimKind::NonFinding(NO_ROLE_FIELD_IN_TEXTURE_ENTRY),
        basis: "REFUTES the earlier reason, which was that the 16 bytes before \
                {instance, group} were never read. They have now been read: the \
                first word is the entry's ordinal (6684/6684 references) and the \
                other 12 take four distinct values across 6684 references without \
                varying with the ordinal. So the header carries NO role field at \
                all, which is a stronger reason than 'not decoded'. Which shader \
                stage the game binds a given texture to is still unread.",
    },
    ClaimSpec {
        subject: TEXTURE_CONTENT_ROLE,
        kind: ClaimKind::Graded(EvidenceLevel::Inferred),
        basis: "assigned per resolved record from its own format word AND a \
                statistic over its own base mip (role::classify). A record whose \
                content fails the check for its format word is Unidentified, so the \
                check is an observation OF THIS RECORD and never a lookup of its \
                family's name. Graded INFERRED, and that is a LOWERING rather than a \
                convenience: one subject records both content families, and the \
                colour-and-mask reading cannot outrank the BC3 codec (bc3_claim_ceiling) \
                even though the normal-map reading could. Reported at the floor so one \
                static grade is honest for both. role::role_evidence answers the \
                different question 'did this record's own check run', which is \
                Observed for every variant. Deliberately NOT the same claim as \
                SAMPLER_ROLE.",
    },
    ClaimSpec {
        subject: NORMAL_MAP_CONTENT,
        kind: ClaimKind::Graded(EvidenceLevel::Verified),
        basis: "re-published from spore_texture::claims, which established it over \
                901/901 records of the 0x15 family in Spore_Content.package. \
                Delegated, never restated: a local copy of a republished fact is \
                how this crate once asserted the opposite of its own decoder.",
    },
    ClaimSpec {
        subject: DXT5_CARRIES_NO_COLOUR,
        kind: ClaimKind::Refuted {
            level: EvidenceLevel::Verified,
            refuted_by: spore_texture::claims::DXT5_SPEC_DEVIATIONS,
        },
        basis: "REFUTED, 2026-10-05, and kept under its original spelling. The claim \
                was a tautology of a broken codec rather than a measurement of any \
                record: green and blue read as zero under R5G5B5 on a 6-BIT word BY \
                CONSTRUCTION, because a 6-bit value cannot set bits 10..19. The block \
                stride was then charged at 8 bytes where the records hold 16. Read as \
                BC3 over all 1747 DXT5 records of Spore_Content.package: every \
                record's lane 0 takes at least 32 distinct values (max 235), lane 1 at \
                least 64 (max 255) and lane 2 at least 32 (max 235); 0 of 1747 have a \
                colour lane of two values or fewer; the documented record \
                0x2f4e681c:0x40632900:0x067a07f0 carries 4436 distinct RGB triples in \
                one 512x512 base mip. The old 'exactly 2 distinct RGB triples, (0,0,0) \
                and (8,0,0)' is what its own codec produced. See \
                docs/MATERIAL-ROLES.md section 4.",
    },
    ClaimSpec {
        subject: DXT5_LAYERS_ARE_DISTINCT,
        kind: ClaimKind::Refuted {
            level: EvidenceLevel::Verified,
            refuted_by: spore_texture::claims::LAYER_COUNT,
        },
        basis: "REFUTED, 2026-10-05, and kept under its original spelling. There are no \
                two layers: every raster record in the corpus is one chain, so \
                ResolvedTexture::layers has length 1 and the question has no subject. \
                The old 1233/1233 measurement compared the two HALVES OF ONE PICTURE \
                that an 8-byte block stride had split in two, which is why they \
                differed byte-for-byte; a correct 16-byte stride finds one image there. \
                The surviving statement is a different one: a model's two DXT5 \
                records are two distinct RECORDS, and TextureSlot keeps them apart.",
    },
    ClaimSpec {
        subject: COLOUR_AND_MASK_CONTENT,
        kind: ClaimKind::Graded(EvidenceLevel::Inferred),
        basis: "graded INFERRED and NOT Verified, because a statement about what a \
                decoded BC3 texel contains cannot outrank the codec that decoded it. \
                spore-texture grades BC3 at INFERRED (claims::BC3_CODEC): the 16-byte \
                stride is a byte count and the BC4 alpha half beats the alternative by \
                a factor of 20-50 on every shape, but no decode of a real record has \
                been compared against the game's own output. bc3_claim_ceiling() \
                returns that grade and a test pins this claim at or below it. The \
                per-record check that admits a record into TextureRole::ColourAndMask \
                is role::colour_and_mask_signature; the measurement behind it is in \
                DXT5_CARRIES_NO_COLOUR's refutation above.",
    },
    ClaimSpec {
        subject: MASK_PLANE_INDEPENDENT_OF_COLOUR,
        kind: ClaimKind::Graded(EvidenceLevel::Inferred),
        basis: "THIS CRATE'S OWN MEASUREMENT, so not delegated -- but graded at the BC3 \
                ceiling rather than Observed, because comparing the colour lanes with \
                the alpha lane depends on the colour half and that half's codec is \
                INFERRED upstream. Over all 1747 DXT5 records of Spore_Content.package \
                the mean absolute colour-to-alpha correlation has median 0.067 and \
                maximum 0.539, against a median mutual correlation of 0.510 among the \
                colour lanes themselves. Published per record on MipStatistics, which is \
                where a caller re-derives it, rather than as a number in this sentence. \
                NOT used as a gate: 467 of 1747 records fall below a 0.30 mean colour \
                correlation, so any correlation threshold would be a statistic rather \
                than a property.",
    },
    ClaimSpec {
        subject: MASK_LAYER_PURPOSE,
        kind: ClaimKind::NonFinding(MEANING_NOT_ESTABLISHED),
        basis: "the fourth lane's CONTENT is measured (one 8-bit channel, \
                statistically independent of the three colour planes, 1 to 256 distinct \
                values per record and CONSTANT in 46 of 1747) and its PURPOSE is \
                unknown. Nothing in the gmdl record, the raster envelope or the \
                texture-set entry distinguishes one candidate purpose from another -- \
                a specular mask and a gloss mask look identical in these records -- so \
                ChannelRole::MaskChannel says what the bytes are and no more. Its name \
                is about the SHAPE of the data, not a job. The subject keeps 'layer' \
                from the refuted two-layer model so the non-finding stays findable.",
    },
    ClaimSpec {
        subject: TEXTURE_SET_SHAPES,
        kind: ClaimKind::Graded(EvidenceLevel::Verified),
        basis: "over every gmdl texture set the repository's Python oracle walks in \
                Spore_Content.package (1819 records, 2381 blocks, 6684 references) \
                there are exactly TWO shapes: (DXT5, DXT5, 0x15) in 1922 sets and \
                (DXT5, 0x15) in 459 sets. The 0x15 record is always last. Recorded \
                as a corpus regularity, NOT as the basis of a role: the second \
                entry is DXT5 in 1922 sets and 0x15 in 459, so the ordinal alone \
                does not determine the content. Re-checked per model by \
                ModelTextures::observed_shape, which reports this model's own \
                shape rather than restating these numbers.",
    },
    ClaimSpec {
        subject: SAMPLER_WORD_IS_ENTRY_ORDINAL,
        kind: ClaimKind::Graded(EvidenceLevel::Verified),
        basis: "the first of the 16 bytes spore-gmdl skips equals the entry's \
                position within its texture set in 6684/6684 references. So the \
                ordinal this crate derives by arithmetic is the record's own \
                statement, and a reader that disagreed with it would be wrong.",
    },
    ClaimSpec {
        subject: GROUP_LOW_BYTE_IS_SLOT,
        kind: ClaimKind::Graded(EvidenceLevel::Verified),
        basis: "a referenced texture's group id carries the same ordinal in its low \
                byte in 6684/6684 references (slot 0 -> ...00, 1 -> ...01, \
                2 -> ...02). The corpus's references span SIX group families -- \
                0x40612900, 0x40632900, 0x40642900, 0x40652900, 0x40662900 and \
                0x406b2900 -- plus 150 references naming group id 0 outright, and \
                the agreement holds across all of them. A convenient CROSS-CHECK; \
                never a source of truth, because a naming convention would \
                survive the records it describes being reordered.",
    },
    ClaimSpec {
        subject: PER_MESH_TEXTURE_PARTITION,
        // Graded only when the division came out even AND the record's own
        // group-id ordinals agreed with it; a non-finding otherwise. The grade is
        // INFERRED in the first case and UNKNOWN in the second, and the second
        // must never borrow the first's confidence: a partition that was not made
        // carries no value at all.
        //
        // The reason code was RENAMED from
        // `texture_set_did_not_divide_by_mesh_count` to
        // `texture_set_not_cut_per_mesh` when the third scope was added. The old
        // spelling was FALSE for the new arm: that list does divide evenly, and
        // the cut was refused because the record contradicted the result.
        kind: ClaimKind::GradedOrNonFinding {
            level: EvidenceLevel::Inferred,
            reason: TEXTURE_SET_NOT_CUT_PER_MESH,
        },
        basis: "the cut is made by dividing the model's flat texture_refs by its \
                mesh count, which is sound only if every material-info block \
                contributed the same number of references AND the list is the \
                record's whole texture set. Measured three ways: spore-gmdl's own \
                reader partitions 3589 of 3589 mesh-bearing gmdl records in \
                Spore_Content.package evenly and not one unevenly; mesh_count \
                equals the block count in 1819/1819 records the Python oracle \
                walks; and every slot's derived ordinal equals the ordinal its \
                referenced group id states, which is what refuses the 100 records \
                whose material-info walk stops before its last declared block. \
                Still an inference from regularities, not a byte-level link -- see \
                MATERIAL_BLOCK_IS_MESH.",
    },
    ClaimSpec {
        subject: CUT_CONTRADICTED_BY_RECORD,
        // A yes/no observation about this record: did the record's own group-id
        // ordinals disagree with the cut? Graded when at least one did, a
        // non-finding when none did. Neither arm claims the list is short -- that
        // is spore-gmdl's typed `StopReason` to state, and it is not reachable
        // from a `&GmdlModel` -- only what the record's own bytes say here.
        kind: ClaimKind::GradedOrNonFinding {
            level: EvidenceLevel::Observed,
            reason: NO_CUT_CONTRADICTION,
        },
        basis: "a texture-set entry's ordinal is stated twice -- by the entry's \
                first header word and by the referenced record's group id low \
                byte, in agreement in 6684/6684 references. Dividing a COMPLETE \
                flat list by the mesh count reproduces those ordinals; dividing a \
                list the walk stopped reading does not. Measured over all 4209 \
                gmdl records of Spore_Content.package, the records whose group \
                ids contradict the cut are exactly the 100 whose material-info \
                walk stops before its last declared block, and no others. NOT \
                CHECKED: a reference whose group id is 0 belongs to no group \
                family and so states no ordinal; 150 references across 132 \
                records do. Such a slot is skipped rather than read as ordinal 0, \
                because reading it that way refused the cut of 132 records whose \
                cut is sound -- the same mistake in the opposite direction. The \
                remaining hole is that a cut made ENTIRELY of such slots is \
                unchecked, and an unchecked cut is refused rather than offered. \
                Both numbers are counted in the corpus census.",
    },
    ClaimSpec {
        subject: MATERIAL_BLOCK_IS_MESH,
        kind: ClaimKind::NonFinding(BLOCK_ORDER_NOT_ESTABLISHED),
        basis: "mesh_count == material-info block count in 1819/1819 records, and \
                an even division in 3589/3589 records spore-gmdl walks, are strong \
                regularities and still not a link: nothing in the record says block \
                i is mesh i, only that there are as many of each. WORSE THAN \
                UNPROVEN, and measured: every one of the 1685 cut records with \
                more than one mesh has all of its per-mesh slices naming the SAME \
                texture set, so permuting material-info blocks changes nothing a \
                renderer can observe. The ordering is therefore UNFALSIFIABLE from \
                the reference data in this corpus, not merely unproven -- settling \
                it needs something outside the texture references (a per-mesh 0x210 \
                parameter block, geometry, or UV evidence) that nothing here has \
                read. The partition is offered as INFERRED and labelled as such, \
                never as a resolved binding.",
    },
    ClaimSpec {
        subject: TEXTURE_SET_ENTRY_LAYOUT,
        kind: ClaimKind::Graded(EvidenceLevel::Observed),
        basis: "read directly off real records: 16 bytes, of which the first is the \
                entry's ordinal, then the referenced record's instance and group \
                ids. The remaining 12 bytes are carried and not interpreted.",
    },
    ClaimSpec {
        subject: MATERIAL_TEXTURE_OWNERSHIP,
        kind: ClaimKind::NonFinding(MATERIAL_BLOCKS_NOT_ASSOCIATED),
        basis: "spore-gmdl pushes every texture set's refs into one list and never \
                records which block they came from, so the block boundaries are \
                unrecoverable from GmdlModel. What this crate offers instead is a \
                per-mesh PARTITION derived by division, graded INFERRED -- see \
                PER_MESH_TEXTURE_PARTITION. The non-finding is about the walk, not \
                about the format.",
    },
    ClaimSpec {
        subject: MATERIAL_ID_READ_VERBATIM,
        kind: ClaimKind::Graded(EvidenceLevel::Observed),
        basis: "the id is one little-endian word per mesh in the gmdl mesh table; reading it \
                is an observation, and nothing interprets it.",
    },
    ClaimSpec {
        subject: MATERIAL_BINDING_COUNT,
        kind: ClaimKind::Graded(EvidenceLevel::Observed),
        basis: "a count of what this build bound, which is the model's whole texture set \
                because MATERIAL_TEXTURE_OWNERSHIP is a non-finding.",
    },
];

/// The specification for `subject`, or `None` when this build records no such
/// claim. Absence is not `UNKNOWN`: it means "this build does not say".
pub fn spec_of(subject: &str) -> Option<&'static ClaimSpec> {
    ALL_CLAIMS.iter().find(|spec| spec.subject == subject)
}

/// The grade `subject` is recorded at, or [`EvidenceLevel::Unknown`] when this
/// build records nothing for it.
pub fn grade_of(subject: &str) -> EvidenceLevel {
    spec_of(subject).map_or(EvidenceLevel::Unknown, |spec| spec.kind.level())
}

/// The strongest grade any claim **in this crate** may carry about what a
/// decoded BC3 texel contains.
///
/// Delegates to [`spore_texture::claims::BC3_CODEC`] rather than declaring a
/// level of its own, so the ceiling moves when the crate that measured the codec
/// moves it. It is `INFERRED` today, and the reason it is not `VERIFIED` is
/// specific and checkable: the 16-byte block stride is a byte count over 4 386
/// records and the BC4 alpha half is measured against the alternative by a
/// factor of 20–50 on every shape, but **no decode of a real Spore DXT5 record
/// has ever been compared against the game's own output**. That comparison is
/// the test that would raise the ceiling, and nothing in this repository has run
/// it.
///
/// A claim that violated the ceiling would be asserting more about Spore's texels
/// than the crate that decoded them does — which is precisely how the refuted
/// `dxt5_carries_no_colour` came to exist.
pub fn bc3_claim_ceiling() -> EvidenceLevel {
    spore_texture::claims::grade_of(spore_texture::claims::BC3_CODEC)
}

/// Every subject in this crate that is a [`ClaimKind::Refuted`], with the
/// upstream subject that measured each refutation.
///
/// Exposed so a reader (and a test) can enumerate the refuted claims without
/// walking [`ALL_CLAIMS`] and matching on a variant.
pub fn refuted_subjects() -> Vec<(&'static str, &'static str)> {
    ALL_CLAIMS
        .iter()
        .filter_map(|spec| spec.kind.refuted_by().map(|by| (spec.subject, by)))
        .collect()
}

/// Builds the non-finding for `subject`.
///
/// The reason code comes from the subject's own specification, so a claim can
/// never be recorded with a reason that disagrees with its documented grade.
pub fn non_finding(subject: &'static str) -> Fact<&'static str> {
    let reason = spec_of(subject)
        .and_then(|spec| spec.kind.reason())
        .expect("subject is not declared as a claim that can be a non-finding");
    Fact::unavailable(reason)
}

/// Builds the graded value for `subject`, sourced from `basis`.
///
/// # Panics
///
/// If `subject` is declared a non-finding. A non-finding carrying a value is
/// exactly the failure this crate exists to prevent, so it is refused at the
/// construction site rather than published.
pub fn graded(
    subject: &'static str,
    basis: &'static str,
    value: &'static str,
) -> Fact<&'static str> {
    let level = grade_of(subject);
    assert_ne!(
        level,
        EvidenceLevel::Unknown,
        "{subject} is declared a non-finding and must not carry a value"
    );
    assert!(
        spec_of(subject).is_some(),
        "{subject} is not a declared claim subject"
    );
    Fact::new(
        level,
        EvidenceState::Derived,
        vec![Provenance::derived(basis)],
        value,
    )
}

/// Builds the graded value for `subject` read verbatim out of a committed
/// table, which is [`spore_core::evidence::EvidenceState::Persisted`] rather
/// than derived.
pub fn graded_persisted(
    subject: &'static str,
    basis: &'static str,
    value: &'static str,
) -> Fact<&'static str> {
    let level = grade_of(subject);
    assert_ne!(
        level,
        EvidenceLevel::Unknown,
        "{subject} is declared a non-finding and must not carry a value"
    );
    assert!(
        spec_of(subject).is_some(),
        "{subject} is not a declared claim subject"
    );
    Fact::persisted(level, basis, value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subjects_are_unique_and_the_table_is_sorted_by_declaration() {
        for (index, spec) in ALL_CLAIMS.iter().enumerate() {
            assert!(!spec.subject.is_empty());
            assert!(
                spec.basis.len() > 30,
                "{}: a basis too thin to justify a grade",
                spec.subject
            );
            for other in ALL_CLAIMS.iter().skip(index + 1) {
                assert_ne!(spec.subject, other.subject, "duplicate subject");
            }
        }
        assert_eq!(spec_of("no_such_subject"), None);
        assert_eq!(grade_of("no_such_subject"), EvidenceLevel::Unknown);
    }

    #[test]
    fn every_non_finding_is_unknown_missing_and_provenance_free() {
        for spec in ALL_CLAIMS {
            let Some(reason) = spec.kind.reason() else {
                continue;
            };
            // A `GradedOrNonFinding` subject is graded when its canonical table
            // has an entry and a non-finding when it does not. The non-finding
            // arm is still checked, because it is the one this crate actually
            // takes for the documented asset's three groups.
            match spec.kind {
                ClaimKind::NonFinding(_) => assert_eq!(
                    spec.kind.level(),
                    EvidenceLevel::Unknown,
                    "{}: a subject that can never carry a value is UNKNOWN",
                    spec.subject
                ),
                ClaimKind::GradedOrNonFinding { .. } => assert_ne!(
                    spec.kind.level(),
                    EvidenceLevel::Unknown,
                    "{}: a subject that can carry a value has a real grade",
                    spec.subject
                ),
                // A refutation carries a VALUE -- the falsity of the old claim --
                // so it is graded like any other available claim. Asserting it
                // here is what stops a refutation from decaying into a
                // non-finding, which would be indistinguishable from "we never
                // knew".
                ClaimKind::Refuted { .. } | ClaimKind::Graded(_) => {}
            }
            let fact = non_finding(spec.subject);
            assert!(fact.is_unavailable(), "{}", spec.subject);
            assert_eq!(fact.level(), EvidenceLevel::Unknown, "{}", spec.subject);
            assert_eq!(
                fact.evidence_state(),
                EvidenceState::Missing,
                "{}",
                spec.subject
            );
            assert_eq!(fact.reason(), Some(reason), "{}", spec.subject);
            assert_eq!(fact.value(), None, "{}", spec.subject);
            assert!(
                fact.provenance().is_empty(),
                "{}: a non-finding carries no provenance to repeat",
                spec.subject
            );
        }
    }

    #[test]
    fn every_graded_claim_carries_its_grade_and_provenance() {
        for spec in ALL_CLAIMS {
            if !spec.kind.can_carry_value() {
                continue;
            }
            let level = spec.kind.level();
            assert_ne!(level, EvidenceLevel::Unknown, "{}", spec.subject);
            let derived = graded(spec.subject, PROV_RESOLVE, "value");
            assert_eq!(derived.level(), level, "{}", spec.subject);
            assert_eq!(derived.evidence_state(), EvidenceState::Derived);
            assert_eq!(derived.provenance().len(), 1);
            assert_eq!(derived.value(), Some(&"value"));
            assert_eq!(derived.reason(), None);

            let persisted = graded_persisted(spec.subject, PROV_TYPE_NAMES, "name");
            assert_eq!(persisted.level(), level, "{}", spec.subject);
            assert_eq!(persisted.evidence_state(), EvidenceState::Persisted);
        }
    }

    #[test]
    fn a_graded_or_non_finding_subject_carries_both_arms_consistently() {
        for subject in [ASSUMED_RECORD_TYPE_NAME, RESOLVED_GROUP_NAME] {
            let spec = spec_of(subject).unwrap_or_else(|| panic!("{subject}"));
            assert!(spec.kind.can_carry_value(), "{subject}");
            let level = spec.kind.level();
            let reason = spec.kind.reason().unwrap_or_else(|| panic!("{subject}"));
            let value = graded_persisted(subject, PROV_TYPE_NAMES, "named");
            assert_eq!(value.level(), level, "{subject}");
            assert_eq!(
                value.evidence_state(),
                EvidenceState::Persisted,
                "{subject}"
            );
            let absent = non_finding(subject);
            assert_eq!(absent.reason(), Some(reason), "{subject}");
            assert_eq!(absent.level(), EvidenceLevel::Unknown, "{subject}");
        }
    }

    #[test]
    #[should_panic(expected = "must not carry a value")]
    fn a_non_finding_subject_cannot_be_given_a_value() {
        let _ = graded(MATERIAL_NAME, PROV_MATERIAL_REGISTRY, "Leather");
    }

    #[test]
    fn the_two_material_non_findings_are_the_invariant_this_crate_exists_for() {
        assert_eq!(grade_of(MATERIAL_NAME), EvidenceLevel::Unknown);
        assert_eq!(grade_of(MATERIAL_ID_MEANING), EvidenceLevel::Unknown);
        assert_eq!(grade_of(SAMPLER_ROLE), EvidenceLevel::Unknown);
        assert_eq!(
            non_finding(MATERIAL_NAME).reason(),
            Some(NO_MATERIAL_REGISTRY)
        );
        assert_eq!(
            non_finding(SAMPLER_ROLE).reason(),
            Some(NO_ROLE_FIELD_IN_TEXTURE_ENTRY)
        );
    }

    #[test]
    fn the_sampler_role_non_finding_is_now_backed_by_a_measurement_not_an_omission() {
        // The reason code changed from "the header was not decoded" to "the
        // header carries no role field", because the header has since been read.
        // What must NOT change is that the claim is still UNKNOWN: a stronger
        // reason for a non-finding must never promote it to a value.
        assert_eq!(grade_of(SAMPLER_ROLE), EvidenceLevel::Unknown);
        assert_ne!(grade_of(SAMPLER_ROLE), EvidenceLevel::Inferred);
        assert_ne!(grade_of(SAMPLER_ROLE), EvidenceLevel::Observed);
        assert_eq!(
            grade_of(SAMPLER_WORD_IS_ENTRY_ORDINAL),
            EvidenceLevel::Verified
        );
        assert_eq!(grade_of(GROUP_LOW_BYTE_IS_SLOT), EvidenceLevel::Verified);
        assert_eq!(grade_of(TEXTURE_SET_SHAPES), EvidenceLevel::Verified);
        // And the content role, which IS answerable, must not be confused with
        // it: two different subjects. The content subject sits at the BC3
        // ceiling (`INFERRED`), and the sampler non-finding at `UNKNOWN` — which
        // is a *lower* grade, so `!=` alone would be satisfied by any change and
        // is asserted alongside the exact pair.
        assert_eq!(grade_of(TEXTURE_CONTENT_ROLE), bc3_claim_ceiling());
        assert_eq!(grade_of(SAMPLER_ROLE), EvidenceLevel::Unknown);
        assert_ne!(grade_of(TEXTURE_CONTENT_ROLE), grade_of(SAMPLER_ROLE));
    }

    #[test]
    fn the_per_mesh_partition_is_inferred_when_made_and_absent_when_not() {
        assert_eq!(
            grade_of(PER_MESH_TEXTURE_PARTITION),
            EvidenceLevel::Inferred
        );
        assert!(spec_of(PER_MESH_TEXTURE_PARTITION)
            .unwrap()
            .kind
            .reason()
            .is_some());
        assert_eq!(
            non_finding(PER_MESH_TEXTURE_PARTITION).reason(),
            Some(TEXTURE_SET_NOT_CUT_PER_MESH)
        );
        assert_eq!(
            grade_of(CUT_CONTRADICTED_BY_RECORD),
            EvidenceLevel::Observed
        );
        assert_eq!(
            non_finding(CUT_CONTRADICTED_BY_RECORD).reason(),
            Some(NO_CUT_CONTRADICTION)
        );
        assert_eq!(grade_of(MATERIAL_BLOCK_IS_MESH), EvidenceLevel::Unknown);
        assert_eq!(
            non_finding(MATERIAL_BLOCK_IS_MESH).reason(),
            Some(BLOCK_ORDER_NOT_ESTABLISHED)
        );
    }

    #[test]
    fn the_colour_and_mask_finding_is_inferred_and_its_purpose_is_not() {
        // Content known, purpose unknown, and the two must not be merged. The
        // old test asserted `dxt5_carries_no_colour` was VERIFIED; the corpus
        // refuted it, so this asserts the opposite about the same subjects.
        assert_eq!(
            grade_of(COLOUR_AND_MASK_CONTENT),
            EvidenceLevel::Inferred,
            "a reading OF a BC3 decode cannot outrank the BC3 codec"
        );
        assert_eq!(
            grade_of(MASK_PLANE_INDEPENDENT_OF_COLOUR),
            EvidenceLevel::Inferred,
            "comparing the colour lanes with the alpha lane depends on the colour \
             half, whose codec is Inferred upstream"
        );
        assert_eq!(
            grade_of(TEXTURE_CONTENT_ROLE),
            EvidenceLevel::Inferred,
            "one subject records both content families, so it sits at the floor"
        );
        assert_eq!(grade_of(NORMAL_MAP_CONTENT), EvidenceLevel::Verified);
        // Content known, purpose unknown.
        assert_eq!(grade_of(MASK_LAYER_PURPOSE), EvidenceLevel::Unknown);
        assert_eq!(
            non_finding(MASK_LAYER_PURPOSE).reason(),
            Some(MEANING_NOT_ESTABLISHED)
        );
    }

    #[test]
    fn the_three_refuted_subjects_are_recorded_not_deleted() {
        // A refutation that removes its subject leaves the next reader with
        // nothing to contradict, which is how a wrong belief gets re-derived. The
        // original spellings are kept precisely so the refutation is findable
        // FROM the thing it refutes.
        for subject in [
            LAYER_COUNT_IS_DERIVED,
            DXT5_CARRIES_NO_COLOUR,
            DXT5_LAYERS_ARE_DISTINCT,
            DXT5_SPEC_DEVIATIONS,
        ] {
            let spec = spec_of(subject).unwrap_or_else(|| panic!("{subject} was deleted"));
            assert!(spec.kind.is_refuted(), "{subject} must stay a refutation");
            assert!(
                spec.kind.refuted_by().is_some(),
                "{subject} names no measuring subject"
            );
            assert!(
                spec.basis.to_ascii_lowercase().contains("refuted"),
                "{subject}: the refutation must be visible in the basis itself"
            );
            // And the grade is of the REFUTATION, which is itself a corpus
            // measurement rather than a belief.
            assert_eq!(spec.kind.level(), EvidenceLevel::Verified, "{subject}");
        }
        // The two that were graded `VERIFIED` for a false statement still grade
        // `VERIFIED` — for their falsity. Nothing about the old grade leaked into
        // the old claim.
        assert_eq!(grade_of(DXT5_CARRIES_NO_COLOUR), EvidenceLevel::Verified);
        assert_eq!(grade_of(DXT5_LAYERS_ARE_DISTINCT), EvidenceLevel::Verified);
        // And a refutation carries a value: it is not a non-finding, because
        // "we looked and found nothing" would be indistinguishable from never
        // having looked.
        assert!(spec_of(DXT5_CARRIES_NO_COLOUR)
            .unwrap()
            .kind
            .can_carry_value());
    }

    #[test]
    fn every_refutation_delegates_to_a_subject_the_measuring_crate_still_declares() {
        // The delegation is checked, not assumed: a `refuted_by` that names a
        // subject `spore_texture` has since deleted would silently turn this
        // crate's evidence pointer into a dangling string. This is the defect
        // class that produced the refuted `dxt5_carries_no_colour` in the first
        // place -- a local restatement drifting from the crate that measured it.
        let refuted = refuted_subjects();
        assert_eq!(
            refuted.len(),
            4,
            "four subjects are refuted; adding one must come with a basis saying so"
        );
        for (subject, by) in refuted {
            let upstream = spore_texture::claims::claim(by)
                .unwrap_or_else(|| panic!("{subject} names {by}, which spore-texture dropped"));
            assert_ne!(
                upstream.meaning.level(),
                EvidenceLevel::Unknown,
                "{subject}: delegated to a subject with no established meaning"
            );
        }
    }

    #[test]
    fn a_claim_about_decoded_bc3_texels_never_exceeds_the_codecs_own_grade() {
        // The rule that makes a refutation stick: this crate may not re-state,
        // let alone upgrade, a reading of a codec the other crate graded.
        let ceiling = bc3_claim_ceiling();
        assert_eq!(ceiling, EvidenceLevel::Inferred);
        assert_eq!(
            ceiling,
            spore_texture::claims::grade_of(spore_texture::claims::BC3_CODEC),
            "the ceiling is whatever the crate that measured the codec says"
        );
        for subject in [
            COLOUR_AND_MASK_CONTENT,
            MASK_PLANE_INDEPENDENT_OF_COLOUR,
            TEXTURE_CONTENT_ROLE,
        ] {
            assert!(
                grade_of(subject) <= ceiling,
                "{subject} is a statement about decoded BC3 texels and may not outrank \
                 the BC3 codec itself"
            );
        }
        // Three subjects sit exactly AT the ceiling, which is the honest place
        // for them: no promotion, no penalty.
        assert_eq!(grade_of(COLOUR_AND_MASK_CONTENT), ceiling);
        assert_eq!(grade_of(MASK_PLANE_INDEPENDENT_OF_COLOUR), ceiling);
        assert_eq!(grade_of(TEXTURE_CONTENT_ROLE), ceiling);
        // And the counterpart, which is why the normal-map finding CAN be
        // Verified: it is a claim about 0x15 records, whose texel layout upstream
        // grades Verified on an independent measurement (lane 0 tracks the derived
        // Z with correlation 0.954).
        assert_eq!(grade_of(NORMAL_MAP_CONTENT), EvidenceLevel::Verified);
        assert!(
            spore_texture::claims::grade_of(spore_texture::claims::LUMINANCE_TEXEL_LAYOUT)
                >= EvidenceLevel::Verified
        );
    }

    #[test]
    fn the_assumed_type_is_inferred_and_never_observed() {
        // The single most important grade in this crate: an observed one would
        // claim the record declares its own type, which it does not.
        assert_eq!(grade_of(ASSUMED_RECORD_TYPE), EvidenceLevel::Inferred);
        assert_ne!(grade_of(ASSUMED_RECORD_TYPE), EvidenceLevel::Observed);
        assert_ne!(grade_of(ASSUMED_RECORD_TYPE), EvidenceLevel::Verified);
    }
}
