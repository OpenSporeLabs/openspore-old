//! Every claim this crate makes that is **not** verified, with its grade.
//!
//! # Why this module exists
//!
//! A decoder that returns a `f32` has already lost the only interesting
//! question about it. `Mesh::normals` holds three numbers per vertex; nothing in
//! the type says whether those numbers are a signed direction, a biased
//! direction, a colour, or a byte pattern someone decided to render. The
//! repository's rule (see `spore_core::evidence`) is that a claim carries its
//! grade, so that an inference and an observation stay distinguishable after
//! they have left the decoder.
//!
//! So: every statement below is one this implementation *reproduces* without
//! understanding, and every one is graded. The grades are the repository's
//! canonical 7-rung scale from `spore_core::EvidenceLevel`.
//!
//! # What is *not* here
//!
//! Anything this crate verifies by construction. The version-8 section walk,
//! the big-endian `refCount`, the byte sizes of the documented declaration types
//! and the mesh layout are reproduced from two independent implementations and
//! checked byte-for-byte against the committed fixture; they carry no claim
//! object because there is no residual doubt to record. This module is not a
//! list of everything the crate believes - it is a list of what it is guessing.
//!
//! # Consuming a grade
//!
//! [`claim`] returns `None` for a subject this build does not record. That is
//! deliberate: a caller asking about `unknown_key_meaning` on a build that never
//! recorded it must get "no claim", not a default grade that reads like an
//! answer.

use std::sync::OnceLock;

use spore_core::evidence::{EvidenceLevel, EvidenceState, Fact, Provenance};

/// How the `NORMAL`/`UBYTE4` element's four bytes become three floats.
pub const VERTEX_NORMAL_ENCODING: &str = "vertex_normal_encoding";

/// What the `refCount` big-endian key table actually references.
pub const REFERENCED_FILES_ROLE: &str = "referenced_files_role";

/// Whether the record's own bounding box covers only this mesh set or also LOD
/// and morph targets.
pub const FILE_BOUNDS_COVERAGE: &str = "file_bounds_coverage";

/// Whether the trailing bone-range / anim-data / baked-deform framing is
/// universal or family-specific.
pub const TRAILER_FRAMING: &str = "trailer_framing";

/// What the trailing three-word key identifies.
pub const UNKNOWN_KEY_MEANING: &str = "unknown_key_meaning";

/// What the 16 opaque bytes of a texture-set entry hold.
pub const TEXTURE_SET_CONTENTS: &str = "texture_set_contents";

/// What a material-info shader-data payload holds.
pub const SHADER_DATA_PAYLOADS: &str = "shader_data_payloads";

/// What the four-byte word between the material ids and the material-info count
/// means.
pub const MESH_TABLE_ZERO_WORD: &str = "mesh_table_zero_word";

/// Why a `BLENDINDICES` slot is always a multiple of three.
pub const BLEND_INDEX_STRIDE: &str = "blend_index_stride";

/// What a `{start, count}` entry of the trailer's bone-range table counts.
pub const BONE_RANGE_MEANING: &str = "bone_range_meaning";

/// Whether the trailer's anim data carries bone-identity words, and what it
/// points at instead.
pub const ANIM_DATA_BONE_IDENTITIES: &str = "anim_data_bone_identities";

/// What the `type_code` word of a vertex element holds.
pub const VERTEX_ELEMENT_TYPE_CODE: &str = "vertex_element_type_code";

/// What a descriptor that declares bone indices and no weights means.
pub const WEIGHTLESS_BLEND_INDICES: &str = "weightless_blend_indices";

/// What the joint transforms the vertex stream names actually live in.
pub const JOINT_TRANSFORM_LOCATION: &str = "joint_transform_location";

/// What the RW4 section codes of a referenced container are called.
pub const RW4_SECTION_CODE_NAMES: &str = "rw4_section_code_names";

/// One recorded claim: what is claimed, how strongly, and on what basis.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GmdlClaim {
    /// Stable identifier, e.g. `"vertex_normal_encoding"`.
    pub subject: &'static str,
    /// The claim in one sentence, in the repository's evidence vocabulary.
    pub statement: &'static str,
    /// The envelope: grade, how it was obtained, and where it came from.
    pub fact: Fact<&'static str>,
}

