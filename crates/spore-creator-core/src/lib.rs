//! # The Creator Core
//!
//! Spore's creators — creature, vehicle, building, and the others — all
//! assemble a thing out of parts. This crate is the part of that which the
//! corpus shows them to share, and nothing else.
//!
//! ## What this crate is grounded in
//!
//! Spore ships **68 creator documents** in `Spore_Game.package`, as text:
//! records of type `crt` (47), `cll` (13), `vcl` (4), `ufo` (2), `bld` (1) and
//! `flr` (1), each an XML document rooted at `<sporemodel>` carrying 680
//! `<blockref>` part instances between them. All 68 parse; the grammar is
//! catalogued in `docs/CREATOR-CORE.md` §1 and reproduced by census in
//! `docs/analysis/creator/01-domain-model.md` … `06-semantic-evidence.md`.
//!
//! That is the whole justification for a shared core: **one document grammar
//! and one code type** (`Editors::cEditorResourceBlock`, 472 bytes, named
//! generically rather than per-domain) across six record families.
//!
//! ## What this crate is not
//!
//! It is not a shared editor *behaviour*, because the corpus does not support
//! one: 469 `/Spore/Editors/*` types exist and not one mentions vehicle,
//! building, UFO or terrain, and no `cVehicleEditor` class-name string exists in
//! the binary. Six data families, one data core, separate clients.
//!
//! It is also not Bevy, not a renderer, and not an editor UI. The authoring
//! model is ordinary Rust so it can be serialised, undone, validated and tested
//! with no window, no GPU and no game install. The projection into engine
//! types lives in `spore-engine`, never here — this crate may not depend on it.
//!
//! ## Persistence
//!
//! [`persist`] is the canonical interchange format, `spore-creator-doc/1`: a
//! JSON Lines file whose first line is a metadata record and whose remaining
//! lines are one block each in document order. It exists because a creator that
//! only lives in RAM is not a creator, and its contract is narrow on purpose —
//! [`save`] is a pure function of a document, and [`load`] returns a document
//! that is **bitwise** equal under this crate's own `PartialEq` or refuses with
//! a [`PersistError`] naming the line. It is **not** the in-game format; that is
//! XML, read by [`parse_sporemodel`], and there is no XML writer here.
//!
//! ## The rule that shapes every field below
//!
//! **Absent is not zero, not empty, and not `false`.** The corpus writes
//! `<childlist>` on 520 of 680 blocks and omits it on the other 160; it omits
//! `<trianglepickorigin>` on exactly the 10 blocks of the one
//! `formatversion 11` document; it contains no `<paintlist>` in any of the 47
//! creature documents. Each of those absences is a fact about the game, so each
//! is an [`Option`] here, and `None` never means "the default".
//!
//! ## Clean-room boundary
//!
//! Nothing here is derived from EA/Maxis code or from game assets. The document
//! grammar is reimplemented from analysis of the container format; no asset byte
//! is committed, and the corpus tests skip cleanly when `SPORE/` is absent.

#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

pub mod claims;
pub mod color;
pub mod document;
pub mod edit;
pub mod error;
pub mod persist;
pub mod sporemodel;
pub mod validate;
pub mod xml;

mod sha256;

pub use color::{ColorError, UnitRgb};
pub use document::{
    Block, BlockId, BlockIndex, BlockTransform, ChannelWeight, CreatorDocument, DocumentProperties,
    Limb, Matrix3, PaintRegion, Vec3,
};
pub use edit::{IndexRemap, OnLastChild, Remap};
pub use error::{
    CreatorError, EditError, EditResult, PersistError, PersistResult, PersistSite, Result,
};
pub use persist::{content_digest, load, load_bytes, save, save_bytes, FLOAT_ENCODING, FORMAT};
pub use sporemodel::{parse_sporemodel, Family};
pub use validate::{validate, Severity, ValidationIssue};
