//! The evidence surface a consumer of this crate can query.
//!
//! The point of these tests is not the wording of a claim but the *shape*: an
//! unresolved field must come back as a non-finding graded `UNKNOWN`, never as a
//! zero or a default, and an established claim must carry provenance.
//!
//! # What changed when `0x15` stopped being refused
//!
//! This file used to assert that three envelope words were unresolved
//! non-findings and that the `0x15` family was one too. Two of those assertions
//! were **false against the corpus** and are now inverted rather than deleted:
//! `field_18` is the base level's byte count in that family and `field_1c` is
//! not a field of it at all. `field_10` is the only envelope word still
//! unresolved, and the test is renamed to say so — a test named "the three
//! unresolved envelope words" that silently covered two words would have been
//! the same observation/interpretation collapse the module exists to prevent.

use spore_core::evidence::{EvidenceLevel, EvidenceState, FactState};
use spore_texture::claims::{
    all_claims, claim, grade_of, DXT5_SPEC_DEVIATIONS, ENVELOPE_FIELD_10_MEANING,
    ENVELOPE_FIELD_18_MEANING, ENVELOPE_FIELD_1C_MEANING, LAYER_HEADER_CONTENTS, LAYER_INTERLEAVE,
    LUMINANCE_CHANNEL_SWIZZLE, LUMINANCE_FOURCC, LUMINANCE_FOURTH_LANE, LUMINANCE_FRAMING,
    LUMINANCE_TEXEL_LAYOUT, MIPS_BEYOND_32,
};

/// The only envelope word still unresolved is a non-finding.
///
/// The list is a one-element array on purpose. It used to hold three words; two
/// of them were false against the corpus and were moved to
/// [`the_two_envelope_words_the_corpus_resolved_are_no_longer_non_findings`].
/// Writing the remaining subjects inline rather than looping is what makes it
/// obvious when a fourth word is added here by accident rather than by decision.
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
    // `field_18` is w*h*4 in every 0x15 record; `field_1c` is the first texel.
    // Both were non-findings until the framing identity closed, and both now
    // carry a grade and provenance. If either ever regresses to `Unavailable`
    // the crate would be under-claiming, which is a quieter failure than
    // over-claiming, so it is pinned explicitly.
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
        EvidenceLevel::Observed,
        "w*h*4 in 901/901 records is an observation about real bytes, not a derivation"
    );
    assert_eq!(
        grade_of(ENVELOPE_FIELD_1C_MEANING),
        EvidenceLevel::Verified,
        "the framing identity only closes when those four bytes are charged to the base level"
    );
}

#[test]
fn the_deliberate_deviations_are_recorded_as_verified_against_the_oracle() {
    assert_eq!(grade_of(DXT5_SPEC_DEVIATIONS), EvidenceLevel::Verified);
    let entry = claim(DXT5_SPEC_DEVIATIONS).expect("recorded");
    // The record must name all four departures, or a future reader inherits the
    // belief that this codec follows the published specification.
    let statement = entry.meaning.value().copied().unwrap_or_default();
    let text = format!("{} {statement}", entry.observation);
    for needle in ["inverted", "R5G5B5", "alpha0 == alpha1", "codes 0 and 3"] {
        assert!(
            text.contains(needle),
            "deviation record does not mention {needle:?}"
        );
    }
}

#[test]
fn the_reproduced_reference_rules_are_not_over_graded() {
    // Observed: read off real records.
    assert_eq!(grade_of(LAYER_INTERLEAVE), EvidenceLevel::Observed);
    // Approximation, not Observed: no record has more than 10 mips, so the
    // beyond-32 rule is a bound reproduced from the reference, not an
    // observation of Spore's behaviour.
    assert_eq!(grade_of(MIPS_BEYOND_32), EvidenceLevel::Approximation);
    // Unresolved: the DXT5 layer header's three words, and whether the 0x15
    // decoder should re-index the channels it now decodes.
    assert_eq!(grade_of(LAYER_HEADER_CONTENTS), EvidenceLevel::Unknown);
    assert_eq!(grade_of(LUMINANCE_CHANNEL_SWIZZLE), EvidenceLevel::Unknown);
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