/// The claims, in declaration order. Lazily built because [`Fact`] cannot be
/// constructed in a `const` context.
fn claims() -> &'static [GmdlClaim] {
    static CLAIMS: OnceLock<Vec<GmdlClaim>> = OnceLock::new();
    CLAIMS.get_or_init(|| {
        vec![
            claim_of(
                VERTEX_NORMAL_ENCODING,
                "NORMAL/UBYTE4 bytes are copied as byte/255 into [0,1]; the bytes are reproduced, not \
                 decoded. Whether Spore stores a signed, biased or scaled normal in them is unknown, so \
                 no bias correction is applied and none should be assumed downstream.",
                EvidenceLevel::Inferred,
                "docs/ASSET-PATH.md",
            ),
            claim_of(
                REFERENCED_FILES_ROLE,
                "Every key in the refCount table names an rw4 container (type 0x2F4E681B) in the same \
                 package: 7989 of 7989 resolve against the real DBPF index. 466 of the 1006 distinct \
                 targets carry RW4 section codes 0x70001, 0x70002, 0x70003, 0x7000C, 0x7000F and \
                 0xFF0001, which the repository's two name tables call KeyframeAnim / Skeleton / \
                 AnimationSkin / Animations; the other 540 carry only 0x80003. That a model is \
                 *animated by* a given key is not established: only 44 of the 315 records that \
                 declare bone ranges reference anything at all, and no joint transform has been \
                 decoded.",
                EvidenceLevel::Supported,
                "docs/ANIMATION.md",
            ),
            claim_of(
                FILE_BOUNDS_COVERAGE,
                "The record's own bbox/radius is larger than the bbox of its own vertices, so it is \
                 read as covering something wider than this mesh set (LOD range or morph targets). The \
                 reason is undecided; the values are reproduced verbatim either way.",
                EvidenceLevel::Inferred,
                "docs/ASSET-PATH.md",
            ),
            claim_of(
                TRAILER_FRAMING,
                "The bone-range / anim-data / baked-deform trailer framing is validated only on the \
                 mini.gmdl and CellImages families. Real cell-stage records at groups 0x40616201 and \
                 0x40616202 carry a larger opaque baked-deform trailer that overruns this walk, which \
                 is why the tail is walked best-effort and reported rather than enforced.",
                EvidenceLevel::Supported,
                "docs/ASSET-PATH.md",
            ),
            claim_of(
                UNKNOWN_KEY_MEANING,
                "The trailing three words are a resource key in {instance, group, type} order: the one \
                 real record captured reads (0, 0xFFFFFFFF, 0) and that middle word is exactly the \
                 spore_core::WILDCARD. What the key points at - an owning block record - is inferred \
                 from the shape alone.",
                EvidenceLevel::Inferred,
                "tests/expected/real_gmdl_1006.json",
            ),
            claim_of(
                TEXTURE_SET_CONTENTS,
                "A 0x20D texture-set entry is a sampler id plus 12 opaque bytes plus the referenced \
                 texture's instance/group. The sampler and the 12 bytes are skipped: the framing is \
                 validated by the record's end offset matching, the contents are not interpreted.",
                EvidenceLevel::Supported,
                "docs/ASSET-PATH.md",
            ),
            claim_of(
                SHADER_DATA_PAYLOADS,
                "Non-0x20D material-info entries are skipped by the RenderWare ShaderData size table \
                 (49 ids). The table's framing is validated by the record's end offset matching; the \
                 payload contents are not interpreted, and an id missing from it is refused rather than \
                 skipped by a guessed length.",
                EvidenceLevel::Supported,
                "docs/ASSET-PATH.md",
            ),
            claim_of(
                MESH_TABLE_ZERO_WORD,
                "The four bytes between the material ids and the material-info count read zero on \
                 every record decoded so far. The word is skipped rather than read, because a zero \
                 value is not evidence of a meaning.",
                EvidenceLevel::Observed,
                "src/assets/Gmdl.hpp",
            ),
            claim_of(
                BLEND_INDEX_STRIDE,
                "Every value stored in a BLENDINDICES element is a multiple of 3, in 2109 of the 2109 \
                 corpus records that declare one, and the gap between consecutive distinct values is \
                 3 in 48623 cases against 1163 for every larger gap combined. The joint table is \
                 therefore addressed with a stride of three and the joint id is slot / 3. Why the \
                 stride is three - three words per joint, three copies of the skeleton, or a \
                 reserved slot - is not determined, so slot / 3 is a reading and not a definition.",
                EvidenceLevel::Supported,
                "docs/ANIMATION.md",
            ),
            claim_of(
                BONE_RANGE_MEANING,
                "The trailer's bone-range entries are read as {start, count} pairs and reproduced \
                 verbatim; the sum of their counts bounds the joint ids the vertex stream names in \
                 313 of the 315 records that declare any (every BLENDINDICES slot / 3 is below it). \
                 That it is a joint count is consistent with the data and unproven: the two \
                 exceptions exceed it, and nothing inside the record says what the counts count.",
                EvidenceLevel::Inferred,
                "docs/ANIMATION.md",
            ),
            claim_of(
                ANIM_DATA_BONE_IDENTITIES,
                "The trailer's anim data carries no bone identity. Over the whole \
                 Spore_Content gmdl corpus, every 32-bit word of the trailer was histogrammed \
                 against the 2002-token morph-parameter-name universe recovered in \
                 docs/analysis/creator/11-bone-identity-vocabulary.md: 8889410 words at four \
                 alignments over the 3590 records whose trailer is located, and 2346955 more \
                 over the 619 whose material info this crate cannot size. Zero of the 170 \
                 witnessed names occur, and none of hip, body, hipL, hipR, knee, kneeL, kneeR, \
                 eye, iris, jaw, shoulder, head, wing, wingL, wingR, joint1. Three distinct \
                 tokens of the universe do occur (58 words), every one of which is the instance \
                 word of a resource key: two are rw4 records at group 0x41606100 and one is the \
                 gmdl record's own instance id in its own trailing key. What the anim data does \
                 carry is a pointer - it names rw4 group 0x41606100, whose 591 records hold 1182 \
                 Skeleton (0x70002) sections over 623 identity tokens, 88.4 percent shared \
                 with the part layer and carrying 96 already-named bones including the full \
                 joint1..joint9 family, root, eye, hip, knee, jaw, shoulder and body. The whole \
                 body anatomy is reached through that pointer, not stored in the gmdl.",
                EvidenceLevel::Observed,
                "docs/analysis/creator/12-gmdl-anim-data-vocabulary.md",
            ),
            claim_of(
                VERTEX_ELEMENT_TYPE_CODE,
                "The 4-byte type_code of a vertex element is a constant per D3DDECLUSAGE across the \
                 whole corpus: POSITION 0, TEXCOORD 2, NORMAL 6, BLENDINDICES 14, BLENDWEIGHT 15, \
                 TANGENT 19, over 5945 descriptors. It is carried verbatim and never used to \
                 interpret the element.",
                EvidenceLevel::Observed,
                "docs/ANIMATION.md",
            ),
            claim_of(
                WEIGHTLESS_BLEND_INDICES,
                "829 of the 2159 descriptors that declare BLENDINDICES declare no BLENDWEIGHT \
                 element, across 812 records; no descriptor declares a weight without indices. \
                 Whether those meshes are rigidly bound, use implicit equal weights, or read weights \
                 from a stream this walk does not reach is unknown, and no weights are synthesised \
                 for them.",
                EvidenceLevel::Observed,
                "docs/ANIMATION.md",
            ),
            claim_of(
                JOINT_TRANSFORM_LOCATION,
                "A gmdl record carries no joint transform. The record names joints, through the \
                 vertex stream's BLENDINDICES slots and the trailer's bone ranges; the matrices that \
                 move them are in the rw4 containers the refCount table names, in sections whose \
                 payloads this crate does not read. Until that is decoded a skeleton can be counted \
                 but not posed, and no transform here is invented to fill the gap.",
                EvidenceLevel::Supported,
                "docs/ANIMATION.md",
            ),
            claim_of(
                RW4_SECTION_CODE_NAMES,
                "The referenced containers' section type codes are read directly and are OBSERVED, \
                 but their names are not established here: the repository holds two tables that \
                 disagree about them (spore_rw4::cpp_type_name calls 0x70001 KeyframeAnim and \
                 0x70002 Skeleton; spore_rw4::python_type_name calls the same codes SkinsInK and has \
                 no entry for 0x70002). Both are reproduced; neither is promoted.",
                EvidenceLevel::Inferred,
                "spore-rw4/src/type_codes.rs",
            ),
        ]
    })
}

