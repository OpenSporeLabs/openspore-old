//! Composing a creator document into a morphology — with provenance, and
//! without lying about completeness.
//!
//! # Where this sits
//!
//! ```text
//! Spore_Game.package ──DBPF──> <sporemodel> ──> spore-creator-core
//!                                                            │  canonical authoring data
//!                                                            ▼
//!                                            spore-morphology (this crate)
//!                                                            │  block resolution + provenance
//!                                                            ▼
//!                                       spore-rigblock ──> part pair, body graph
//!                                                            │
//!                          ┌─────────────────────────────────┴──────────────────┐
//!                          ▼                                                    ▼
//!                       catalog                                   spore-engine (projection)
//!            which (group, instance) a document can name          what gets drawn
//! ```
//!
//! Authoring data, morphology semantics and runtime stay separate layers, which
//! is what the historical architecture implies and what this crate keeps: it
//! knows nothing about Bevy, nothing about rendering, and nothing about
//! animation.
//!
//! # The one promise
//!
//! **A composition always says how much of it resolved.** [`Composition::counts`]
//! is a method rather than something a caller computes, because the failure mode
//! this crate exists to prevent is a creature that renders "fine" while quietly
//! containing fewer parts than the document asked for. There is no code path that
//! drops an unresolvable block: it becomes [`UnresolvedPart`], with the hop it
//! reached and the reason it stopped.
//!
//! # Evidence
//!
//! Every hop is graded. A hop measured against the corpus is `Measured`; a hop
//! followed from the chain without an independent witness for that record is
//! `Inferred`, and a part carrying any inferred hop reports that rather than
//! presenting as fully measured.
//!
//! # The catalogue, and the same rule one layer out
//!
//! [`catalog`] turns a set of creator documents into the list of `(group,
//! instance)` pairs they actually name — 680 `<blockref>` blocks over 60
//! distinct parts in the shipped corpus — so a creator tool can browse what is
//! reachable and replace one block with another. It resolves through [`Resolver`]
//! rather than re-walking the chain, and its contribution is the *classification*
//! of what comes back: [`Absent`] keeps a miss, a record that would not decode, a
//! structure that is not what was expected, a source that could not look at all,
//! and a stage the chain never reached as five different statements instead of
//! one `Option`.

#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

pub mod catalog;
pub mod composition;
pub mod dbpf;
pub mod error;
pub mod projection;
pub mod provenance;
pub mod resolve;
pub mod source;

pub use catalog::{
    geometry_of, Absent, Catalog, CatalogCounts, CatalogEntry, DeclaredDependency, Dependencies,
    DocumentRecord, ExtentComparison, Geometry, Lookup, PartReference, RigblockBounds, Skeleton,
    Stop,
};
pub use composition::{Composition, Counts, PartSlot, UnresolvedPart};
pub use dbpf::PackageSource;
pub use error::{MorphologyError, PartDecodeError};
pub use projection::{
    project, Bounds, ProjectedBlock, ProjectedPart, Projection, ProjectionError, Unresolved,
};
pub use provenance::{Grade, Hop, Stage, StopReason, Trail};
pub use resolve::{compose, ResolvedPart, Resolver};
// Re-exported so a consumer can name a part's mesh type without taking a direct
// dependency on `spore-rigblock`: the crate that hands the mesh out should be the
// crate you already depend on, or the geometry type has two homes.
pub use source::{MemorySource, Probe, RecordSource};
pub use spore_rigblock::GmshMesh;
