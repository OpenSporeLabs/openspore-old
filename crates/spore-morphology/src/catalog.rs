//! The authorable part catalogue: every `(group, instance)` a creator document
//! can actually name, with what is really at that key.
//!
//! # What a catalogue is for
//!
//! ```text
//! real <sporemodel> -> distinct (group, instance) -> this module
//!                                                      │ technical identity,
//!                                                      │ provenance, real bounds,
//!                                                      │ real counts, real gaps
//!                                                      ▼
//!                                            a creator tool browses and replaces
//! ```
//!
//! The identity is the whole of it. A `<blockid>` spells a `(group, instance)`
//! pair, and that pair is the only part identity this layer carries: **no field
//! of a `<sporemodel>`, `rigblock`, `prop`, `rw4` or `gmsh` record holds a
//! display name**, so none is offered here. Inventing one would be a fabricated
//! string in a layer whose entire value is that its strings are keys.
//!
//! # Why the entries are deduplicated by identity
//!
//! The 68 shipped creator documents place **680** `<blockref>` blocks, and those
//! blocks name only **60** distinct `(group, instance)` pairs. 58 documents place
//! exactly one distinct pair, and **46 of the 47 creature documents do** — a
//! creature document is a repeated block, not a varied assembly, and a catalogue
//! that listed 680 rows would be a census of blocks pretending to be a list of
//! parts. Both numbers are reported: `references` counts blocks, `entries`
//! counts parts.
//!
//! # The distinctions this module refuses to collapse
//!
//! * **A rigblock bounds box is not the part's extent.** It marks where a part may
//!   be placed. Over the creator-referenced parts the geometry's axis-aligned
//!   extent is a median **0.63x** the rigblock's cell-derived span, ranging
//!   **0.10x to 8.51x** — smaller on the median axis, 8.5x larger at the extreme.
//!   Against the span the record's *header* declares the median is 1.55x over
//!   18.7x, and against the rigblock's widest header offset — the reading
//!   `tests/projection.rs` already uses, whose maximum of **13.21x** this module
//!   reproduces exactly — it is 2.59x over the same 13.21x.
//!   [`ExtentComparison`] holds both extents side by side; neither field
//!   substitutes for the other, and the full four-reading census is on
//!   [`ExtentComparison`].
//! * **A recovered name is not a name in use.** `spore_rigblock::vocabulary` recovers
//!   244 of 2002 bone identities, and across the 680 parts the creator core composes
//!   there are 677 bones with 557 named ones — **every single one of them `joint1`**.
//!   [`Skeleton::names`] reports what this part actually carries, never the dictionary.
//! * **A miss is not an error.** [`Absent::NoSuchRecord`] means every searched
//!   source was asked; [`Absent::Unreadable`] means a record is indexed and would
//!   not come out; [`Absent::NotSearched`] means no package was open and nothing
//!   was asked at all. See [`Lookup`].
//! * **An empty collection is a finding, not an absence.** A part whose container
//!   declares no cross-record reference is `Lookup::Found` with an **empty**
//!   [`Dependencies`], never `Absent`. Measured: all 1824 part containers carry
//!   **zero** validated `{ instance, type, group, 0x00A00CE4 }` entries of any
//!   type, so the part layer has no material or raster **record** dependency to
//!   report. See [`claims`].
//!
//! # The chain is not re-implemented
//!
//! Each entry is resolved by [`Resolver`], the same code the rest of the crate
//! uses, through a one-block document holding that identity. The catalogue adds
//! only the *classification* of the resolver's verdict: which key was tried, and
//! whether the source could look at all. Two implementations of the chain would
//! be two answers for one record, which is the failure this repository treats as
//! worse than a gap.

use std::collections::BTreeMap;
use std::fmt;

use spore_core::{EvidenceLevel, Fact, Provenance, ResourceKey};
use spore_creator_core::{Block, BlockId, BlockIndex, CreatorDocument, Family};

use crate::composition::PartSlot;
use crate::provenance::{Grade, Stage, StopReason, Trail};
use crate::resolve::{Resolver, TYPE_PROP};
use crate::source::{Probe, RecordSource};

/// The rigblock bounds record type. Re-exported so a catalogue caller does not
/// have to reach into `resolve` for the one constant it needs.
pub use crate::resolve::TYPE_RIGBLOCK;

/// The `raster` record type, the only cross-record texture reference whose type
/// id is established.
pub const TYPE_RASTER: u32 = spore_core::record::type_id::RASTER;

/// The tag word that validates a 16-byte `{ instance, type, group, tag }` entry.
const POINTER_TAG: u32 = 0x00A0_0CE4;

/// The width of a validated pointer entry, in bytes.
const POINTER_WIDTH: usize = 16;

/// Where this module's measurements and derivations live.
const PROV_CATALOG: &str = "crates/spore-morphology/src/catalog.rs";

/// The chain walk every entry goes through.
const PROV_RESOLVE: &str = "crates/spore-morphology/src/resolve.rs";

/// The measured chain, the prop-pointer selection rule, and the corpus counts.
const PROV_CREATOR_CORE: &str = "docs/CREATOR-CORE.md §11";

/// The rigblock envelope and the bone-name vocabulary, with their grades.
const PROV_RIGBLOCK_CLAIMS: &str = "crates/spore-rigblock/src/claims.rs";

/// The RW4 section type-code tables, in which `0x20003` is `Raster`.
const PROV_RW4_TYPES: &str = "crates/spore-rw4/src/type_codes.rs";

// ---------------------------------------------------------------------------
// Documents and references
// ---------------------------------------------------------------------------

/// One creator document, as the catalogue sees it.
#[derive(Debug, Clone, Copy)]
pub struct DocumentRecord<'a> {
    /// The record the document was read from.
    ///
    /// Technical identity only. A `<sporemodel>` carries no name field, so this
    /// is the only thing a reader can be told about *which* document a part came
    /// from.
    pub key: ResourceKey,
    /// Which family the document arrived as.
    pub family: Family,
    /// The decoded document.
    pub document: &'a CreatorDocument,
}

/// One block that named a catalogue entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PartReference {
    /// The document that named it, by record identity.
    pub document: ResourceKey,
    /// Which family that document arrived as.
    pub family: Family,
    /// Which block of that document named it.
    pub block: BlockIndex,
}

// ---------------------------------------------------------------------------
// Lookup: a value or a classified non-finding
// ---------------------------------------------------------------------------

