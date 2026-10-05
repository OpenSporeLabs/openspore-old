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
//!                                                            ▼
//!                                                  spore-engine (projection)
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

#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

pub mod composition;
pub mod dbpf;
pub mod error;
pub mod projection;
pub mod provenance;
pub mod resolve;
pub mod source;

pub use composition::{Composition, Counts, PartSlot, UnresolvedPart};
pub use dbpf::PackageSource;
pub use error::{MorphologyError, PartDecodeError};
pub use projection::{
    project, Bounds, ProjectedBlock, ProjectedPart, Projection, ProjectionError, Unresolved,
};
pub use provenance::{Grade, Hop, Stage, StopReason, Trail};
pub use resolve::{compose, ResolvedPart, Resolver};
pub use source::{MemorySource, RecordSource};