/// All recorded claims, in declaration order.
pub fn all_claims() -> &'static [GmdlClaim] {
    claims()
}

/// One claim's envelope, or `None` when this build records no claim for
/// `subject`. Absence is not `UNKNOWN`: it means "this build does not say",
/// which is a different statement from "this build says it is unknown".
pub fn claim(subject: &str) -> Option<Fact<&'static str>> {
    claims()
        .iter()
        .find(|entry| entry.subject == subject)
        .map(|entry| entry.fact.clone())
}

/// The grade attached to the `NORMAL`/`UBYTE4` reproduction, which is the one
/// claim a consumer of [`crate::Mesh::normals`] is most likely to need.
pub fn vertex_normal_encoding_level() -> EvidenceLevel {
    claim(VERTEX_NORMAL_ENCODING).map_or(EvidenceLevel::Unknown, |fact| fact.level())
}

/// Builds one claim envelope. The provenance is mandatory: an unsourced claim is
/// exactly the failure this module exists to prevent.
fn claim_of(
    subject: &'static str,
    statement: &'static str,
    level: EvidenceLevel,
    basis: &'static str,
) -> GmdlClaim {
    GmdlClaim {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_claim_carries_a_grade_and_provenance() {
        for entry in all_claims() {
            assert!(!entry.subject.is_empty());
            assert!(
                entry.statement.len() > 20,
                "{}: statement too thin to be evidence",
                entry.subject
            );
            assert!(entry.fact.is_available());
            assert!(
                !entry.fact.provenance().is_empty(),
                "{}: unsourced claim",
                entry.subject
            );
            assert_ne!(
                entry.fact.level(),
                EvidenceLevel::Unknown,
                "{}: an unknown claim is not a claim",
                entry.subject
            );
            assert_eq!(entry.fact.evidence_state(), EvidenceState::Derived);
            assert_eq!(entry.fact.value(), Some(&entry.statement));
        }
    }

    #[test]
    fn subjects_are_unique_and_resolvable() {
        let mut subjects: Vec<&str> = all_claims().iter().map(|entry| entry.subject).collect();
        subjects.sort_unstable();
        let count = subjects.len();
        subjects.dedup();
        assert_eq!(subjects.len(), count, "two claims share a subject");
        for subject in subjects {
            assert!(claim(subject).is_some(), "{subject} must be resolvable");
        }
    }

    #[test]
    fn an_unknown_subject_is_absent_rather_than_graded() {
        assert_eq!(claim("no_such_claim"), None);
    }

    #[test]
    fn the_normal_encoding_is_graded_inferred() {
        // The one claim this crate most needs to be honest about: the bytes are
        // reproduced, not decoded.
        let fact = claim(VERTEX_NORMAL_ENCODING).expect("recorded");
        assert_eq!(fact.level(), EvidenceLevel::Inferred);
        assert_eq!(vertex_normal_encoding_level(), EvidenceLevel::Inferred);
        assert!(fact.value().is_some_and(|text| text.contains("not")));
    }

    #[test]
    fn the_trailer_is_graded_no_stronger_than_supported() {
        // The framing is validated on one family of records. Promoting it to
        // Confirmed would claim more than the evidence supports, which is the
        // failure this scale exists to catch.
        let fact = claim(TRAILER_FRAMING).expect("recorded");
        assert!(
            fact.level() <= EvidenceLevel::Supported,
            "got {}",
            fact.level()
        );
    }
}