/// A value that may be absent, with the absence itself classified.
///
/// [`spore_core::Fact`] already separates "has a value" from "we looked and
/// found nothing", and grades the first. What it does not do is separate *why*
/// nothing was found, and that is the whole reason this envelope exists:
/// `Fact::unavailable("no_such_record")` and
/// `Fact::unavailable("source_not_searched")` are the same two bytes on the wire
/// and opposite findings about the corpus.
///
/// The present half **is** a `Fact`, so a grade and a provenance are never lost
/// by looking at the value.
#[derive(Debug, Clone, PartialEq)]
pub enum Lookup<T> {
    /// Read and decoded. The inner [`Fact`] carries its grade and provenance.
    Found(Fact<T>),
    /// A classified non-finding. Never a zero, an empty collection or `false`.
    Absent(Absent),
}

impl<T> Lookup<T> {
    /// Records a present value, graded and attributed.
    ///
    /// # Panics
    ///
    /// If `provenance` is empty, by [`Fact::new`]'s own contract.
    pub fn found(level: EvidenceLevel, provenance: Vec<Provenance>, value: T) -> Self {
        Self::Found(Fact::new(level, level_state(level), provenance, value))
    }

    /// The value, when present.
    pub fn value(&self) -> Option<&T> {
        match self {
            Self::Found(fact) => fact.value(),
            Self::Absent(_) => None,
        }
    }

    /// Whether a value is present.
    pub fn is_found(&self) -> bool {
        matches!(self, Self::Found(_))
    }

    /// The classified non-finding, when absent.
    pub fn absent(&self) -> Option<&Absent> {
        match self {
            Self::Found(_) => None,
            Self::Absent(absent) => Some(absent),
        }
    }

    /// The grade of the present value, or [`EvidenceLevel::Unknown`] when absent.
    ///
    /// `Unknown` here is a real grade rather than a default: it is what
    /// [`Fact`] assigns an unavailable group, and it is exactly right for a
    /// non-finding.
    pub fn level(&self) -> EvidenceLevel {
        match self {
            Self::Found(fact) => fact.level(),
            Self::Absent(_) => EvidenceLevel::Unknown,
        }
    }

    /// Consumes the envelope, yielding the value when present.
    pub fn into_value(self) -> Option<T> {
        match self {
            Self::Found(fact) => fact.into_value(),
            Self::Absent(_) => None,
        }
    }
}

/// Maps a grade to the state a `Fact` carrying it has.
const fn level_state(level: EvidenceLevel) -> spore_core::EvidenceState {
    match level {
        EvidenceLevel::Unknown => spore_core::EvidenceState::Missing,
        EvidenceLevel::Observed
        | EvidenceLevel::Confirmed
        | EvidenceLevel::Verified
        | EvidenceLevel::Supported
        | EvidenceLevel::Inferred
        | EvidenceLevel::Approximation => spore_core::EvidenceState::Derived,
    }
}

/// Why a lookup produced no value.
///
/// A **closed** vocabulary. Every variant is a different statement, and the ones
/// that are easy to confuse are kept apart on purpose:
/// [`NotSearched`](Self::NotSearched) and
/// [`NoSuchRecord`](Self::NoSuchRecord) are opposite claims about the corpus,
/// and [`NotAttempted`](Self::NotAttempted) says nothing about it at all.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Absent {
    /// **Nothing was searched**: the source holds no package.
    ///
    /// Not a finding about the corpus, and never to be counted as one. This is
    /// the state a caller reaches by pointing at an install that is not there.
    NotSearched,
    /// Every searched source was asked and holds no record at this key.
    NoSuchRecord {
        /// The key that was looked for.
        key: ResourceKey,
    },
    /// A record is indexed at this key and its payload would not come out.
    Unreadable {
        /// The key that was looked for.
        key: ResourceKey,
        /// The index row's raw `compression` word.
        compression: u16,
    },
    /// The bytes came out and the structure refused: present, wrong shape.
    ///
    /// "There is a record here and it is not the thing this stage expects" is a
    /// different finding from "there is no record here", and it is the one a
    /// guessed record type produces.
    WrongStructure {
        /// The record that was there.
        key: ResourceKey,
    },
    /// The record is there, readable, correctly shaped, and does not contain
    /// this.
    ///
    /// A real corpus state rather than a failure: 256 of 1824 part containers
    /// hold no `Skeleton (0x70002)` section, and every catalogue entry whose
    /// part is one of them reports this.
    NotPresent {
        /// The record that was read.
        key: ResourceKey,
        /// What was looked for inside it, as a closed word.
        what: &'static str,
    },
    /// **The lookup was never attempted**, because the chain stopped earlier.
    ///
    /// Not a miss and not an error: the geometry of a block whose `prop` record
    /// will not parse is not known to be absent, and reporting it absent would
    /// be a negative nobody measured.
    NotAttempted {
        /// Where the chain stopped instead.
        stage: Stage,
    },
}

impl Absent {
    /// A stable machine-readable code, closed over this enum.
    pub const fn code(&self) -> &'static str {
        match self {
            Self::NotSearched => "not_searched",
            Self::NoSuchRecord { .. } => "no_such_record",
            Self::Unreadable { .. } => "unreadable",
            Self::WrongStructure { .. } => "wrong_structure",
            Self::NotPresent { .. } => "not_present",
            Self::NotAttempted { .. } => "not_attempted",
        }
    }

    /// The key this absence is about, when there was one to try.
    pub fn key(&self) -> Option<ResourceKey> {
        match self {
            Self::NoSuchRecord { key }
            | Self::Unreadable { key, .. }
            | Self::WrongStructure { key }
            | Self::NotPresent { key, .. } => Some(*key),
            Self::NotSearched | Self::NotAttempted { .. } => None,
        }
    }

    /// Whether this absence is a finding about the corpus at all.
    ///
    /// `false` for [`Self::NotSearched`] and [`Self::NotAttempted`], the two
    /// states that describe the reader rather than the data.
    pub fn is_finding(&self) -> bool {
        !matches!(self, Self::NotSearched | Self::NotAttempted { .. })
    }
}

/// Adds one fact group's outcome to the counts.
///
/// Generic over the value type so the four groups of an entry — which are four
/// unrelated types — are counted by one rule. A hand-written chain of four would
/// be the second place to forget that an empty collection is a finding.
fn tally<T>(counts: &mut CatalogCounts, group: &Lookup<T>) {
    match group.absent() {
        None => counts.absent_facts += 1,
        Some(absent) if !absent.is_finding() => counts.not_a_finding += 1,
        Some(absent) => {
            *counts.absent_codes.entry(absent.code()).or_default() += 1;
        }
    }
}

impl fmt::Display for Stop {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.summary())
    }
}

impl fmt::Display for Absent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code())?;
        if let Some(key) = self.key() {
            write!(f, " at {key}")?;
        }
        match self {
            Self::Unreadable { compression, .. } => {
                write!(f, " (compression {compression:#06x})")
            }
            Self::NotPresent { what, .. } => write!(f, " (no {what})"),
            Self::NotAttempted { stage } => write!(f, " (chain stopped at {stage})"),
            _ => Ok(()),
        }
    }
}

