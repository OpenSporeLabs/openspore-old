//! Every claim this crate makes that is **not** verified, with its grade.
//!
//! # Why this module exists
//!
//! A decoder that hands back eight `u32`s has already lost the only interesting
//! question about three of them. [`crate::RasterEnvelope`] carries `field_10`,
//! `field_18` and `field_1c`; nothing in the type says that nobody knows what
//! those words are. That is exactly the failure this repository exists to
//! prevent: an observation and an interpretation becoming indistinguishable
//! after they leave the decoder (see `spore_core::evidence`).
//!
//! So each such statement is recorded here **with its grade**, and the grades
//! for the unresolved envelope words are [`EvidenceLevel::Unknown`] — not
//! `Inferred`, not `Supported`, and not a plausible guess dressed up as
//! `Observed`. A word whose value has been seen in every sample still has an
//! unknown *meaning*, and the value being constant is not evidence about what
//! it is.
//!
//! # The two vocabularies, kept apart
//!
//! * The **observation** ("`field_10` reads 8 in every sampled record; no
//!   decoder reads it") is a fact about bytes and is plain text.
//! * The **meaning** ("`field_10` is a format revision / a bit depth / a
//!   padding hole") is the claim, and it is carried as a [`Fact`]. For an
//!   unresolved field the fact is **unavailable**, which in this repository's
//!   vocabulary means "we looked and found nothing" — never a zero, never a
//!   default, never a silent `false`.
//!
//! # What is *not* here
//!
//! Anything this crate reproduces from two independent implementations
//! (`tools/spore/dxt5/dxt5.py` and `src/assets/Dxt5.cpp`), checked
//! byte-for-byte against the hand-computed golden values in
//! `tests/test_textures.py::TestDxt5Synthetic`. The block layout, the alpha
//! ramp and its half-to-even rounding, the inverted palette test and the R5G5B5
//! channel read are not claims in this module: they are pinned by tests. What
//! *is* recorded is that they depart from the published BC3 specification —
//! as [`DXT5_SPEC_DEVIATIONS`], because "this is not what the spec says" is
//! established. "The spec is what Spore should have written" is not a question
//! this crate asks.
//!
//! # What changed when the `0x15` family stopped being refused
//!
//! [`LUMINANCE_FOURCC`] used to be a non-finding: the family was named, refused
//! by name, and called "luminance" on the strength of a Python oracle's comment
//! and one line of a design document. Four claims now carry what the corpus
//! actually shows — the family is not luminance ([`LUMINANCE_FOURCC`]), it
//! frames itself differently ([`LUMINANCE_FRAMING`]), its texels are
//! `Z, X, Y, lane4` ([`LUMINANCE_TEXEL_LAYOUT`]), and `lane4` is independent of
//! the normal map ([`LUMINANCE_FOURTH_LANE`]).
//!
//! The old subject was **updated, not deleted**, and two claims that had been
//! non-findings are now established: [`ENVELOPE_FIELD_18_MEANING`] is the base
//! level's byte count in this family, and [`ENVELOPE_FIELD_1C_MEANING`] is not
//! a field of this family at all. One claim stayed unresolved on purpose —
//! [`LUMINANCE_CHANNEL_SWIZZLE`] — because "should the decoder re-index the
//! channels" is a question about a shader this workspace has never read, and
//! answering it conveniently would be exactly the failure this module exists to
//! prevent.
//!
//! Two research documents are now **refuted by the bytes**, which is recorded
//! rather than quietly dropped: `docs/MATERIALS-DESIGN.md` §1 says `field_1c`
//! holds "a float 1.0 in 0x15 records" (it is the first texel), and calls the
//! family luminance (it is a normal map). `docs/LUMINANCE-RASTER.md` carries the
//! arithmetic.

use std::sync::OnceLock;

use spore_core::evidence::{EvidenceLevel, EvidenceState, Fact, Provenance};

/// What the envelope word at `0x10` means.
pub const ENVELOPE_FIELD_10_MEANING: &str = "envelope_field_10_meaning";

/// What the envelope word at `0x18` means.
pub const ENVELOPE_FIELD_18_MEANING: &str = "envelope_field_18_meaning";

