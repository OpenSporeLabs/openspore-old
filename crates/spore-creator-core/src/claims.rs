//! What this crate does **not** know.
//!
//! Every other format crate in this workspace carries a `claims` module, and the
//! rule is unchanged by the migration: extend this when something is learned,
//! never delete a claim because it is inconvenient. A reader who finds an
//! absence documented here can trust it; a reader who finds one undocumented has
//! to go looking, which is the expensive case.
//!
//! The counts behind each claim are measured over all 68 creator documents in
//! `Spore_Game.package` (680 `<blockref>` blocks) or all 1549 rigblock records,
//! as cited. Sources: `docs/CREATOR-CORE.md` and
//! `docs/analysis/creator/01` … `08`.
//!
//! ## The ones that shape the API
//!
//! Three unknowns are not incidental — they are why the types look the way they
//! do, and a future finding must not be allowed to quietly invalidate them:
//!
//! 1. **Local versus absolute frame** (`blockid → world position`). The corpus
//!    stores one `position` per block and no frame marker. `07-editor-data.json`
//!    G-04 leaves it open. We store the value as written and compose at use.
//! 2. **What `paintregion` and `paintid` mean.** 23 and 6 distinct values
//!    respectively, and the `plt` palette record type has **0 records in all 14
//!    packages** scanned, so there is nothing to join against. Both stay opaque
//!    words. We do not call them colours or regions in the API beyond the
//!    element they came from.
//! 3. **What the `<weight channel>` words name.** 14 distinct values; no
//!    registry, no enum, no decoder. `ChannelWeight::channel` is a `u32` and
//!    stays one.
//!
//! ## Claims that constrain the authoring operations
//!
//! Three more are recorded for the same reason — not because they are
//! incidental, but because they are what stops [`crate::edit`] from doing the
//! obvious thing and being wrong:
//!
//! 4. **`reparent_transform_consequence`.** A reparent moves the edge and
//!    rewrites no transform, because `transform_frame` above is open. If the
//!    game composes a block's `<position>` from its parent, then reparenting
//!    moves the part in world space and the correct implementation must
//!    re-express the child's position; we do not know which it is.
//! 5. **`replace_symmetric_handedness`.** Nothing in the corpus shows the same
//!    placement holding two different `<blockid>` values, so we do not know
//!    whether swapping a part on one half of a mirrored pair must break the
//!    mirror relation. `replace_block` leaves it alone and lets `validate`
//!    report anything odd.
//! 6. **`paintregion_meaning` / `paintid_meaning`, again.** `set_paint` stores
//!    what it is given and validates neither the values nor their order,
//!    because there is no registry to validate them against.

/// A subject this crate has looked for and not found.
///
/// Kept as data rather than prose so a test can assert the list has not quietly
/// shrunk, and so a future session can strike one off with a citation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Claim {
    /// A closed-vocabulary subject code.
    pub subject: &'static str,
    /// The evidence grade: every entry here is [`crate::document::DOCUMENT_EVIDENCE`]
    /// or lower, because "we looked" is itself the claim.
    pub level: spore_core::EvidenceLevel,
    /// What was searched, so the negative is credible.
    pub searched: &'static str,
    /// What would settle it.
    pub would_settle: &'static str,
}

