//! What this crate does **not** know, with the grade for each.
//!
//! Following the repository rule: a claim is extended when something is learned
//! and never deleted because it is inconvenient. An unavailable fact is *we
//! looked and found nothing*, never a zero, an empty collection, or `false`.

use spore_core::{EvidenceLevel, EvidenceState, Fact, Provenance};

/// One recorded limit or non-finding.
#[derive(Debug, Clone)]
pub struct Claim {
    /// What is not known.
    pub subject: &'static str,
    /// The finding. `Fact<&'static str>`: available means a measured value.
    pub fact: Fact<&'static str>,
}

/// Where the measurements behind these claims are written down.
const REPORT_08: &str = "docs/analysis/creator/08-rigblock-tail.md";
const REPORT_10: &str = "docs/analysis/creator/10-rigblock-paired-container.md";
const REPORT_11: &str = "docs/analysis/creator/11-bone-identity-vocabulary.md";

/// The record-level id at `d+0x10` of a skeleton section.
pub const SKELETON_SECTION_ID: &str = "skeleton_section_id";

/// Array 2 of a skeleton: values in `{0,1,2,3}` only.
pub const BONE_KIND_FIELD: &str = "bone_kind_field";

/// Which rigblock parameter each subdivision level drives.
pub const LEVEL_SEMANTICS: &str = "rigblock_level_semantics";

/// Why some rigblocks have no counterpart at `0x40606000`.
pub const MISSING_COUNTERPARTS: &str = "rigblock_missing_counterparts";

/// How many rigblocks resolve to a part, and how many do not.
pub const RESOLUTION_COUNTS: &str = "rigblock_resolution_counts";

/// The remaining unnamed bone identities.
pub const UNNAMED_BONES: &str = "unnamed_bone_identities";

/// Whether `attach` / `csnap` bones cross part boundaries.
pub const ATTACHMENT_SCOPE: &str = "attachment_scope";

/// Which transform is the rest pose.
pub const REST_POSE: &str = "rest_pose_selection";

/// Where the creature's anatomical vocabulary is actually used.
pub const ANATOMY_LOCATION: &str = "anatomy_location";

/// Bone identities in the whole-body `gmdl` layer.
pub const GMDL_BONE_IDENTITY: &str = "gmdl_bone_identity";