/// What the envelope word at `0x1c` means.
pub const ENVELOPE_FIELD_1C_MEANING: &str = "envelope_field_1c_meaning";

/// What the envelope word at `0x00` means beyond "format revision".
pub const ENVELOPE_VERSION_MEANING: &str = "envelope_version_meaning";

/// What the 16 opaque bytes of a DXT5 layer header hold.
pub const LAYER_HEADER_CONTENTS: &str = "layer_header_contents";

/// Why every DXT5 layer header precedes every layer payload.
pub const LAYER_INTERLEAVE: &str = "layer_header_payload_interleave";

/// What the `0x15` family actually is, and whether "luminance" describes it.
pub const LUMINANCE_FOURCC: &str = "luminance_fourcc_family";

/// How the `0x15` family frames its payload.
pub const LUMINANCE_FRAMING: &str = "luminance_0x15_framing";

/// What the four bytes of one `0x15` texel are, in file order.
pub const LUMINANCE_TEXEL_LAYOUT: &str = "luminance_0x15_texel_layout";

/// Why the `0x15` family's decoded bytes are emitted in file order and not swizzled.
pub const LUMINANCE_CHANNEL_SWIZZLE: &str = "luminance_0x15_channel_swizzle";

/// What the fourth `0x15` lane carries.
pub const LUMINANCE_FOURTH_LANE: &str = "luminance_0x15_fourth_lane";

/// What happens to mip levels beyond 32.
pub const MIPS_BEYOND_32: &str = "mip_chain_beyond_32";

/// How Spore's block decode departs from the published BC3/DXT5 specification.
pub const DXT5_SPEC_DEVIATIONS: &str = "dxt5_spec_deviations";

/// Why an address is a `Fact` and not a value.
///
/// `Unavailable` is the honest state for a field whose meaning nobody
/// established: the repository's rule is that it must not be represented as a
/// zero, an empty string or `false`. Each subject therefore carries either an
/// available meaning with a grade and provenance, or this unavailable envelope.
const NO_MEANING_ESTABLISHED: &str = "meaning_not_established";

/// One recorded claim about the raster format: what was observed, and what (if
/// anything) that observation is known to mean.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextureClaim {
    /// Stable identifier, e.g. `"envelope_field_10_meaning"`.
    pub subject: &'static str,
    /// What the bytes are observed to be, and what this crate does with them.
    /// This half is always present and is never an interpretation.
    pub observation: &'static str,
    /// The meaning, when one is established. `unavailable` means "we looked and
    /// found nothing", which is not the same as "it is zero".
    pub meaning: Fact<&'static str>,
}