// ---------------------------------------------------------------------------
// The groups a catalogue entry reports
// ---------------------------------------------------------------------------

/// A rigblock's decoded occupancy bounds.
#[derive(Debug, Clone, PartialEq)]
pub struct RigblockBounds {
    /// Subdivision depth `k`.
    ///
    /// `0` means the record is a 36-byte header with **no cells**, so
    /// [`Self::span`] and [`Self::envelope`] are absent rather than zero. Six of
    /// the sixty creator-referenced rigblocks are `k == 0`, and one of them is
    /// shared by the six blocks of
    /// `0x2b978c46:0x40626200:0x05e3df13` — a bounds box that measures nothing,
    /// which is why the fields are `Option` and not `0.0`.
    pub levels: u32,
    /// Occupancy cells carried, `4^levels`.
    pub cells: usize,
    /// The `(low, high)` corners implied by the extreme cells.
    pub envelope: Option<([f32; 3], [f32; 3])>,
    /// The axis-aligned span of [`Self::envelope`], `None` unless every
    /// component is positive.
    pub span: Option<[f32; 3]>,
    /// The span the record's **header** declares, independent of its cells.
    ///
    /// The same quantity as [`Self::span`] measured two ways, and they are not
    /// the same number: the header states where the box is and the cells say how
    /// much of it is occupied, so the cell-derived extent is quantised around the
    /// declared one. Over the creator-referenced parts the median
    /// geometry-to-bounds ratio is **0.63x** against this reading and **1.55x**
    /// against [`Self::span`] — which is why a report that says "the rigblock
    /// bounds" without saying which reading it used has not said anything
    /// checkable. See [`ExtentComparison`].
    ///
    /// `None` when any component is not positive, which is the honest answer for
    /// a degenerate box: a negative or zero extent has no magnitude to compare.
    ///
    /// Available even for a `k == 0` record, which has no cells and therefore no
    /// [`Self::span`]. Six of the sixty referenced rigblocks are in that state.
    pub declared_span: Option<[f32; 3]>,
    /// The largest magnitude among the header's six floats, `None` when it is
    /// zero.
    ///
    /// The **third** reading of "the rigblock bounds", and the one the projection
    /// test measures against: not a span, and not a half-extent, but the furthest
    /// the box reaches from the origin on any axis. It exists here because three
    /// readings of one number is exactly the condition under which a comparison
    /// between two tools silently compares two different quantities.
    pub widest_offset: Option<f32>,
}

impl RigblockBounds {
    /// Whether the record carries any cells, and therefore any envelope.
    ///
    /// A `k == 0` rigblock is not "an empty bounds box"; it is a header that
    /// never encoded one.
    pub fn has_cells(&self) -> bool {
        self.cells > 0
    }
}

/// A part's decoded geometry, as far as a catalogue entry needs it.
#[derive(Debug, Clone, PartialEq)]
pub struct Geometry {
    /// Vertices in the decoded `gmsh` record.
    pub vertices: usize,
    /// Triangles in the decoded `gmsh` record.
    pub triangles: usize,
    /// Axis-aligned `(low, high)` over the decoded positions, or `None` when the
    /// record decoded with no vertices.
    ///
    /// `None` rather than a degenerate zero box: "no vertices" is not "vertices
    /// at the origin", and an infinite corner pair is not a measurement either.
    pub extent: Option<([f32; 3], [f32; 3])>,
}

/// A part's body graph, with the recovered names **this part** carries.
#[derive(Debug, Clone, PartialEq)]
pub struct Skeleton {
    /// Bones in the graph.
    pub bones: usize,
    /// How many of them recovered a name at `Family` or `Literal` confidence.
    pub named: usize,
    /// The recovered names in use, with each bone's index in [`Self::bones`].
    ///
    /// This is deliberately not the 244-entry dictionary in
    /// `spore_rigblock::vocabulary`. Measured over the 680 `<blockref>` parts
    /// the creator core composes: 677 bones, 557 named, and **all 557 are
    /// `joint1`** — no anatomy, no laterality, no `attach`, no `csnap`. The
    /// other 120 bones have identity words outside the recovered vocabulary at
    /// all. A recovered vocabulary is a plausible, coherent dictionary and
    /// coherent is not the same as used.
    pub names: Vec<(&'static str, usize)>,
}

/// One `{ instance, type, group, 0x00A00CE4 }` entry inside a part container.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeclaredDependency {
    /// The record it names.
    pub key: ResourceKey,
    /// Whether a searched source actually holds that record.
    ///
    /// `false` means the part **declares** a reference this build cannot
    /// resolve, which is a finding; it is not the same as the entry not being
    /// there.
    pub resolved: bool,
}

/// Cross-record references a part's container declares.
///
/// # Measured over the real corpus
///
/// **Zero.** All 1824 `0x2F4E681B` records at `0x40606000` were scanned for the
/// validated 16-byte entry form and **none carries one of any type**. So the part
/// layer has no `raster` or material **record** dependency to report, and a
/// catalogue that printed one would have invented it.
///
/// What the containers *do* carry is `Raster (0x20003)` **sections** — inline,
/// 0 to 8 per container over the creator-referenced parts — whose payload layout
/// is not decoded by this workspace and whose referent is not established. This
/// crate does not walk the RW4 section directory (`spore-rw4` owns that, and a
/// second walker here would be a second source of truth waiting to drift), so a
/// section is not a dependency key and is not reported as one.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Dependencies {
    /// Every validated entry found, in the order the container holds them.
    pub declared: Vec<DeclaredDependency>,
}

impl Dependencies {
    /// The declared entries of one record type.
    pub fn of_type(&self, type_id: u32) -> impl Iterator<Item = &DeclaredDependency> {
        self.declared
            .iter()
            .filter(move |d| d.key.type_id == type_id)
    }

    /// The `0x2F4E681C` `raster` records the part names.
    ///
    /// The only texture reference whose record type is established. **There is no
    /// established material record type**: nothing in this layer maps a material
    /// id to a role, so this crate reports material ids as unclassified rather
    /// than inventing a type id to file them under.
    pub fn rasters(&self) -> impl Iterator<Item = &DeclaredDependency> {
        self.of_type(TYPE_RASTER)
    }

    /// The declared entries a searched source holds.
    pub fn resolved(&self) -> impl Iterator<Item = &DeclaredDependency> {
        self.declared.iter().filter(|d| d.resolved)
    }

    /// The declared entries no searched source holds.
    pub fn unresolved(&self) -> impl Iterator<Item = &DeclaredDependency> {
        self.declared.iter().filter(|d| !d.resolved)
    }

