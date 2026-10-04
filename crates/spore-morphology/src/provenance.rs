//! Where a resolved part came from — as data, not as a string.
//!
//! # Why this is a struct and not a `format!`
//!
//! The expedition's requirement is that every resolved part can answer "which
//! record produced this, and was it measured or inferred". That has to be
//! queryable by a later comparison against another implementation, so it is
//! typed fields. A debug string cannot be compared without re-parsing it, and
//! re-parsing a string to compare two tools is how a divergence hides.
//!
//! Each hop records the `ResourceKey` it went through and whether the step was
//! established or inferred. A hop that was skipped is `None` and the stage says
//! why — never a zero, never a substituted neighbour.

use spore_core::{
    EvidenceLevel, EvidenceState, Provenance, ProvenanceMode, ResourceKey, SourceClass,
};

/// Which hop of the chain a part reached, and where it stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Stage {
    /// The `<blockid>` read out of the document. Always reached for a slot that
    /// exists, because the slot came from a decoded block.
    BlockId,
    /// The rigblock bounds record at the block's own key.
    Rigblock,
    /// The `prop` pointer record at the rigblock's own key.
    Prop,
    /// The part pair at `0x40606000`.
    PartPair,
    /// The `rw4` container of the pair, and any skeleton inside it.
    Container,
}

impl Stage {
    /// Every stage, in chain order.
    pub const ALL: [Stage; 5] = [
        Stage::BlockId,
        Stage::Rigblock,
        Stage::Prop,
        Stage::PartPair,
        Stage::Container,
    ];

    /// A stable machine-readable name.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::BlockId => "block_id",
            Self::Rigblock => "rigblock",
            Self::Prop => "prop",
            Self::PartPair => "part_pair",
            Self::Container => "container",
        }
    }
}

impl core::fmt::Display for Stage {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// How strong the evidence is for one hop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Grade {
    /// Established by a byte-level measurement over the corpus.
    Measured,
    /// Followed from the measured chain but not independently witnessed for
    /// this particular record.
    Inferred,
}

impl Grade {
    /// The evidence level this grade maps to.
    pub const fn level(self) -> EvidenceLevel {
        match self {
            Self::Measured => EvidenceLevel::Observed,
            Self::Inferred => EvidenceLevel::Inferred,
        }
    }

    /// The evidence state this grade maps to.
    pub const fn state(self) -> EvidenceState {
        match self {
            Self::Measured => EvidenceState::Persisted,
            Self::Inferred => EvidenceState::Derived,
        }
    }
}

/// One hop of the chain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hop {
    /// Which hop.
    pub stage: Stage,
    /// The record it went through, if it got that far.
    pub key: Option<ResourceKey>,
    /// How strong the evidence is.
    pub grade: Grade,
    /// Why this hop ended here, when it did not reach the next one.
    ///
    /// A closed vocabulary, and `None` on a hop that succeeded. This is the field
    /// that distinguishes "the corpus has no such record" from "we did not look".
    pub stopped_because: Option<StopReason>,
}

/// Why a hop ended short of the next stage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopReason {
    /// No record of the required type exists at that key in any searched package.
    NoSuchRecord,
    /// A record exists but is not the structure this stage expects.
    WrongStructure,
    /// A record exists and is the right structure but will not decode.
    Undecodable,
    /// The hop is not part of this family's chain.
    NotApplicable,
}

impl StopReason {
    /// A stable machine-readable name.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NoSuchRecord => "no_such_record",
            Self::WrongStructure => "wrong_structure",
            Self::Undecodable => "undecodable",
            Self::NotApplicable => "not_applicable",
        }
    }
}

impl core::fmt::Display for StopReason {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The full trail for one part.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Trail {
    hops: Vec<Hop>,
}

impl Trail {
    /// Starts a trail at the block id stage.
    pub fn new(block_id: ResourceKey) -> Self {
        Self {
            hops: vec![Hop {
                stage: Stage::BlockId,
                key: Some(block_id),
                grade: Grade::Measured,
                stopped_because: None,
            }],
        }
    }

    /// Appends a hop that succeeded.
    pub fn reached(mut self, stage: Stage, key: ResourceKey, grade: Grade) -> Self {
        self.hops.push(Hop {
            stage,
            key: Some(key),
            grade,
            stopped_because: None,
        });
        self
    }

    /// Appends the hop that failed, with why.
    ///
    /// The hop records the key it *tried*, so "we looked at this and it was not
    /// there" is answerable. That distinction is the whole point.
    pub fn stopped_at(
        mut self,
        stage: Stage,
        key: Option<ResourceKey>,
        grade: Grade,
        reason: StopReason,
    ) -> Self {
        self.hops.push(Hop {
            stage,
            key,
            grade,
            stopped_because: Some(reason),
        });
        self
    }

    /// The hops, in chain order.
    pub fn hops(&self) -> &[Hop] {
        &self.hops
    }

    /// The furthest stage reached.
    pub fn furthest(&self) -> Stage {
        self.hops
            .iter()
            .map(|hop| hop.stage)
            .max()
            .expect("a trail always has its first hop")
    }

    /// Whether the chain reached the end.
    pub fn is_complete(&self) -> bool {
        self.furthest() == Stage::Container
            && self.hops.iter().all(|hop| hop.stopped_because.is_none())
    }

    /// The key at a stage, if the chain got there.
    pub fn key_at(&self, stage: Stage) -> Option<ResourceKey> {
        self.hops
            .iter()
            .find(|hop| hop.stage == stage)
            .and_then(|hop| hop.key)
    }

    /// Why the chain stopped, if it did.
    pub fn stopped_because(&self) -> Option<StopReason> {
        self.hops.iter().find_map(|hop| hop.stopped_because)
    }

