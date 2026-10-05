//! Core OpenSpore domain types.
//!
//! This crate is deliberately dependency-free (no Bevy, no I/O, no GPU). Every
//! other crate in the workspace depends on it, so it must stay cheap to compile
//! and free of platform assumptions.
//!
//! # Why a provenance vocabulary lives here
//!
//! OpenSpore is a clean-room reimplementation whose behaviour is reconstructed
//! from evidence rather than from source. Two of those evidence grades are
//! load-bearing *everywhere* in the engine:
//!
//! * a **record's identity** is a `(type, group, instance)` triple — the DBPF
//!   "TGI" — and nothing in Spore's containers names a record any other way;
//! * a **claim about a record's meaning** carries an evidence grade. Without
//!   one, an inference and an observation become indistinguishable downstream,
//!   which is exactly the failure mode the whole repository exists to avoid.
//!
//! The [`evidence`] module is therefore a domain type, not documentation.
//!
//! # Clean-room boundary
//!
//! Nothing here is derived from EA/Maxis code. The container layouts and the
//! evidence vocabulary are reimplementations from format analysis; see
//! `docs/ASSET-PATH.md`, `docs/RENDERWARE-RESEARCH.md` and
//! `docs/tooling/semantic-exchange.md`.

#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

pub mod evidence;
pub mod key;
pub mod orientation;
pub mod record;

pub use evidence::{
    EvidenceLevel, EvidenceState, Fact, FactState, Provenance, ProvenanceMode, SourceClass,
};
pub use key::{parse_id, ResourceKey, WILDCARD};
pub use orientation::{y_rotation_matrix, Orientation};
pub use record::{fourcc, Matrix3, RecordType};