/// Every open question, in one list.
pub const CLAIMS: &[Claim] = &[
    Claim {
        subject: "transform_frame",
        level: spore_core::EvidenceLevel::Unknown,
        searched: "all 680 blocks across 68 documents: one <position> per block, no frame marker element",
        would_settle: "one Wine trace of a known <blockid> showing whether the game composes or reads it absolutely",
    },
    Claim {
        subject: "reparent_transform_consequence",
        level: spore_core::EvidenceLevel::Unknown,
        searched: "no creator session observed and no two corpus documents differing only by a moved edge; 601 edges over 68 documents, none with a before/after pair",
        would_settle: "a Wine trace of one drag, or two saved documents of the same creature differing only in one parent link",
    },
    Claim {
        subject: "replace_symmetric_handedness",
        level: spore_core::EvidenceLevel::Unknown,
        searched: "84 reciprocal <symmetric> relations; no rigblock record declares handedness, and the corpus never shows the same placement with two different <blockid> values",
        would_settle: "decoding Editors::EditorRigblock's side information, or a creator session that swaps a part on one half of a mirrored pair",
    },
    Claim {
        subject: "paintregion_meaning",
        level: spore_core::EvidenceLevel::Unknown,
        searched: "195 <paint> entries, 23 distinct values; plt (0x011989B7) records: 0 in all 14 packages",
        would_settle: "a decoded palette record, or the shader that consumes the region index",
    },
    Claim {
        subject: "paintid_meaning",
        level: spore_core::EvidenceLevel::Unknown,
        searched: "6 distinct values corpus-wide (0xf5967f30 x175); Palettes::cSwatchManager named in the binary but no record backing it",
        would_settle: "the cSwatchManager loader, or a creator session that changes it and the record that follows",
    },
    Claim {
        subject: "weight_channel_meaning",
        level: spore_core::EvidenceLevel::Unknown,
        searched: "252 <weight channel=...> entries over 119 blocks, 14 distinct channel words; no registry in any package",
        would_settle: "the skinning shader's channel enumeration, or Editors::EditorRigblock's handle lists decoded",
    },
    Claim {
        subject: "limb_semantics",
        level: spore_core::EvidenceLevel::Unknown,
        searched: "limbtype 5, musclescale 5, basemusclescale 8 across crt and flr only",
        would_settle: "the creature editor's limb picker, or a differential over two creatures differing only in these",
    },
    Claim {
        subject: "user_orientation_role",
        level: spore_core::EvidenceLevel::Unknown,
        searched: "user_orientation present on 11 vcl blocks only; the binary stores mTotalOrientation as derived, not stored",
        would_settle: "whether the game reads this field at all, which a single trace would answer",
    },
    Claim {
        subject: "snapped_semantics",
        level: spore_core::EvidenceLevel::Unknown,
        searched: "680 of 680 blocks write <snapped>; value distribution not yet censused",
        would_settle: "census of the value distribution, then a creator session that snaps and does not",
    },
    Claim {
        subject: "triangle_direction_relationship",
        level: spore_core::EvidenceLevel::Unknown,
        searched: "680 blocks; triangledirection and orientation disagree in 102, so neither derives the other",
        would_settle: "the editor's normal-alignment code path",
    },
    Claim {
        subject: "attachment_points",
        level: spore_core::EvidenceLevel::Unknown,
        searched: "no socket, joint or attachment element in any of the 68 documents",
        would_settle: "decoding Editors::EditorRigblock's nine fixed_vector<ResourceKey,8> snap/align/interact lists, which exist in code only",
    },
    Claim {
        subject: "part_geometry_link",
        level: spore_core::EvidenceLevel::Supported,
        searched: "1549 rigblock records: the record is a quantised bounding subdivision, not a model reference; blockid -> prop (0x00B1B104) at the same key -> {0x2F4E681B, 0x40606000} resolves for 1316 of 1549",
        would_settle: "decoding 0x01C135DA (skinned mesh) and 0x2F4E681B, plus the 233 rigblocks with no counterpart and flora at 0/107",
    },
    Claim {
        subject: "blockid_type_word",
        level: spore_core::EvidenceLevel::Verified,
        searched: "all 60 distinct <blockid> pairs name records of exactly one type, 0x5CAA5E28, and 0 name a gmdl",
        would_settle: "settled: the type is implied, and inventing a ResourceKey for a block would be a claim",
    },
    Claim {
        subject: "skeleton_and_animation_binding",
        level: spore_core::EvidenceLevel::Unknown,
        searched: "no bone, joint or animation element in any creator document; gmdl BLENDINDICES exist but 0 of 582 creature meshes decode",
        would_settle: "a 0x01C135DA decoder plus the shader that consumes its bone block",
    },
];

/// Looks up a claim by subject code.
pub fn claim(subject: &str) -> Option<&'static Claim> {
    CLAIMS.iter().find(|claim| claim.subject == subject)
}

/// The grade for a subject, or [`spore_core::EvidenceLevel::Unknown`] when this
/// crate has no opinion. Unknown is the honest default here: "we did not look"
/// and "we looked and found nothing" are different, and neither is `Verified`.
pub fn level_of(subject: &str) -> spore_core::EvidenceLevel {
    claim(subject).map_or(spore_core::EvidenceLevel::Unknown, |claim| claim.level)
}

#[cfg(test)]
mod tests {
    use super::*;
    use spore_core::EvidenceLevel;

    #[test]
    fn every_claim_names_what_was_searched_and_what_would_settle_it() {
        for claim in CLAIMS {
            assert!(!claim.subject.is_empty());
            assert!(
                !claim.searched.is_empty(),
                "{} claims a negative with nothing to back it",
                claim.subject
            );
            assert!(
                !claim.would_settle.is_empty(),
                "{} says what is unknown but not how to find out",
                claim.subject
            );
        }
    }

    #[test]
    fn subject_codes_are_unique_so_a_lookup_cannot_be_ambiguous() {
        let mut subjects: Vec<&str> = CLAIMS.iter().map(|claim| claim.subject).collect();
        let before = subjects.len();
        subjects.sort_unstable();
        subjects.dedup();
        assert_eq!(subjects.len(), before);
    }

    #[test]
    fn a_subject_this_crate_never_looked_at_is_unknown_not_verified() {
        assert_eq!(level_of("no_such_subject"), EvidenceLevel::Unknown);
        assert!(claim("no_such_subject").is_none());
    }

    #[test]
    fn the_transform_frame_question_is_still_open_and_says_what_would_close_it() {
        let claim = claim("transform_frame").expect("the frame question is recorded");
        assert_eq!(claim.level, EvidenceLevel::Unknown);
        assert!(claim.would_settle.contains("trace"));
    }
}