    /// Whether the container declared nothing at all.
    ///
    /// A **finding**, not an absence: the container was read and holds no
    /// cross-record reference. Measured over all 1824 real part containers.
    pub fn is_empty(&self) -> bool {
        self.declared.is_empty()
    }
}

/// Where a chain stopped, when it did not reach a part.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stop {
    /// The furthest stage reached.
    pub stage: Stage,
    /// Why it stopped there, in the resolver's own closed vocabulary.
    pub reason: StopReason,
    /// The key it tried, so "we looked here" is answerable rather than implied.
    pub tried: Option<ResourceKey>,
}

impl Stop {
    /// Builds a stop from the resolver's own unresolved record, refining
    /// `NoSuchRecord` into the three things it can actually mean.
    ///
    /// The refinement never contradicts the resolver: it re-probes the very key
    /// the resolver recorded trying, and `RecordSource::probe` agrees with
    /// `RecordSource::record` about whether bytes came out.
    fn from_unresolved(unresolved: &crate::UnresolvedPart, source: &dyn RecordSource) -> Self {
        let tried = unresolved
            .trail
            .as_ref()
            .and_then(|t| t.key_at(unresolved.stage));
        let reason = match (unresolved.reason, tried) {
            (StopReason::NoSuchRecord, Some(key)) => match source.probe(&key) {
                Probe::Present(_) => StopReason::NoSuchRecord,
                Probe::NoSuchRecord => StopReason::NoSuchRecord,
                Probe::Unreadable { .. } => StopReason::Undecodable,
            },
            (reason, _) => reason,
        };
        Self {
            stage: unresolved.stage,
            reason,
            tried,
        }
    }

    /// A one-line account, for a report a human reads.
    pub fn summary(&self) -> String {
        format!(
            "unresolved at {} because {}{}",
            self.stage,
            self.reason,
            match self.tried {
                Some(key) => format!("; tried {key}"),
                None => String::new(),
            }
        )
    }
}

/// The rigblock's placement box beside the geometry's extent.
///
/// # These are two measurements, and neither is the other
///
/// A rigblock is a **bounds box marking where a part may be placed**. It is not
/// the part's extent and it is not its bounding box. The geometry is what gets
/// drawn, and its own axis-aligned extent is what bounds a drawing.
///
/// Measured over the creator-referenced parts, per axis wherever both extents are
/// positive:
///
/// | reading | n | min | p25 | median | p75 | max |
/// |---|---:|---:|---:|---:|---:|---:|
/// | extent / rigblock **cell-derived** span, per axis | 159 | 0.10 | 0.42 | **0.63** | 0.95 | 8.51 |
/// | extent / half of the cell-derived span, per axis | 159 | 0.19 | 0.84 | 1.27 | 1.90 | 17.0 |
/// | extent / rigblock **header-declared** span, per axis | 171 | 0.25 | 1.00 | **1.55** | 2.16 | 18.7 |
/// | widest axis extent / rigblock **widest header offset**, one per part | 58 | 1.06 | 2.00 | 2.59 | 3.78 | 13.21 |
///
/// The last row is the reading `tests/projection.rs` already measures its
/// worst-case ratio against, and its maximum — **13.21x** — is that test's
/// maximum too, so the two agree on the same number. Its median, 2.59x, is a
/// property of the *normalisation* as much as of the parts: dividing the
/// geometry's largest axis extent by the largest magnitude among the rigblock
/// header's six floats cannot come out below 1, which is why this row's minimum
/// is 1.06 while the per-axis readings reach 0.10.
///
/// The two rigblock-span readings also have different populations, and that is
/// the point rather than a nuisance: six of the sixty referenced rigblocks are
/// `k == 0` and carry **no cells at all**, so they have no cell-derived span while
/// their header still declares one. `n = 159` covers the 53 parts that have both a
/// cell-derived span and decoded geometry; `n = 171` adds the twelve axes the
/// `k == 0` records can contribute; `n = 58` is every entry with geometry and a
/// positive widest offset.
///
/// So the geometry is *not* systematically larger than the bounds box on the
/// per-axis reading — it is **smaller** on the median axis, up to 10x smaller and
/// up to 8.5x to 18.7x larger at the extremes — and which rigblock measurement a
/// report quotes moves the median anywhere between 0.63 and 2.59. Stating all four
/// is the honest form; picking the flattering one is how "the rigblock is the
/// part's size" becomes a fact nobody measured.
#[derive(Debug, Clone, PartialEq)]
pub struct ExtentComparison {
    /// The rigblock envelope's axis-aligned span.
    ///
    /// `None` when the rigblock carries no cells — see
    /// [`RigblockBounds::has_cells`].
    pub rigblock_span: Option<[f32; 3]>,
    /// The geometry's axis-aligned `(low, high)` extent, or `None` when the
    /// geometry did not decode.
    pub geometry_extent: Option<([f32; 3], [f32; 3])>,
    /// Per axis, `geometry_extent / rigblock_span`, wherever both are positive.
    ///
    /// `None` on an axis where either side is zero, rather than `Some(0.0)` or
    /// `Some(f64::INFINITY)` — a ratio against a degenerate extent has no value.
    pub axis_ratio: [Option<f32>; 3],
}

impl ExtentComparison {
    /// Builds the comparison from the two measurements, or from whatever is
    /// missing when a stage did not yield a measurement.
    pub fn new(
        rigblock_span: Option<[f32; 3]>,
        geometry_extent: Option<([f32; 3], [f32; 3])>,
    ) -> Self {
        let mut axis_ratio = [None; 3];
        if let (Some(span), Some((low, high))) = (rigblock_span, geometry_extent) {
            for axis in 0..3 {
                let extent = high[axis] - low[axis];
                if span[axis] > 0.0 && extent > 0.0 {
                    axis_ratio[axis] = Some(extent / span[axis]);
                }
            }
        }
        Self {
            rigblock_span,
            geometry_extent,
            axis_ratio,
        }
    }

    /// Whether both sides produced a positive extent on at least one axis.
    pub fn is_comparable(&self) -> bool {
        self.axis_ratio.iter().any(Option::is_some)
    }

    /// The largest per-axis ratio, when there is one.
    pub fn widest_axis_ratio(&self) -> Option<f32> {
        self.axis_ratio
            .iter()
            .filter_map(|ratio| *ratio)
            .fold(None, |widest: Option<f32>, ratio| {
                Some(widest.map_or(ratio, |w: f32| w.max(ratio)))
            })
    }
}

// ---------------------------------------------------------------------------
// The catalogue
// ---------------------------------------------------------------------------