/// The claims, in declaration order.
///
/// Lazily built because [`Fact`] cannot be constructed in a `const` context:
/// an available fact must carry a `Vec<Provenance>`.
pub fn all_claims() -> &'static [TextureClaim] {
    static CLAIMS: OnceLock<Vec<TextureClaim>> = OnceLock::new();
    CLAIMS.get_or_init(|| {
        vec![
            unresolved(
                ENVELOPE_FIELD_10_MEANING,
                "field_10 reads 8 in every sampled raster (the 512x512 records 0x40662900/01, the \
                 raster_ap_5552/raster_0 family, and the 0x15 record raster_6948). No decoder \
                 reads it: not the Python oracle, not src/assets/Texture.cpp, not this crate. \
                 docs/MATERIALS-DESIGN.md section 1 records it as '=8 every sample; decoder must \
                 NOT rely on it'.",
            ),
            resolved(
                ENVELOPE_FIELD_18_MEANING,
                "field_18 is format-specific. In every one of the 901 0x15 records of \
                 Spore_Content.package it is exactly width*height*4: 0x1000 for the 32x32/6-mip \
                 shape, 0x40000 for 256x256/9, 0x100000 for 512x512/10. In the DXT5 records it \
                 reads 0x00040000 whatever the dimensions (512x512 DXT5 carries 0x40000, not the \
                 0x100000 its w*h*4 would give), so the word does not have one meaning across \
                 formats. No decoder reads it: the walk derives the same quantity from the \
                 envelope's own geometry, which is this crate's existing 'derived, never read' \
                 rule applied to one more word.",
                EvidenceLevel::Observed,
                "In the 0x15 family, the base level's payload length in bytes. It is redundant \
                 with the derived chain and is deliberately not consulted, so that a record whose \
                 header disagrees with its geometry is caught by the framing identity rather than \
                 by trusting a word whose cross-format meaning is not shared.",
                "Spore_Content.package, all 901 records of type 0x2F4E681C with fourcc 0x15",
            ),
            resolved(
                ENVELOPE_FIELD_1C_MEANING,
                "field_1c reads 0x0000FFFF in DXT5 records. In a 0x15 record it is NOT a header \
                 field: that family's header is 28 bytes, and the word at 0x1c is the first four \
                 bytes of the base-level payload. Measured: 670 of the 901 0x15 records have \
                 field_1c = 0x008080ff and the rest are 231 further distinct values tracking the \
                 varying pixel content (e.g. 0x098080ff), so it is a texel and not a constant. \
                 This REFUTES docs/MATERIALS-DESIGN.md section 1, which records it as 'a float \
                 1.0 in 0x15 records'.",
                EvidenceLevel::Verified,
                "Payload, not metadata. The word docs/MATERIALS-DESIGN.md section 1 describes as \
                 'a float 1.0 in 0x15 records' is refuted by the bytes: 0x008080ff is not 1.0f \
                 and it varies per record, and the framing identity only closes when those four \
                 bytes are charged to the base level. RasterEnvelope still reports the word because \
                 a caller reading an envelope should see the bytes that are there.",
                "Spore_Content.package; refutes docs/MATERIALS-DESIGN.md section 1",
            ),
            resolved(
                ENVELOPE_VERSION_MEANING,
                "version reads 1 in every sampled raster. No record with any other value has been \
                 seen, so the crate does not validate it: refusing an unobserved version would be \
                 a guess about a format nobody has.",
                EvidenceLevel::Observed,
                "A format revision counter, always 1 in the records sampled so far. Never \
                 validated, because an unobserved value is not a known-bad one.",
                "docs/MATERIALS-DESIGN.md section 1",
            ),
            unresolved(
                LAYER_HEADER_CONTENTS,
                "Each DXT5 layer is preceded by 16 bytes read as a 4-byte name plus three u32 (one \
                 sample spells the name 'e9D1'). The three words are skipped, never interpreted, \
                 and no layer's presence depends on them. The 0x15 family has NO layer header at \
                 all, which is one of the two structural ways it differs from DXT5 -- so this \
                 claim is about one of the two formats, not about 'a raster'.",
            ),
            resolved(
                LAYER_INTERLEAVE,
                "All n layer headers precede all payloads: payloadOff = 32 + n*16 = 0x40 on every \
                 real 512x512 DXT5 record, and the block at that offset is a valid DXT5 block \
                 (20 00 02 00 65 39 00 00). The 0x15 family has no layer headers, so the claim is \
                 specific to DXT5's framing.",
                EvidenceLevel::Observed,
                "Headers are grouped at the front of the payload region rather than interleaved \
                 with their payloads. This is what makes layer i start at 32 + n*16 + i*chain.",
                "docs/MATERIALS-DESIGN.md section 1",
            ),
            resolved(
                LUMINANCE_FOURCC,
                "Records whose fourcc is 0x00000015 (observed: 901 of the 2954 raster records in \
                 Spore_Content.package, in exactly three shapes -- 32x32/6 mips/5508 bytes, \
                 256x256/9/349584, 512x512/10/1398164) are NOT luminance. Each texel is four \
                 bytes; lane 0 has mean > 230 in 901 of 901 records, lanes 1 and 2 are centred on \
                 127.5 in 899 of 901, and 7 records carry the constant (255,128,128) in lanes \
                 0-2. This crate decodes all 901. The old 'luminance, out of scope' reading was \
                 a name with no decoder behind it and the bytes REFUTE it, as they refute \
                 docs/MATERIALS-DESIGN.md section 1 which calls this family luminance and puts it \
                 out of scope. The 'exactly three shapes' count is scoped to that one package: the \
                 framing identity is size-driven, so a fourth shape would decode rather than \
                 fail, and none has been seen in any corpus measured here.",
                EvidenceLevel::Verified,
                "A tangent-space normal map plus a fourth lane: lanes 0, 1, 2 are Z, X, Y with 128 \
                 as zero, which is what the flat-normal records spell out literally. 'Luminance' \
                 survives only as this crate's and the repository's existing identifier \
                 (luminance_fourcc_family), kept so the vocabulary does not fork.",
                "Spore_Content.package; docs/LUMINANCE-RASTER.md",
            ),
            resolved(
                LUMINANCE_FRAMING,
                "A 0x15 record is 28 bytes of header followed by the base level unprefixed, then \
                 for each further mip a little-endian u32 byte count and that many payload bytes. \
                 The identity record_size == 28 + w*h*4 + sum over mips 1..n of (4 + \
                 max(1,w>>m)*max(1,h>>m)*4) holds with nothing left over in 901 of 901 records, \
                 and each recorded prefix equals its own mip's w*h*4.",
                EvidenceLevel::Verified,
                "An exact-consume identity over the whole corpus, not a plausibility fit. It is \
                 strictly stronger than the DXT5 rule, which divides a payload to find a layer \
                 count; here the record size is fully determined, so a mismatch is a refusal \
                 (LuminanceRecordSize / LuminanceLengthPrefix) rather than a rounding.",
                "Spore_Content.package; docs/LUMINANCE-RASTER.md",
            ),
            resolved(
                LUMINANCE_TEXEL_LAYOUT,
                "One 0x15 texel is four bytes in file order: Z, X, Y, fourth. Evidence: the flat \
                 records hold the constant (0xFF, 0x80, 0x80, mask); lane 0 tracks the derived \
                 Z = sqrt(1-nx^2-ny^2) with correlation 0.954 and an exact byte match on the \
                 median 74% of texels (>=95% on 27 records); lanes 1 and 2 are mutually \
                 uncorrelated (0.004), which two independent normal components are and two \
                 derived channels are not.",
                EvidenceLevel::Verified,
                "The three first lanes are one tangent-space normal map encoded 0..255 with 128 as \
                 zero, and the Z component comes FIRST on disk. Which of R/G/B a consumer should \
                 read that as is NOT part of this claim.",
                "Spore_Content.package; docs/LUMINANCE-RASTER.md",
            ),
            unresolved(
                LUMINANCE_CHANNEL_SWIZZLE,
                "decode_luminance_mip emits the four stored bytes as R, G, B, A in file order, \
                 with no swizzle, even though the bytes show the Z component first (lane 0). \
                 A consumer that wants the conventional normal-map order R=X, G=Y, B=Z must \
                 re-index, and nothing in this repository establishes that any renderer does so. \
                 So what is NOT swizzled is observed; whether it SHOULD be is unresolved.",
            ),
            resolved(
                LUMINANCE_FOURTH_LANE,
                "The fourth byte of a 0x15 texel is independent of the normal map: correlation \
                 with lane 0 is 0.032, its mean varies per record (median 169.7, range 0.2 to \
                 229.8) and 540 of 901 records give it only two distinct values, 0 and 255.",
                EvidenceLevel::Inferred,
                "A per-texel mask of some kind -- binary in most records, graded in others. Which \
                 mask (ambient occlusion, a specular term, a paint channel) is not established; \
                 no claim is made beyond 'independent of the normal map and carried as alpha'.",
                "Spore_Content.package",
            ),
            resolved(
                MIPS_BEYOND_32,
                "dxt5_chain_size charges 8 bytes per mip past 32 because `w >> mip` is undefined \
                 for mip >= 32. The largest chain observed is 10 mips (512x512, mipCount=10), so \
                 no record exercises the rule.",
                EvidenceLevel::Approximation,
                "One 4x4 block per mip beyond 32. Reproduced from the reference so the two agree; \
                 it bounds an absurd header rather than describing how Spore writes a 33rd mip.",
                "src/assets/Dxt5.cpp",
            ),
            resolved(
                DXT5_SPEC_DEVIATIONS,
                "The block decoder departs from the published BC3/DXT5 specification in four \
                 places, all reproduced from tools/spore/dxt5/dxt5.py and src/assets/Dxt5.cpp and \
                 all pinned by tests/dxt5_golden.rs: (1) the palette-size test is inverted - \
                 c0 > c1 yields TWO colours, not four; (2) channels are read R5G5B5, not R6G6B5, \
                 so a 6-bit input yields red in {0,8} only; (3) codes 0 and 3 return a 6/2 blend \
                 of the endpoints rather than the endpoints themselves; (4) there is no \
                 alpha0 == alpha1 collapse.",
                EvidenceLevel::Verified,
                "Spore's decoder, not the BC3 specification, defines these blocks. A change here \
                 changes decoded pixels and must be justified by a record, never by the spec.",
                "tools/spore/dxt5/dxt5.py",
            ),
        ]
    })
}