    /// The weakest grade used anywhere on the trail.
    ///
    /// Deliberately `max` over a rank where `Inferred` outranks `Measured`: one
    /// inferred hop makes the whole trail inferred, because a caller asking "is
    /// this measured?" must not be told yes about a part whose last hop was a
    /// guess.
    pub fn weakest_grade(&self) -> Grade {
        self.hops
            .iter()
            .map(|hop| hop.grade)
            .max_by_key(|grade| match grade {
                Grade::Measured => 0,
                Grade::Inferred => 1,
            })
            .unwrap_or(Grade::Inferred)
    }

    /// Every distinct stage-to-key pair, as `spore_core` provenance records.
    ///
    /// This is what a later cross-tool comparison consumes, so it is produced
    /// from the same fields the resolver used rather than assembled separately.
    ///
    /// The grade does not ride along inside a `Provenance` — that type has no
    /// room for it — so it is returned beside the records by
    /// [`Trail::grades`] so a caller cannot lose the distinction by forgetting a
    /// second lookup.
    pub fn as_provenance(&self) -> Vec<Provenance> {
        self.hops
            .iter()
            .filter_map(|hop| {
                hop.key.map(|key| {
                    Provenance::new(
                        ProvenanceMode::Derived,
                        format!("{}:{key}", stage_label(hop.stage)),
                        SourceClass::Derived,
                    )
                })
            })
            .collect()
    }

    /// The grade of each hop, in the same order as [`Trail::as_provenance`]
    /// skips nothing — it is one entry per hop that carries a key.
    pub fn grades(&self) -> Vec<Grade> {
        self.hops
            .iter()
            .filter(|hop| hop.key.is_some())
            .map(|hop| hop.grade)
            .collect()
    }
}

fn stage_label(stage: Stage) -> &'static str {
    stage.as_str()
}

#[cfg(test)]
mod tests {
    use super::*;

    const BLOCK: u32 = 0x0001_3530;
    const CREATURE: u32 = 0x4062_6000;

    fn key(type_id: u32) -> ResourceKey {
        ResourceKey::new(type_id, CREATURE, BLOCK)
    }

    #[test]
    fn a_complete_trail_reaches_the_container() {
        let trail = Trail::new(key(spore_rigblock::TYPE_RIGBLOCK))
            .reached(
                Stage::Rigblock,
                key(spore_rigblock::TYPE_RIGBLOCK),
                Grade::Measured,
            )
            .reached(Stage::Prop, key(0x00B1_B104), Grade::Measured)
            .reached(Stage::PartPair, key(0x2F4E_681B), Grade::Measured)
            .reached(Stage::Container, key(0x2F4E_681B), Grade::Measured);
        assert!(trail.is_complete());
        assert_eq!(trail.furthest(), Stage::Container);
        assert_eq!(trail.stopped_because(), None);
        assert_eq!(trail.weakest_grade(), Grade::Measured);
        assert_eq!(trail.hops().len(), 5);
    }

    #[test]
    fn a_trail_that_stops_records_the_key_it_tried_and_why() {
        let trail = Trail::new(key(spore_rigblock::TYPE_RIGBLOCK))
            .reached(
                Stage::Rigblock,
                key(spore_rigblock::TYPE_RIGBLOCK),
                Grade::Measured,
            )
            .stopped_at(
                Stage::Prop,
                Some(key(0x00B1_B104)),
                Grade::Measured,
                StopReason::NoSuchRecord,
            );
        assert!(!trail.is_complete());
        assert_eq!(trail.furthest(), Stage::Prop);
        assert_eq!(trail.stopped_because(), Some(StopReason::NoSuchRecord));
        // "We looked here" must be answerable, not just "we did not find one".
        assert_eq!(trail.key_at(Stage::Prop), Some(key(0x00B1_B104)));
    }

    #[test]
    fn a_stop_before_a_key_existed_records_no_key() {
        // The block id is unreadable: there was no key to try.
        let mut trail = Trail::new(key(spore_rigblock::TYPE_RIGBLOCK));
        trail = trail.stopped_at(
            Stage::Rigblock,
            None,
            Grade::Inferred,
            StopReason::NotApplicable,
        );
        assert_eq!(trail.key_at(Stage::Rigblock), None);
        assert_eq!(trail.stopped_because(), Some(StopReason::NotApplicable));
    }

    #[test]
    fn the_weakest_grade_is_reported_not_the_strongest() {
        let trail = Trail::new(key(spore_rigblock::TYPE_RIGBLOCK))
            .reached(
                Stage::Rigblock,
                key(spore_rigblock::TYPE_RIGBLOCK),
                Grade::Measured,
            )
            .reached(Stage::Prop, key(0x00B1_B104), Grade::Inferred);
        assert_eq!(
            trail.weakest_grade(),
            Grade::Inferred,
            "a trail containing an inferred hop must not present as measured"
        );
    }

    #[test]
    fn provenance_comes_from_the_same_fields_the_resolver_used() {
        let trail = Trail::new(key(spore_rigblock::TYPE_RIGBLOCK)).reached(
            Stage::Rigblock,
            key(spore_rigblock::TYPE_RIGBLOCK),
            Grade::Measured,
        );
        let provenance = trail.as_provenance();
        assert_eq!(provenance.len(), 2);
        assert!(provenance[0].reference.contains("block_id"));
        assert!(provenance[1].reference.contains("rigblock"));
        assert_eq!(trail.grades(), vec![Grade::Measured, Grade::Measured]);
    }

    #[test]
    fn stage_names_are_stable_and_unique() {
        let mut names: Vec<&str> = Stage::ALL.iter().map(|stage| stage.as_str()).collect();
        let total = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), total, "two stages share a name");
    }
}