/// One `(group, instance)` a creator document can name in a `<blockid>`.
#[derive(Debug, Clone)]
pub struct CatalogEntry {
    /// The authorable identity: the `(group, instance)` a `<blockid>` writes.
    ///
    /// The whole technical identity of a part in this layer. Nothing here
    /// carries a display name, because nothing in these records does.
    pub id: BlockId,
    /// Every block of every document that named this identity.
    ///
    /// 680 blocks across the 68 shipped documents name 60 entries, so this is
    /// usually many rows long, and one entry is named by **480 of them**:
    /// `0x40626000:0xd05c53a3`, across all 47 creature documents. It is the
    /// provenance a replacement needs, and the histogram that says how shared a
    /// part really is.
    pub references: Vec<PartReference>,
    /// The `rigblock` record, at the identity's own key.
    pub rigblock_key: ResourceKey,
    /// The `prop` record, at the identity's own key.
    pub prop_key: ResourceKey,
    /// The rigblock's decoded occupancy bounds, or the classified non-finding.
    pub rigblock: Lookup<RigblockBounds>,
    /// The `rw4` container half of the part pair, when the chain reached it.
    pub container_key: Option<ResourceKey>,
    /// The `gmsh` geometry half of the pair, when the chain reached it.
    pub geometry_key: Option<ResourceKey>,
    /// The decoded geometry, or the classified non-finding.
    pub geometry: Lookup<Geometry>,
    /// The body graph in the container, or the classified non-finding.
    ///
    /// [`Absent::NotPresent`] is the ordinary answer for the containers that
    /// hold no skeleton section.
    pub skeleton: Lookup<Skeleton>,
    /// Cross-record material and raster references, or the classified
    /// non-finding.
    ///
    /// An **empty** [`Dependencies`] is `Found`, because the container was read
    /// and holds no reference. That is the measured state of all 1824 real part
    /// containers.
    pub dependencies: Lookup<Dependencies>,
    /// The placement box beside the geometry's extent.
    pub extents: ExtentComparison,
    /// The weakest grade anywhere on the chain, when the chain reached a part.
    ///
    /// `None` when it did not reach one: there is no chain to grade. Not the
    /// same as a chain graded `Inferred`, and not the same as no chain at all.
    pub weakest_grade: Option<Grade>,
    /// Where the chain stopped, when it did not reach a part.
    pub unresolved: Option<Stop>,
    /// The resolver's own trail, for a caller that wants the hop list.
    pub trail: Trail,
}

impl CatalogEntry {
    /// The group half of the identity.
    pub fn group_id(&self) -> u32 {
        self.id.group_id
    }

    /// The instance half of the identity.
    pub fn instance_id(&self) -> u32 {
        self.id.instance_id
    }

    /// How many blocks named this part.
    pub fn reference_count(&self) -> usize {
        self.references.len()
    }

    /// How many distinct documents named this part.
    pub fn document_count(&self) -> usize {
        let mut seen: Vec<ResourceKey> = self
            .references
            .iter()
            .map(|reference| reference.document)
            .collect();
        seen.sort_unstable();
        seen.dedup();
        seen.len()
    }

    /// The families whose documents named this part, in census order.
    pub fn families(&self) -> Vec<Family> {
        let mut out: Vec<Family> = self
            .references
            .iter()
            .map(|reference| reference.family)
            .collect();
        out.sort_unstable();
        out.dedup();
        out
    }

    /// Whether the chain reached a part with decoded geometry.
    ///
    /// The question a creator tool asks before offering a replacement. It is
    /// deliberately stricter than "the chain did not stop": an entry can reach
    /// the pair and still fail to produce drawable geometry.
    pub fn is_authorable(&self) -> bool {
        self.geometry.is_found() && self.unresolved.is_none()
    }

    /// The rigblock's span, when it carries cells.
    pub fn rigblock_span(&self) -> Option<[f32; 3]> {
        self.rigblock.value()?.span
    }

    /// The geometry's axis-aligned extent, when it decoded with any vertices.
    pub fn geometry_extent(&self) -> Option<([f32; 3], [f32; 3])> {
        self.geometry.value().and_then(|geometry| geometry.extent)
    }

    /// A one-line account for a report a human reads.
    ///
    /// Technical identity only — there is no name to print.
    pub fn summary(&self) -> String {
        let mut out = format!(
            "0x{:08x}:0x{:08x}  {} block(s) in {} document(s)",
            self.id.group_id,
            self.id.instance_id,
            self.reference_count(),
            self.document_count()
        );
        match (&self.geometry.value(), &self.unresolved) {
            (Some(geometry), _) => out.push_str(&format!(
                "  {} verts / {} tris",
                geometry.vertices, geometry.triangles
            )),
            (None, Some(stop)) => out.push_str(&format!("  {stop}")),
            (None, None) => out.push_str(&format!(
                "  geometry absent: {}",
                self.geometry
                    .absent()
                    .map(Absent::to_string)
                    .unwrap_or_else(|| "unclassified".to_owned())
            )),
        }
        if let Some(skeleton) = self.skeleton.value() {
            out.push_str(&format!(
                "  {} bones, {} named",
                skeleton.bones, skeleton.named
            ));
        }
        if let Some(grade) = self.weakest_grade {
            out.push_str(&format!("  grade {}", grade_name(grade)));
        }
        out
    }
}

/// The stable spelling of a grade, matching the resolver's.
fn grade_name(grade: Grade) -> &'static str {
    match grade {
        Grade::Measured => "measured",
        Grade::Inferred => "inferred",
    }
}

/// Every `(group, instance)` the given documents name, resolved against a
/// source.
#[derive(Debug, Clone)]
pub struct Catalog {
    entries: Vec<CatalogEntry>,
}