/// Every claim this crate records.
pub fn all() -> Vec<Claim> {
    vec![
        Claim {
            subject: SKELETON_SECTION_ID,
            fact: Fact::unavailable(format!(
                "the meaning of the skeleton section's d+0x10 id word: 1352 distinct \
                 values across 6270 sections and never the record's own instance id \
                 (0 of 6270); no registry or name table exists ({REPORT_10} Â§4)"
            )),
        },
        Claim {
            subject: BONE_KIND_FIELD,
            fact: Fact::unavailable(format!(
                "the meaning of skeleton array 2: values lie in {{0,1,2,3}} in 6270 of \
                 6270 sections and correlate with nothing yet measured, so it is kept \
                 for being stable rather than for being understood ({REPORT_10} Â§4)"
            )),
        },
        Claim {
            subject: LEVEL_SEMANTICS,
            fact: Fact::unavailable(format!(
                "which rigblock parameter each subdivision level drives: the writer \
                 FUN_0041aaa0 calls model.vtable[0x74]/[0x80], which were not resolved. \
                 The encoding is verified; the levels are not named ({REPORT_08} Â§7)"
            )),
        },
        Claim {
            subject: MISSING_COUNTERPARTS,
            fact: Fact::unavailable(format!(
                "why 233 of 1549 rigblocks have no 0x40606000 counterpart, and why flora \
                 has none at all (0 of 107): a second geometry layer is a lead, not a \
                 finding ({REPORT_10} Â§5)"
            )),
        },
        Claim {
            subject: RESOLUTION_COUNTS,
            // This one IS available. The counts were measured, and the point of
            // recording it is that the *unresolved* number is data, not silence.
            fact: Fact::new(
                EvidenceLevel::Observed,
                EvidenceState::Derived,
                vec![
                    Provenance::derived(REPORT_08),
                    Provenance::derived(REPORT_10),
                ],
                "1316 of 1549 rigblocks reach a part at 0x40606000; 233 do not, and \
                 must never be filled with a substitute mesh or a placeholder body",
            ),
        },
        Claim {
            subject: UNNAMED_BONES,
            fact: Fact::unavailable(format!(
                "names for 1758 of the 2002 bone identities: 244 were recovered, \
                 covering 43% of bone occurrences, and the remainder is dominated by \
                 per-asset names such as CA_shoulders_01 that cannot be enumerated \
                 without the asset database ({REPORT_11} Â§2)"
            )),
        },
        Claim {
            subject: ATTACHMENT_SCOPE,
            fact: Fact::unavailable(format!(
                "whether attach / csnap bones reference other parts: both are recovered \
                 names living inside one part's skeleton, and no array in 0x70002 stores \
                 a part-to-part reference because the payload has zero slack \
                 ({REPORT_11} Â§6)"
            )),
        },
        Claim {
            subject: ANATOMY_LOCATION,
            fact: Fact::unavailable(
                "where Spore's anatomical bone names (hip, knee, eye, iris, jaw, \
                 shoulder) are used: the 244 recovered names are correct hashes, but \
                 across the 677 bones in the 680 composed <blockref> parts the ONLY \
                 recovered name is joint1 - no anatomy, no laterality, no \
                 attach/csnap. The creator core's parts are simple articulated blocks, \
                 so the anatomy belongs to a different record (report 11 section 11)",
            ),
        },
        Claim {
            subject: GMDL_BONE_IDENTITY,
            fact: Fact::unavailable(
                "bone identities in the whole-body gmdl layer: gmdl is NOT an RW4 \
                 container, so 0 of the 4209 records carry a 0x70002 Skeleton section. \
                 All 4209 live in Spore_Content.package. spore-gmdl already exposes \
                 bone_ranges from the trailer but not bone identities, which would sit \
                 in the anim data or baked deforms",
            ),
        },
        Claim {
            subject: REST_POSE,
            fact: Fact::unavailable(format!(
                "which of a part's transforms is its rest pose: 10859 orthonormal 3x3 \
                 transforms exist in section 0x12000E05 and nothing indexes or labels \
                 them ({REPORT_11} Â§3)"
            )),
        },
    ]
}

/// The claim for one subject.
pub fn claim(subject: &str) -> Option<Claim> {
    all().into_iter().find(|claim| claim.subject == subject)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_subject_is_unique_and_findable() {
        let claims = all();
        let mut subjects: Vec<&str> = claims.iter().map(|claim| claim.subject).collect();
        let total = subjects.len();
        subjects.sort_unstable();
        subjects.dedup();
        assert_eq!(subjects.len(), total, "two claims share a subject");
        for recorded in &claims {
            assert!(claim(recorded.subject).is_some());
        }
    }

    #[test]
    fn exactly_one_claim_is_available_and_it_is_the_measured_one() {
        let claims = all();
        let available: Vec<&str> = claims
            .iter()
            .filter(|claim| claim.fact.is_available())
            .map(|claim| claim.subject)
            .collect();
        assert_eq!(
            available,
            vec![RESOLUTION_COUNTS],
            "a claim became available without a measurement, or the measured one was lost"
        );
    }

    #[test]
    fn an_unavailable_claim_is_not_a_zero() {
        // The discipline in one assertion: a non-finding carries no value at all.
        let claim = claim(REST_POSE).expect("recorded");
        assert!(!claim.fact.is_available());
        assert!(claim.fact.value().is_none());
        assert_eq!(claim.fact.level(), EvidenceLevel::Unknown);
        assert_eq!(claim.fact.evidence_state(), EvidenceState::Missing);
    }

    #[test]
    fn the_available_claim_names_its_provenance() {
        let claim = claim(RESOLUTION_COUNTS).expect("recorded");
        assert!(claim.fact.is_available());
        assert!(!claim.fact.provenance().is_empty());
        assert!(claim
            .fact
            .value()
            .is_some_and(|value| value.contains("1316")));
    }
}
