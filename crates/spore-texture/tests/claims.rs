//! The evidence surface a consumer of this crate can query.
//!
//! The point of these tests is not the wording of a claim but the *shape*: an
//! unresolved field must come back as a non-finding graded `UNKNOWN`, never as a
//! zero or a default, and an established claim must carry provenance.
//!
//! # What changed when the framing was re-measured
//!
//! This file used to assert that three envelope words were unresolved
//! non-findings, that a DXT5 record carried a 16-byte layer header whose contents
//! nobody knew, and that "all layer headers precede all payloads" was an
//! observation. **All four assertions were false against the corpus** and are now
//! inverted rather than deleted: `field_18` is the base level's byte count in
//! every format, there are no layer headers, and there is no interleave to
//! explain because there is one chain. `field_10` is the only envelope word
//! still unresolved.
//!
//! A claim that contradicts the decoder is worse than no claim, so the two
//! codecs are pinned *below* `Verified` here: the framing is a byte count and the
//! codec is a published reading, and this file fails the build if the codec is
//! ever quietly promoted.

use spore_core::evidence::{EvidenceLevel, EvidenceState, FactState};
use spore_texture::claims::{
    all_claims, claim, grade_of, BC1_CODEC, BC3_ALPHA_SIX_LEVEL_CLAMP, BC3_CODEC,
    BC3_COLOUR_SIX_LEVEL_ENTRIES, BC4_RAMP_ROUNDING, DXT5_SPEC_DEVIATIONS,
    ENVELOPE_FIELD_10_MEANING, ENVELOPE_FIELD_18_MEANING, ENVELOPE_FIELD_1C_MEANING,
    FIRST_PREFIXED_LEVEL, LAYER_COUNT, LAYER_HEADER_CONTENTS, LAYER_INTERLEAVE,
    LUMINANCE_CHANNEL_SWIZZLE, LUMINANCE_FOURCC, LUMINANCE_FOURTH_LANE, LUMINANCE_FRAMING,
    LUMINANCE_TEXEL_LAYOUT, MIPS_BEYOND_32,
};

/// The only envelope word still unresolved is a non-finding.
///
/// The list is a one-element array on purpose. It used to hold three words; two
/// of them were false against the corpus and were moved to
/// [`the_two_envelope_words_the_corpus_resolved_are_no_longer_non_findings`].
/// Writing the remaining subject inline rather than looping is what makes it
/// obvious when a second word is added here by accident rather than by decision.
#[test]
fn the_one_unresolved_envelope_word_is_an_unknown_non_finding() {
    assert_the_non_finding(ENVELOPE_FIELD_10_MEANING);
}

/// Asserts the full shape of a non-finding: unavailable state, UNKNOWN grade,
/// missing evidence state, no value, no provenance, and the observation still
/// stated.
fn assert_the_non_finding(subject: &str) {
    let entry = claim(subject).expect("recorded");
    assert_eq!(entry.meaning.state(), FactState::Unavailable, "{subject}");
    assert_eq!(entry.meaning.level(), EvidenceLevel::Unknown, "{subject}");
    assert_eq!(
        entry.meaning.evidence_state(),
        EvidenceState::Missing,
        "{subject}"
    );
    assert_eq!(
        entry.meaning.value(),
        None,
        "{subject}: unavailable is not a value"
    );
    assert!(
        entry.meaning.provenance().is_empty(),
        "{subject}: a non-finding has no provenance"
    );
    assert!(
        !entry.observation.is_empty(),
        "{subject}: the observation is still stated"
    );
    assert!(
        entry.observation.len() > 40,
        "{subject}: an observation too thin to be evidence"
    );
}

#[test]
fn the_two_envelope_words_the_corpus_resolved_are_no_longer_non_findings() {
    // `field_18` is the base level's byte count in every format; `field_1c` is
    // the first texel of the 0x15 family's base level. Both were non-findings
    // until the framing identity closed, and both now carry a grade and
    // provenance. If either ever regresses to `Unavailable` the crate would be
    // under-claiming, which is a quieter failure than over-claiming, so it is
    // pinned explicitly.
    for subject in [ENVELOPE_FIELD_18_MEANING, ENVELOPE_FIELD_1C_MEANING] {
        let entry = claim(subject).expect("recorded");
        assert_eq!(entry.meaning.state(), FactState::Available, "{subject}");
        assert_ne!(entry.meaning.level(), EvidenceLevel::Unknown, "{subject}");
        assert!(
            !entry.meaning.provenance().is_empty(),
            "{subject}: an established claim needs a source"
        );
    }
    assert_eq!(
        grade_of(ENVELOPE_FIELD_18_MEANING),
        EvidenceLevel::Verified,
        "the base level's byte count in 4386/4386 records is an exact identity, not a sample"
    );
    assert_eq!(
        grade_of(ENVELOPE_FIELD_1C_MEANING),
        EvidenceLevel::Verified,
        "the framing identity only closes when those four bytes are charged to the base level"
    );
}