impl Catalog {
    /// Enumerates every distinct `(group, instance)` the documents name, in
    /// ascending `(group, instance)` order.
    ///
    /// **Infallible, and deliberately so.** A source that could not be searched
    /// produces a catalogue whose every non-finding says `not_searched`; an
    /// error return would invite a caller to discard the catalogue, which is the
    /// one thing it must not do, since the absence of an install is not a finding
    /// about Spore. Order is by identity so two runs over the same documents
    /// produce the same list.
    pub fn build(source: &dyn RecordSource, documents: &[DocumentRecord<'_>]) -> Self {
        let searchable = source.is_searchable();
        let resolver = Resolver::new(source);

        // (identity, references), collected first so the entries come out in
        // identity order rather than in whatever order the documents arrived.
        let mut collected: BTreeMap<BlockId, Vec<PartReference>> = BTreeMap::new();
        for record in documents {
            for (block, block_ref) in record.document.blocks.iter().enumerate() {
                let Some(id) = block_ref.block_id else {
                    continue;
                };
                collected.entry(id).or_default().push(PartReference {
                    document: record.key,
                    family: record.family,
                    block,
                });
            }
        }

        let entries = collected
            .into_iter()
            .map(|(id, references)| build_entry(source, searchable, &resolver, id, references))
            .collect();

        Self { entries }
    }

    /// Every entry, in ascending `(group, instance)` order.
    pub fn entries(&self) -> &[CatalogEntry] {
        &self.entries
    }

    /// One entry by identity.
    pub fn entry(&self, id: BlockId) -> Option<&CatalogEntry> {
        self.entries.iter().find(|entry| entry.id == id)
    }

    /// How many entries a catalogue holds.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the catalogue holds no entries.
    ///
    /// An empty catalogue is a statement about the **documents**, not about
    /// Spore: it means no block named a readable `<blockid>`. It is not evidence
    /// that no parts exist.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Every other entry: the "browse alternatives, then replace" move.
    ///
    /// Alternatives are **not** grouped by anatomy, and the group id is not a
    /// substitute. Over the shipped corpus the group id is the document family's
    /// bucket — `0x40626200` for creature, `0x40616200` for cell — so grouping by
    /// it would return one family, not a class of interchangeable parts. Nothing
    /// in these records establishes which parts may stand in for which, so the
    /// catalogue offers the whole reachable set and refuses to imply a taxonomy
    /// it cannot support.
    pub fn alternatives(&self, id: BlockId) -> impl Iterator<Item = &CatalogEntry> {
        self.entries.iter().filter(move |entry| entry.id != id)
    }

    /// Only the entries a creator tool could actually place.
    pub fn authorable(&self) -> impl Iterator<Item = &CatalogEntry> {
        self.entries.iter().filter(|entry| entry.is_authorable())
    }

    /// The counts, so completeness is one field read away.
    pub fn counts(&self) -> CatalogCounts {
        let mut counts = CatalogCounts {
            entries: self.entries.len(),
            ..CatalogCounts::default()
        };
        for entry in &self.entries {
            counts.references += entry.reference_count();
            counts.documents += entry.document_count();
            if entry.geometry.is_found() {
                counts.with_geometry += 1;
            }
            if entry.is_authorable() {
                counts.authorable += 1;
            }
            if entry.skeleton.is_found() {
                counts.with_skeleton += 1;
            }
            if let Some(skeleton) = entry.skeleton.value() {
                counts.bones += skeleton.bones;
                counts.named_bones += skeleton.named;
            }
            if let Some(unresolved) = &entry.unresolved {
                counts.unresolved += 1;
                *counts
                    .stops
                    .entry((unresolved.stage.as_str(), unresolved.reason.as_str()))
                    .or_default() += 1;
            }
            if let Some(grade) = entry.weakest_grade {
                *counts.grades.entry(grade_name(grade)).or_default() += 1;
            }
            tally(&mut counts, &entry.rigblock);
            tally(&mut counts, &entry.geometry);
            tally(&mut counts, &entry.skeleton);
            tally(&mut counts, &entry.dependencies);
        }
        counts
    }

    /// Every bone name actually in use, with its count.
    ///
    /// The distribution, over the catalogue's own population — **not** the
    /// 244-name dictionary. Over the shipped corpus this is
    /// `{ "joint1": 2 }`: two named bones across 57 distinct parts, and no other
    /// word anywhere. That is the population an authoring tool will meet, so it
    /// is the population a catalogue reports.
    pub fn bone_name_histogram(&self) -> BTreeMap<&'static str, usize> {
        let mut out = BTreeMap::new();
        for entry in &self.entries {
            let Some(skeleton) = entry.skeleton.value() else {
                continue;
            };
            for (name, _) in &skeleton.names {
                *out.entry(*name).or_insert(0usize) += 1;
            }
        }
        out
    }
}

/// How much of a catalogue is real.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CatalogCounts {
    /// Distinct `(group, instance)` pairs.
    pub entries: usize,
    /// Blocks that named one. `references` is the block census; `entries` is the
    /// part census, and the two are not expected to be equal.
    pub references: usize,
    /// Distinct document-to-part relationships.
    pub documents: usize,
    /// Entries whose geometry decoded.
    pub with_geometry: usize,
    /// Entries a tool could place: geometry decoded and no stop.
    pub authorable: usize,
    /// Entries whose container carries a skeleton section.
    pub with_skeleton: usize,
    /// Bones across those body graphs.
    pub bones: usize,
    /// Bones that recovered a name.
    pub named_bones: usize,
    /// Entries whose chain stopped before a part.
    pub unresolved: usize,
    /// Grade of each stopped chain, by stable spelling.
    pub grades: BTreeMap<&'static str, usize>,
    /// Stop stage and reason, by the resolver's own stable spellings.
    pub stops: BTreeMap<(&'static str, &'static str), usize>,
    /// Fact groups that produced a value.
    pub absent_facts: usize,
    /// Fact groups whose absence describes the **reader** rather than the data:
    /// a source with no package, or a stage the chain never reached.
    ///
    /// A caller that counts these as corpus findings is counting its own
    /// failure modes, which is why they are counted separately.
    pub not_a_finding: usize,
    /// Corpus findings by [`Absent::code`].
    pub absent_codes: BTreeMap<&'static str, usize>,
}

// ---------------------------------------------------------------------------
// Building one entry
// ---------------------------------------------------------------------------

