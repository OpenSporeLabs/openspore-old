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
pub const LAYER_COUNT_IS_DERIVED: &str = "layer_count_is_derived";

/// Re-publication of [`spore_texture::claims::DXT5_SPEC_DEVIATIONS`], so a
/// caller reading only this crate's claims still learns that the decoded texels
/// are deliberately *not* BC3-spec DXT5.
pub const DXT5_SPEC_DEVIATIONS: &str = "dxt5_spec_deviations";

/// Re-publication of [`spore_texture::claims::ENVELOPE_FIELD_10_MEANING`].
pub const ENVELOPE_FIELD_10_MEANING: &str = "envelope_field_10_meaning";

/// Re-publication of [`spore_texture::claims::ENVELOPE_FIELD_18_MEANING`].
pub const ENVELOPE_FIELD_18_MEANING: &str = "envelope_field_18_meaning";

/// Re-publication of [`spore_texture::claims::ENVELOPE_FIELD_1C_MEANING`].
pub const ENVELOPE_FIELD_1C_MEANING: &str = "envelope_field_1c_meaning";

/// Re-publication of [`spore_texture::claims::LUMINANCE_FOURCC`].
pub const LUMINANCE_FOURCC_FAMILY: &str = "luminance_fourcc_family";

/// What the material id names. Nothing, in this build.
pub const MATERIAL_NAME: &str = "material_name";

/// What the material id *means*. Nothing, in this build.
pub const MATERIAL_ID_MEANING: &str = "material_id_meaning";

/// Which sampler stage a texture binding feeds. Unknown: the 16 bytes a gmdl
/// texture-set entry skips before `{instance, group}` are not decoded.
pub const SAMPLER_ROLE: &str = "sampler_role";

/// The observed layout of one gmdl texture-set entry: 16 skipped bytes, then the
/// referenced record's instance and group ids.
pub const TEXTURE_SET_ENTRY_LAYOUT: &str = "texture_set_entry_layout";

/// Which texture-set block belongs to which material id. Not established, so a
/// [`MaterialModel`](crate::MaterialModel)'s bindings are the whole model's
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
pub const TEXTURE_ENTRY_HEADER_NOT_DECODED: &str = "texture_entry_header_not_decoded";

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

// ---------------------------------------------------------------------------
// Provenance references — the paths a graded claim is sourced from
// ---------------------------------------------------------------------------

/// The gmdl walk that skips the texture-set entry header.
pub const PROV_GMDL_WALK: &str = "crates/spore-gmdl/src/parse.rs";

/// This crate's own resolution step, for statements derived here.
pub const PROV_RESOLVE: &str = "crates/spore-material/src/resolve.rs";

/// The raster envelope byte-pinned against real records.
pub const PROV_MATERIALS_DESIGN: &str = "docs/MATERIALS-DESIGN.md §1";

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
}

impl ClaimKind {
    /// The grade this claim is recorded at.
    pub const fn level(self) -> EvidenceLevel {
        match self {
            Self::Graded(level) | Self::GradedOrNonFinding { level, .. } => level,
            Self::NonFinding(_) => EvidenceLevel::Unknown,
        }
    }

    /// The reason code, when this claim can be a non-finding.
    pub const fn reason(self) -> Option<&'static str> {
        match self {
            Self::Graded(_) => None,
            Self::NonFinding(reason) | Self::GradedOrNonFinding { reason, .. } => Some(reason),
        }
    }

    /// Whether this claim may carry a value.
    pub const fn can_carry_value(self) -> bool {
        !matches!(self, Self::NonFinding(_))
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
        kind: ClaimKind::Graded(EvidenceLevel::Observed),
        basis: "No layer-count field exists; the count comes from dividing the payload by \
                the layer stride, and yields 2 on every real 512x512 DXT5 record measured \
                (0x2f4e681c:0x40632900/01:0x067a07f0 on 2026-10-04).",
    },
    ClaimSpec {
        subject: DXT5_SPEC_DEVIATIONS,
        kind: ClaimKind::Graded(EvidenceLevel::Verified),
        basis: "pinned byte-for-byte by spore-texture against two independent \
                implementations and the hand-computed golden values.",
    },
    ClaimSpec {
        subject: ENVELOPE_FIELD_10_MEANING,
        kind: ClaimKind::NonFinding(MEANING_NOT_ESTABLISHED),
        basis: "re-published from spore_texture::claims; no decoder reads the word.",
    },
    ClaimSpec {
        subject: ENVELOPE_FIELD_18_MEANING,
        kind: ClaimKind::NonFinding(MEANING_NOT_ESTABLISHED),
        basis: "re-published from spore_texture::claims; no decoder reads the word.",
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
        kind: ClaimKind::NonFinding(TEXTURE_ENTRY_HEADER_NOT_DECODED),
        basis: "The 16 bytes before {instance, group} in a texture-set entry are skipped, \\
                not decoded. Any concrete texture purpose -- colour, gloss, surface \\
                orientation -- would be a guess about a shader nobody here has read.",
    },
    ClaimSpec {
        subject: TEXTURE_SET_ENTRY_LAYOUT,
        kind: ClaimKind::Graded(EvidenceLevel::Observed),
        basis: "read directly off real records by spore-gmdl's walk; the skip is in the \
                source at read_texture_set.",
    },
    ClaimSpec {
        subject: MATERIAL_TEXTURE_OWNERSHIP,
        kind: ClaimKind::NonFinding(MATERIAL_BLOCKS_NOT_ASSOCIATED),
        basis: "spore-gmdl pushes every texture set's refs into one list and never records \
                which block they came from, so no per-material share can be claimed.",
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
                ClaimKind::Graded(_) => unreachable!("Graded has no reason code"),
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
            Some(TEXTURE_ENTRY_HEADER_NOT_DECODED)
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