#[test]
fn the_refuted_framings_are_recorded_as_refuted_rather_than_deleted() {
    // There are no layer headers and no interleave. Both subjects survive, with
    // the measurement that refutes them attached, because a deleted refutation
    // leaves the next reader with nothing to contradict.
    for subject in [LAYER_HEADER_CONTENTS, LAYER_INTERLEAVE] {
        let entry = claim(subject).expect("recorded");
        assert_eq!(
            entry.meaning.state(),
            FactState::Available,
            "{subject}: the refutation itself is established"
        );
        assert!(
            entry.observation.contains("REFUTED"),
            "{subject}: the word REFUTED must be in the claim, not only in a commit message"
        );
    }
    assert_eq!(grade_of(LAYER_COUNT), EvidenceLevel::Verified);
    assert!(
        claim(LAYER_COUNT)
            .expect("recorded")
            .observation
            .contains("ONE chain"),
        "the layer count's observation must state what the records are"
    );
}

#[test]
fn the_old_eight_byte_block_is_recorded_as_refuted() {
    // `dxt5_spec_deviations` used to describe this crate's codec. It now records
    // the 8-byte reading the corpus refutes, and the four departures are named
    // so a reader of the claim alone learns what used to be believed.
    let entry = claim(DXT5_SPEC_DEVIATIONS).expect("recorded");
    assert_eq!(grade_of(DXT5_SPEC_DEVIATIONS), EvidenceLevel::Verified);
    assert!(
        entry.observation.contains("REFUTED"),
        "the claim must say it was refuted"
    );
    for needle in ["inverted", "R5G5B5", "alpha0 == alpha1", "6/2"] {
        assert!(
            entry.observation.contains(needle)
                || entry.meaning.value().is_some_and(|v| v.contains(needle)),
            "the refutation record does not mention {needle:?}"
        );
    }
    // And the no-colour consequence must be named as the artefact it was, so the
    // claim cannot be re-derived from a reader's memory of it.
    assert!(
        entry.observation.contains("no colour"),
        "the 'carries no colour' claim was an artefact of the 8-byte reading and must say so"
    );
}

#[test]
fn a_codec_is_never_graded_above_inferred() {
    // The framing is a byte count. The codec inside a 16-byte block is the
    // published reading of a record that declares 'DXT5', which is an inference
    // until a real decode is compared against the game's own output. Upgrading
    // either because the decoder needs it to be right is exactly the failure the
    // grades exist to prevent.
    assert_eq!(grade_of(BC3_CODEC), EvidenceLevel::Inferred);
    assert_eq!(grade_of(BC1_CODEC), EvidenceLevel::Inferred);
}

#[test]
fn the_open_questions_are_recorded_as_non_findings() {
    // Three sub-choices inside the BC3 codec and one framing question are
    // unresolved: the corpus does not decide them and a real decode would.
    for subject in [
        BC3_ALPHA_SIX_LEVEL_CLAMP,
        BC3_COLOUR_SIX_LEVEL_ENTRIES,
        BC4_RAMP_ROUNDING,
        FIRST_PREFIXED_LEVEL,
        LUMINANCE_CHANNEL_SWIZZLE,
    ] {
        assert_the_non_finding(subject);
    }
}

#[test]
fn the_reproduced_reference_rules_are_not_over_graded() {
    // Approximation, not Observed: no record has more than 10 levels, so the
    // beyond-32 rule is a bound reproduced from the previous reference, not an
    // observation of Spore's behaviour.
    assert_eq!(grade_of(MIPS_BEYOND_32), EvidenceLevel::Approximation);
    assert_eq!(grade_of(LAYER_INTERLEAVE), EvidenceLevel::Verified);
    assert_eq!(grade_of(LAYER_HEADER_CONTENTS), EvidenceLevel::Verified);
}

#[test]
fn the_luminance_family_is_established_and_graded_apart() {
    // Three claims are established by an exact identity over the whole corpus.
    for subject in [LUMINANCE_FOURCC, LUMINANCE_FRAMING, LUMINANCE_TEXEL_LAYOUT] {
        assert_eq!(
            grade_of(subject),
            EvidenceLevel::Verified,
            "{subject}: the framing and the texel order are exact-consume facts"
        );
        assert!(claim(subject).expect("recorded").meaning.is_available());
    }
    // One is an inference and is graded as one. "A mask" is a reading of four
    // bytes nobody has a name for; only its independence from the normal map was
    // measured. Upgrading this to Verified because the decoder needs a word for
    // it is precisely the failure the grades exist to prevent.
    assert_eq!(
        grade_of(LUMINANCE_FOURTH_LANE),
        EvidenceLevel::Inferred,
        "the fourth lane's independence is observed; its meaning is inferred"
    );
}

#[test]
fn an_unrecorded_subject_is_absence_rather_than_a_guess() {
    assert!(claim("no_such_subject").is_none());
    assert_eq!(grade_of("no_such_subject"), EvidenceLevel::Unknown);
    assert!(!all_claims().is_empty());
}
