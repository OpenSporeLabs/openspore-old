//! GMDL ("GameModel") decoder and static-mesh extractor.
//!
//! Spore's `gmdl` record — type id `0x00E6BCE5` — is its primary geometry
//! container: bounding box, index buffers, interleaved vertex buffers, mesh
//! references, materials and a shader/texture material-info section, wrapped in
//! an animation trailer. This crate decodes that record and extracts static
//! meshes from it. It is on the vertical slice's critical path
//! (DBPF record → gmdl → mesh → renderer) and is deliberately free of any
//! engine, GPU or I/O dependency so that `spore-assets` can adapt its output to
//! whatever renderer is in use.
//!
//! # What this crate is a port of
//!
//! Four sources describe the same layout, all in this repository and
//! cross-checked against each other:
//!
//! * `src/assets/Gmdl.cpp` / `Gmdl.hpp` — the C++ reference walk;
//! * `src/assets/Mesh.cpp` / `Mesh.hpp` — the C++ mesh extraction and bounds;
//! * `tools/spore/gmdl/gmdl.py` — a stdlib-only Python oracle, an independent
//!   decode of the same bytes;
//! * `docs/ASSET-PATH.md` §"Stage-by-stage decode" — the graded write-up of
//!   that path, whose evidence table this crate preserves.
//!
//! Where this port and the references disagree, the references win and the
//! divergence is a bug in this crate. Where the *format* is not understood, the
//! crate says so rather than filling the gap — see [`claims`], and the
//! documentation of [`mesh_from_gmdl`], which opens with that same warning about
//! the `UBYTE4` normal encoding.
//!
//! # The one thing to read before trusting this decoder
//!
//! **The gmdl `refCount` table is big-endian — the count word *and* all three
//! words of every `{instance, group, type}` key — while every other word in the
//! record is little-endian.** Two independent bugs live there. A little-endian
//! count multiplies it by `0x01000000` and walks the key table straight off the
//! end of the record; that was the most expensive bug in the format's history,
//! breaking 1 510 of the 4 209 gmdl records in `Spore_Content.package`. A
//! big-endian count with little-endian *keys* is worse in one respect and better
//! in another: the walk lands in bounds and yields
//! [`GmdlModel::referenced_files`] that address nothing — 0 of 7 989 resolve
//! against the real DBPF index, where the big-endian reading resolves 7 989.
//! Both are spelled out at the call site, kept as a separate reader method, and
//! pinned by `tests/refcount_be.rs` (synthetic) and `tests/corpus_animation.rs`
//! (real index resolution, `#[ignore]`d). `docs/ANIMATION.md` §2 has the
//! measurement.
//!
//! # Strict, then best-effort
//!
//! The walk from the version word through the material info is strict: a short
//! buffer is an error naming the section. The trailer after it (bone ranges,
//! anim data, baked deforms, trailing key) is best-effort and cannot fail a
//! parse, because real record families exist whose trailer framing this walk
//! does not know. That boundary is reported rather than hidden:
//! [`GmdlModel::strict_consumed`] is how far the walk actually read,
//! [`GmdlModel::consumed`] is always the input length, and
//! [`GmdlModel::trailer`] says whether the tail was reached at all.
//!
//! # Clean-room
//!
//! Nothing here is derived from EA or Maxis code. The layout was reconstructed
//! from the record bytes and the community SDK's type information, which
//! `docs/RECON-3.1.0.22.md` appendix A grades per claim.

#![forbid(unsafe_code)]
#![warn(missing_docs, missing_debug_implementations)]

pub mod claims;

mod error;
mod mesh;
mod parse;
mod reader;
mod skin;
mod tables;

pub use claims::{all_claims, claim, vertex_normal_encoding_level, GmdlClaim};
pub use error::GmdlError;
pub use mesh::{compute_mesh_bounds, mesh_from_gmdl, Mesh, Topology};
pub use parse::{
    parse, parse_recovering, GmdlIndexBuffer, GmdlMeshRef, GmdlModel, GmdlTextureRef,
    GmdlVertexBuffer, GmdlVertexElement, PartialGmdl, StopReason, TrailerStage, TrailerWalk,
    WalkStage,
};
pub use skin::GmdlSkinBinding;
pub use tables::{
    shader_data_size, vertex_stride, DeclType, DeclUsage, ShaderDataSize, PRIM_TRIANGLE_LIST,
    SHADER_DATA_SIZES, TEXTURE_SET_ID,
};

/// The record type id of a gmdl ("GameModel") record.
///
/// From `spore_core::record::type_id::GMDL`, which is transcribed from
/// `tools/spore/types/typenames.json`. Spelled as a constant here so a caller
/// that looks up records by type never re-derives the number.
pub const GMDL_TYPE: u32 = spore_core::record::type_id::GMDL;

/// The only gmdl version this crate decodes.
///
/// Version 9 changes the material-info framing — its per-material block carries
/// no entry-count word — and no version-9 record has been validated against this
/// walk, so it is refused by name ([`GmdlError::UnsupportedVersion`]) instead of
/// attempted. Every record this build has decoded is version 8.
pub const SUPPORTED_VERSION: u32 = 8;