/// One claim's envelope, or `None` when this build records no claim for
/// `subject`. Absence is not `UNKNOWN`: it means "this build does not say",
/// which is a different statement from "this build says it is unknown".
pub fn claim(subject: &str) -> Option<&'static TextureClaim> {
    all_claims().iter().find(|entry| entry.subject == subject)
}

/// The grade of one subject's meaning, or [`EvidenceLevel::Unknown`] when this
/// build records nothing for it.
///
/// The grade is about the *meaning*, not about the observation: an unresolved
/// field is `UNKNOWN` even though its value has been seen in every sample.
pub fn grade_of(subject: &str) -> EvidenceLevel {
    claim(subject).map_or(EvidenceLevel::Unknown, |entry| entry.meaning.level())
}

/// Builds a claim whose meaning nobody established.
///
/// The observation stands on its own; the meaning is a non-finding, and
/// `EvidenceLevel::Unknown` is what that costs.
fn unresolved(subject: &'static str, observation: &'static str) -> TextureClaim {
    TextureClaim {
        subject,
        observation,
        meaning: Fact::unavailable(NO_MEANING_ESTABLISHED),
    }
}

/// Builds a claim whose meaning is established, with a mandatory grade and
/// provenance: an unsourced claim is the failure this module exists to prevent.
fn resolved(
    subject: &'static str,
    observation: &'static str,
    level: EvidenceLevel,
    meaning: &'static str,
    basis: &'static str,
) -> TextureClaim {
    TextureClaim {
        subject,
        observation,
        meaning: Fact::new(
            level,
            EvidenceState::Derived,
            vec![Provenance::derived(basis)],
            meaning,
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_unresolved_subject_is_unavailable_and_unknown() {
        // The two envelope words this crate still has nothing to say about, the
        // DXT5 layer header, and the channel-swizzle question. Each must be a
        // non-finding, never a zero and never a guess.
        for subject in [
            ENVELOPE_FIELD_10_MEANING,
            LAYER_HEADER_CONTENTS,
            LUMINANCE_CHANNEL_SWIZZLE,
        ] {
            let entry = claim(subject).unwrap_or_else(|| panic!("{subject} is not recorded"));
            assert!(
                entry.meaning.is_unavailable(),
                "{subject}: an unresolved meaning must be unavailable, not a value"
            );
            assert_eq!(entry.meaning.level(), EvidenceLevel::Unknown, "{subject}");
            assert_eq!(
                entry.meaning.evidence_state(),
                EvidenceState::Missing,
                "{subject}"
            );
            assert_eq!(entry.meaning.value(), None, "{subject}");
            assert!(
                entry.meaning.provenance().is_empty(),
                "{subject}: a non-finding has no provenance to repeat"
            );
            assert_eq!(grade_of(subject), EvidenceLevel::Unknown, "{subject}");
        }
    }

    #[test]
    fn an_established_claim_carries_its_grade_and_provenance() {
        for subject in [
            ENVELOPE_VERSION_MEANING,
            ENVELOPE_FIELD_18_MEANING,
            ENVELOPE_FIELD_1C_MEANING,
            LAYER_INTERLEAVE,
            LUMINANCE_FOURCC,
            LUMINANCE_FRAMING,
            LUMINANCE_TEXEL_LAYOUT,
            LUMINANCE_FOURTH_LANE,
            MIPS_BEYOND_32,
            DXT5_SPEC_DEVIATIONS,
        ] {
            let entry = claim(subject).unwrap_or_else(|| panic!("{subject} is not recorded"));
            assert!(entry.meaning.is_available(), "{subject}");
            assert_ne!(entry.meaning.level(), EvidenceLevel::Unknown, "{subject}");
            assert!(
                !entry.meaning.provenance().is_empty(),
                "{subject}: an unsourced claim is the failure this module exists to prevent"
            );
            assert_eq!(
                entry.meaning.reason(),
                None,
                "{subject}: an available fact has no reason"
            );
            assert_eq!(
                entry.meaning.value(),
                Some(entry.meaning.value().unwrap_or(&""))
            );
        }
    }

    #[test]
    fn observations_are_always_present_and_thin_claims_are_rejected() {
        for entry in all_claims() {
            assert!(!entry.subject.is_empty());
            assert!(
                entry.observation.len() > 40,
                "{}: observation too thin to be evidence",
                entry.subject
            );
            assert_eq!(
                entry.meaning.state(),
                if entry.meaning.is_available() {
                    spore_core::evidence::FactState::Available
                } else {
                    spore_core::evidence::FactState::Unavailable
                }
            );
        }
    }

    #[test]
    fn subjects_are_unique_and_an_unrecorded_subject_is_not_a_guess() {
        let claims = all_claims();
        for (index, entry) in claims.iter().enumerate() {
            for other in claims.iter().skip(index + 1) {
                assert_ne!(entry.subject, other.subject, "duplicate subject");
            }
        }
        assert_eq!(claim("no_such_subject"), None);
        assert_eq!(grade_of("no_such_subject"), EvidenceLevel::Unknown);
    }

    #[test]
    fn the_beyond_32_rule_is_an_approximation_not_an_observation() {
        // Grading the reproduced reference rule as Observed would be a lie: no
        // record has more than 10 mips.
        assert_eq!(grade_of(MIPS_BEYOND_32), EvidenceLevel::Approximation);
        assert_eq!(grade_of(DXT5_SPEC_DEVIATIONS), EvidenceLevel::Verified);
        assert_eq!(grade_of(LAYER_INTERLEAVE), EvidenceLevel::Observed);
    }

    #[test]
    fn the_grades_of_the_luminance_family_say_what_the_evidence_says() {
        // Each grade is pinned here so that a convenient upgrade fails the build.
        // In particular `LUMINANCE_FOURTH_LANE` is INFERRED, not VERIFIED: "a
        // mask" is a reading of four bytes nobody has a name for, and the
        // measurement only ever established independence from the normal map.
        assert_eq!(grade_of(LUMINANCE_FOURCC), EvidenceLevel::Verified);
        assert_eq!(grade_of(LUMINANCE_FRAMING), EvidenceLevel::Verified);
        assert_eq!(grade_of(LUMINANCE_TEXEL_LAYOUT), EvidenceLevel::Verified);
        assert_eq!(
            grade_of(LUMINANCE_FOURTH_LANE),
            EvidenceLevel::Inferred,
            "'a mask of some kind' is an inference and must not be graded higher"
        );
        assert_eq!(grade_of(LUMINANCE_CHANNEL_SWIZZLE), EvidenceLevel::Unknown);
    }

    #[test]
    fn the_refuted_documents_are_named_rather_than_dropped() {
        // The two claims that contradict docs/MATERIALS-DESIGN.md section 1 must
        // carry the refutation in their observation, so a reader of the claim
        // alone learns that the document and the corpus disagree.
        for subject in [ENVELOPE_FIELD_1C_MEANING, LUMINANCE_FOURCC] {
            let entry = claim(subject).unwrap();
            assert!(
                entry.observation.contains("MATERIALS-DESIGN")
                    || entry.observation.contains("float 1.0")
                    || entry.observation.contains("refuted"),
                "{subject}: the refutation must be visible in the claim itself"
            );
        }
    }
}