/// Resolves one identity and classifies every group of the result.
fn build_entry(
    source: &dyn RecordSource,
    searchable: bool,
    resolver: &Resolver<'_>,
    id: BlockId,
    references: Vec<PartReference>,
) -> CatalogEntry {
    // One block, holding this identity. Going through the resolver rather than
    // walking the chain here is the point: the catalogue must not be a second
    // implementation of the chain.
    let document = CreatorDocument {
        format_version: None,
        properties: Default::default(),
        blocks: vec![Block {
            block_id: Some(id),
            ..Block::default()
        }],
    };
    let composition = resolver
        .compose(&document)
        .expect("a one-block document composes");

    let rigblock_key = ResourceKey::new(TYPE_RIGBLOCK, id.group_id, id.instance_id);
    let prop_key = ResourceKey::new(TYPE_PROP, id.group_id, id.instance_id);

    let rigblock = rigblock_group(source, searchable, rigblock_key);

    match &composition.slots()[0] {
        PartSlot::Resolved(part) => {
            let part = part.as_ref();
            let geometry_extent = part.geometry.as_ref().and_then(geometry_extent);
            let span = positive_span(part.rigblock.span());
            let geometry = match &part.geometry {
                Some(mesh) => Lookup::found(
                    EvidenceLevel::Observed,
                    vec![Provenance::derived(PROV_RESOLVE)],
                    Geometry {
                        vertices: mesh.positions.len(),
                        triangles: mesh.triangle_count(),
                        extent: geometry_extent,
                    },
                ),
                // Unreachable through the resolver: it stops the chain rather
                // than returning a part without geometry. Stated rather than
                // left as an `unwrap` on an invariant nobody wrote down.
                None => Lookup::Absent(Absent::NotAttempted {
                    stage: Stage::PartPair,
                }),
            };
            let skeleton = match &part.skeleton {
                Some(skeleton) => {
                    let names: Vec<(&'static str, usize)> = skeleton
                        .named_bones()
                        .into_iter()
                        .map(|(index, name)| (name, index))
                        .collect();
                    Lookup::found(
                        EvidenceLevel::Observed,
                        vec![
                            Provenance::derived(PROV_RIGBLOCK_CLAIMS),
                            Provenance::derived(PROV_CATALOG),
                        ],
                        Skeleton {
                            bones: skeleton.bones.len(),
                            named: names.len(),
                            names,
                        },
                    )
                }
                None => Lookup::Absent(Absent::NotPresent {
                    key: part.container_key,
                    what: "skeleton section",
                }),
            };
            let dependencies = dependencies_group(source, searchable, part.container_key);
            CatalogEntry {
                id,
                references,
                rigblock_key,
                prop_key,
                rigblock,
                container_key: Some(part.container_key),
                geometry_key: part.geometry_key,
                geometry,
                skeleton,
                dependencies,
                extents: ExtentComparison::new(span, geometry_extent),
                weakest_grade: Some(part.trail.weakest_grade()),
                unresolved: None,
                trail: part.trail.clone(),
            }
        }
        // A source with no package open stops the chain at the first hop, which
        // would otherwise read as `NotAttempted { stage: Rigblock }` for every
        // downstream group. That is the wrong finding: nothing was skipped, the
        // source simply had nothing to look in. Overriding here is what makes
        // "we did not look" distinguishable from "we looked and the chain ran out
        // of road", and it is why the arm comes before the ordinary stop.
        PartSlot::Unresolved(_) if !searchable => CatalogEntry {
            id,
            references,
            rigblock_key,
            prop_key,
            rigblock: Lookup::Absent(Absent::NotSearched),
            container_key: None,
            geometry_key: None,
            geometry: Lookup::Absent(Absent::NotSearched),
            skeleton: Lookup::Absent(Absent::NotSearched),
            dependencies: Lookup::Absent(Absent::NotSearched),
            extents: ExtentComparison::new(None, None),
            weakest_grade: None,
            unresolved: Some(Stop {
                stage: Stage::Rigblock,
                reason: StopReason::NoSuchRecord,
                tried: None,
            }),
            trail: Trail::new(rigblock_key),
        },
        PartSlot::Unresolved(unresolved) => {
            let stop = Stop::from_unresolved(unresolved, source);
            let stage = stop.stage;
            let trail = unresolved.trail.clone().unwrap_or_default();
            CatalogEntry {
                id,
                references,
                rigblock_key,
                prop_key,
                rigblock: rigblock.clone(),
                container_key: None,
                geometry_key: None,
                geometry: Lookup::Absent(Absent::NotAttempted { stage }),
                skeleton: Lookup::Absent(Absent::NotAttempted { stage }),
                dependencies: Lookup::Absent(Absent::NotAttempted { stage }),
                extents: ExtentComparison::new(
                    rigblock.value().and_then(|bounds| bounds.span),
                    None,
                ),
                weakest_grade: None,
                unresolved: Some(stop),
                trail,
            }
        }
    }
}

/// Reads and decodes the rigblock, or classifies why there is nothing to report.
fn rigblock_group(
    source: &dyn RecordSource,
    searchable: bool,
    key: ResourceKey,
) -> Lookup<RigblockBounds> {
    if !searchable {
        return Lookup::Absent(Absent::NotSearched);
    }
    let bytes = match source.probe(&key) {
        Probe::Present(bytes) => bytes,
        Probe::NoSuchRecord => return Lookup::Absent(Absent::NoSuchRecord { key }),
        Probe::Unreadable { compression } => {
            return Lookup::Absent(Absent::Unreadable { key, compression })
        }
    };
    match spore_rigblock::Rigblock::decode(&bytes) {
        Ok(rigblock) => {
            let declared = [
                rigblock.min_high[0] - rigblock.max_low[0],
                rigblock.min_high[1] - rigblock.max_low[1],
                rigblock.min_high[2] - rigblock.max_low[2],
            ];
            Lookup::found(
                EvidenceLevel::Observed,
                vec![Provenance::derived(PROV_RIGBLOCK_CLAIMS)],
                RigblockBounds {
                    levels: rigblock.levels,
                    cells: rigblock.cells.len(),
                    envelope: rigblock.envelope(),
                    span: positive_span(rigblock.span()),
                    declared_span: positive_span(Some(declared)),
                    widest_offset: (0..3)
                        .flat_map(|axis| {
                            [rigblock.max_low[axis].abs(), rigblock.min_high[axis].abs()]
                        })
                        .fold(None, |widest: Option<f32>, offset| {
                            Some(widest.map_or(offset, |w: f32| w.max(offset)))
                        })
                        .filter(|widest| *widest > 0.0),
                },
            )
        }
        Err(_) => Lookup::Absent(Absent::WrongStructure { key }),
    }
}

/// Scans the part container for validated cross-record entries.
fn dependencies_group(
    source: &dyn RecordSource,
    searchable: bool,
    container_key: ResourceKey,
) -> Lookup<Dependencies> {
    if !searchable {
        return Lookup::Absent(Absent::NotSearched);
    }
    let bytes = match source.probe(&container_key) {
        Probe::Present(bytes) => bytes,
        Probe::NoSuchRecord => return Lookup::Absent(Absent::NoSuchRecord { key: container_key }),
        Probe::Unreadable { compression } => {
            return Lookup::Absent(Absent::Unreadable {
                key: container_key,
                compression,
            })
        }
    };
    let declared = validated_entries(&bytes)
        .into_iter()
        .map(|key| DeclaredDependency {
            resolved: matches!(source.probe(&key), Probe::Present(_)),
            key,
        })
        .collect();
    // Found, even when empty. "The container was read and holds no cross-record
    // reference" is a measurement, and reporting it as an absence would make a
    // complete absence look like something nobody looked for.
    Lookup::found(
        EvidenceLevel::Observed,
        vec![
            Provenance::derived(PROV_CATALOG),
            Provenance::derived(PROV_RW4_TYPES),
        ],
        Dependencies { declared },
    )
}

/// Every validated `{ instance, type, group, 0x00A00CE4 }` entry in a record.
///
/// The same 16-byte form the resolver selects a part by, and the same
/// tag-and-type validation, so a dependency and a part are read by one rule. The
/// difference is deliberate: this scan takes **every** type, not only `0x2F4E681B`,
/// because the question here is "does this part name any record at all".
///
/// Stepping four bytes at a time rather than sixteen is not a relaxation. The tag
/// is what validates the entry, and a container can hold padding between entries;
/// skipping sixteen would miss any entry that follows one, and every entry is
/// sixteen bytes wide precisely because the form was validated at that stride
/// elsewhere in the corpus.
fn validated_entries(bytes: &[u8]) -> Vec<ResourceKey> {
    let read = |at: usize| -> u32 {
        u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
    };
    let mut out = Vec::new();
    let mut offset = 0usize;
    while offset + POINTER_WIDTH <= bytes.len() {
        if read(offset + 12) == POINTER_TAG {
            out.push(ResourceKey::new(
                read(offset + 4),
                read(offset + 8),
                read(offset),
            ));
        }
        offset += 4;
    }
    out
}

/// The span, unless a component is not positive.
///
/// A zero or negative extent has no magnitude, and reporting one as `0.0` or as
/// infinity would make a degenerate box comparable with a real one. Measured over
/// the creator-referenced parts every axis is positive, so this filter removes
/// nothing real — it exists so that a synthetic or future record cannot slip a
/// meaningless ratio into [`ExtentComparison`].
fn positive_span(span: Option<[f32; 3]>) -> Option<[f32; 3]> {
    span.filter(|span| span.iter().all(|value| *value > 0.0))
}

/// The axis-aligned extent over a mesh's decoded positions.
fn geometry_extent(mesh: &spore_rigblock::GmshMesh) -> Option<([f32; 3], [f32; 3])> {
    let first = mesh.positions.first()?;
    let mut low = *first;
    let mut high = *first;
    for position in &mesh.positions[1..] {
        for axis in 0..3 {
            low[axis] = low[axis].min(position[axis]);
            high[axis] = high[axis].max(position[axis]);
        }
    }
    Some((low, high))
}

// ---------------------------------------------------------------------------
// What this crate does not know
// ---------------------------------------------------------------------------

/// What the catalogue does **not** know, with the grade for each.
///
/// Following the repository rule: a claim is extended when something is learned
/// and never deleted because it is inconvenient. An unavailable fact is *we
/// looked and found nothing*, never a zero, an empty collection or `false`.
pub mod claims {
    use spore_core::{EvidenceLevel, EvidenceState, Fact, Provenance};

    use super::{PROV_CATALOG, PROV_CREATOR_CORE, PROV_RIGBLOCK_CLAIMS, PROV_RW4_TYPES};

    /// One recorded limit or non-finding.
    #[derive(Debug, Clone)]
    pub struct Claim {
        /// What is not known.
        pub subject: &'static str,
        /// The finding. [`Fact::unavailable`] means "looked, found nothing".
        pub fact: Fact<&'static str>,
    }

    /// No display name for any part in this layer.
    pub const PART_DISPLAY_NAME: &str = "part_display_name";

    /// No established material record type, and no material-id-to-role registry.
    pub const MATERIAL_REGISTRY: &str = "material_registry";

    /// The referent of a container's inline `Raster (0x20003)` section.
    pub const INLINE_RASTER_SECTION: &str = "inline_raster_section";

    /// Which parts may substitute for which.
    pub const SUBSTITUTABILITY: &str = "part_substitutability";

    /// Which rigblock subdivision level drives which parameter.
    pub const RIGBLOCK_LEVEL_SEMANTICS: &str = "rigblock_level_semantics";

    /// Every claim this module records.
    pub fn all() -> Vec<Claim> {
        vec![
            Claim {
                subject: PART_DISPLAY_NAME,
                fact: Fact::unavailable(
                    "no display name for a part: no field of a <sporemodel>, rigblock, \
                     prop, rw4 or gmsh record carries one, so the catalogue reports the \
                     (group, instance) identity and nothing beside it",
                ),
            },
            Claim {
                subject: MATERIAL_REGISTRY,
                fact: Fact::unavailable(
                    "the material record type and the role of any material id: the part \
                     layer declares no cross-record reference at all, measured over all \
                     1824 rw4 records at 0x40606000, so there is no id here to classify \
                     and no registry that would say what one is for",
                ),
            },
            Claim {
                subject: INLINE_RASTER_SECTION,
                fact: Fact::unavailable(format!(
                    "what a part container's inline Raster (0x20003) section holds: the \
                     section type code is named in {PROV_RW4_TYPES}, but its payload \
                     layout is not decoded by this workspace and its referent is not \
                     established. Creator-referenced parts carry 0 to 8 of them, so the \
                     part layer stores its own texture rather than naming one"
                )),
            },
            Claim {
                subject: SUBSTITUTABILITY,
                fact: Fact::unavailable(
                    "which parts may stand in for which: nothing in these records \
                     establishes a class of interchangeable parts, and a group id is the \
                     document family's bucket rather than an anatomy class, so \
                     Catalog::alternatives offers the reachable set and implies no \
                     taxonomy",
                ),
            },
            Claim {
                subject: RIGBLOCK_LEVEL_SEMANTICS,
                // Delegated rather than restated: this crate does not decode the
                // grid, so claiming the semantics are unknown here would be
                // disagreeing with the crate that owns them.
                fact: Fact::new(
                    EvidenceLevel::Inferred,
                    EvidenceState::Derived,
                    vec![Provenance::derived(PROV_RIGBLOCK_CLAIMS)],
                    "a rigblock bounds box marks where a part may be placed and is not \
                     the part's extent; which parameter each subdivision level drives \
                     is UNKNOWN and is recorded by spore-rigblock",
                ),
            },
            Claim {
                subject: "referenced_part_count",
                fact: Fact::new(
                    EvidenceLevel::Observed,
                    EvidenceState::Derived,
                    vec![
                        Provenance::derived(PROV_CREATOR_CORE),
                        Provenance::derived(PROV_CATALOG),
                    ],
                    "the 68 shipped creator documents place 680 <blockref> blocks, and \
                     those blocks name 60 distinct (group, instance) pairs; 58 of the \
                     68 documents place exactly one, and 46 of the 47 creature \
                     documents do, so a catalogue must not present a creature document \
                     as a varied assembly",
                ),
            },
            Claim {
                subject: "in_use_bone_vocabulary",
                fact: Fact::new(
                    EvidenceLevel::Observed,
                    EvidenceState::Derived,
                    vec![
                        Provenance::derived(PROV_CREATOR_CORE),
                        Provenance::derived(PROV_RIGBLOCK_CLAIMS),
                    ],
                    "across the 680 <blockref> parts there are 677 bones, 557 of them \
                     named, and every named bone is joint1; the 120 unnamed bones have \
                     identity words outside the recovered 244-name vocabulary, so the \
                     dictionary is a correct hash table and almost entirely unused here",
                ),
            },
        ]
    }
}
