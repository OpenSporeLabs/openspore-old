//! Spore raster-texture decoding: one framing identity, three formats, RGBA8 mips.
//!
//! Spore's `raster` record — type id [`spore_core::record::type_id::RASTER`] — is
//! the game's texture container. This crate turns one of those records into RGBA8
//! mip images and nothing else: no I/O, no package access, no renderer, no image crate. The
//! contract is deliberately narrow — a decoded mip is `width * height * 4` bytes
//! of RGBA8 in a `Vec<u8>` — and `spore-assets` adapts that to whatever the
//! renderer needs.
//!
//! # Three formats, one framing
//!
//! The corpus's raster records split across three `fourcc` values, and
//! [`decode_raster`] dispatches on it:
//!
//! | `fourcc` | records in `Spore_Content.package` | block size | status |
//! |---|---|---|---|
//! | `0x35545844` `DXT5` | 1 747 | 16 bytes (BC3) | decoded |
//! | `0x00000015` | 901 | 4 bytes per texel | decoded |
//! | `0x31545844` `DXT1` | 306 | 8 bytes (BC1) | decoded |
//!
//! All three share one framing identity, stated and proved in
//! [`framing`]: a header, then the base level unprefixed, then a little-endian
//! `u32` byte count in front of every remaining level, and the record's length is
//! exactly what its own envelope implies. It closes on **2954 of 2954** raster
//! records in `Spore_Content.package` and **1432 of 1432** in
//! `Spore_EP1_Content_01.package`. The full derivation, with the byte counts, is in
//! `docs/RASTER-FRAMING.md`.
//!
//! There are **no layers**. `RasterImage::layers` holds exactly one chain; see
//! [`claims::LAYER_COUNT`] for the refutation of the two-layer reading that
//! preceded it.
//!
//! # The one thing to read before trusting the DXT5 decoder
//!
//! **Spore's `DXT5` records are BC3.** The earlier reading of this crate — 8
//! bytes per block, an inverted palette test, R5G5B5 channel reads, a 6/2 alpha
//! blend and no `alpha0 == alpha1` collapse — is **refuted by the corpus**: a
//! 256² base level occupies 65 536 bytes, which is 4096 blocks of *16*, and every
//! recorded level prefix says so. The `claims` module carries that refutation in
//! full, in [`claims::DXT5_SPEC_DEVIATIONS`], because "this is not what the spec
//! says" was the previous crate's headline and deleting it would leave the next
//! reader nothing to contradict.
//!
//! What that reading *appeared* to establish — "these records carry no colour, only
//! alpha" — was an artefact of it: R5G5B5 on a 6-bit word reads green and blue as
//! zero by construction, because a 6-bit value cannot set bits 10..19. Read as BC3
//! the same records carry 32 distinct greens and 32 distinct blues. The codec is
//! graded [`claims::BC3_CODEC`] as **Inferred**, because no decode of a real record
//! has been compared against the game's own output; that is the test that would
//! raise it.
//!
//! # The `0x15` family
//!
//! The second row is what `docs/LUMINANCE-RASTER.md` is about. The repository's
//! existing vocabulary calls it "luminance"; the bytes say otherwise (it is a
//! tangent-space normal map plus a fourth lane), and the bytes win — see
//! [`claims::LUMINANCE_FOURCC`], which records the correction rather than deleting
//! the old claim.
//!
//! # What is a port of what
//!
//! For `0x15` the decoder is derived from the records and the derivation is
//! written out in `docs/LUMINANCE-RASTER.md`; the Python oracle calls the family
//! "out of scope, hard error here" and `docs/MATERIALS-DESIGN.md` §1 says
//! `field_1c` holds "a float 1.0". Neither is true of the records.
//!
//! For DXT5 and DXT1 the codec is the **published** BC3 and BC1 specifications,
//! reached by a measurement that fixes the block stride and then leaves the
//! standard reading as the only one consistent with the declared fourcc. It is
//! graded `Inferred` accordingly. The earlier 8-byte "Spore block" had the
//! opposite provenance — two implementations agreeing with each other on
//! synthetic input — and the corpus decided the question.
//!
//! Where this port and the references disagree, the corpus wins and the divergence
//! is a claim here rather than a deleted comment. Where the *format* is still not
//! understood, the crate says so: see [`claims`].
//!
//! # No committed fixture
//!
//! Real raster records live only in the git-ignored `SPORE/` tree, so this
//! crate's tests are synthetic and self-contained: hand-computed blocks, a
//! synthetic raster record built byte by byte in a test helper, and a truncation
//! sweep over every prefix length. Nothing here depends on a game installation, a
//! package or a network.
//!
//! # Clean-room
//!
//! Nothing in this crate is derived from EA or Maxis code. The layouts were
//! reconstructed from the record bytes and from the community SDK's type
//! information; `docs/RECON-3.1.0.22.md` grades the underlying claims.

#![forbid(unsafe_code)]
#![warn(missing_docs, missing_debug_implementations)]

pub mod claims;
mod dxt5;
mod error;
pub mod framing;
mod luminance;
mod raster;

use spore_core::record::type_id;

pub use crate::dxt5::{
    block_mip_size, decode_dxt1_mip, decode_dxt5_mip, dxt1_chain_size, dxt1_mip_size,
    dxt5_chain_size, dxt5_mip_size, BC1_BLOCK_BYTES, BLOCK_BYTES, MIP_FLOOR,
};
pub use crate::error::TextureError;
pub use crate::framing::{frame_for, FrameFormat, DXT1_FRAME, DXT5_FRAME, LUMINANCE_FRAME};
pub use crate::luminance::{
    decode_luminance_mip, luminance_chain_size, luminance_dimension, luminance_mip_size,
    luminance_record_size, LUMINANCE_BYTES_PER_TEXEL, LUMINANCE_FOURCC, LUMINANCE_HEADER_SIZE,
    LUMINANCE_PREFIX_SIZE,
};
pub use crate::raster::{
    decode_raster, layer_count_for, mip_dimension, parse_envelope, MipImage, RasterEnvelope,
    RasterImage,
};

/// The DBPF record type id of a raster (texture) record.
///
/// Re-exported from `spore-core` rather than re-spelled, so the type table has
/// exactly one home.
pub const RASTER_TYPE: u32 = type_id::RASTER;

/// Format FOURCC of the BC3-shaped family: `DXT5`.
///
/// Stored little-endian, so the four bytes on disk are `44 58 54 35`.
pub const DXT5_FOURCC: u32 = 0x3554_5844;

/// Format FOURCC of the BC1 family: `DXT1`.
///
/// Stored little-endian, so the four bytes on disk are `44 58 54 31`.
pub const DXT1_FOURCC: u32 = 0x3154_5844;

/// Bytes in the raster envelope of the two block formats: eight little-endian
/// `u32` words.
///
/// The `0x15` family's header is **28** bytes, not 32 — see
/// [`LUMINANCE_HEADER_SIZE`]. This constant names the block formats' length,
/// which is also the minimum [`parse_envelope`] needs to read an envelope of any
/// format; no real record of either header length is shorter than 32 bytes, so
/// that requirement costs nothing.
pub const ENVELOPE_SIZE: usize = 32;

/// Bytes in one **layer header**, which the corpus **refutes**.
///
/// There are none: no raster record in the measured corpus has a 16-byte layer
/// header, and the bytes at `0x20` of a DXT5 record are the first two BC3 blocks
/// of the base level. This constant is retained only so a caller written against
/// the old two-layer reading still compiles; nothing in this crate charges it to
/// a record. See [`claims::LAYER_HEADER_CONTENTS`].
pub const LAYER_HEADER_SIZE: usize = 16;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_type_id_comes_from_spore_core_and_is_the_raster() {
        assert_eq!(RASTER_TYPE, 0x2F4E_681C);
        assert_eq!(
            spore_core::RecordType::new(RASTER_TYPE).name(),
            Some("raster")
        );
    }

    #[test]
    fn the_fourccs_are_dxt5_and_dxt1_stored_little_endian() {
        assert_eq!(DXT5_FOURCC.to_le_bytes(), *b"DXT5");
        assert_eq!(DXT5_FOURCC, 0x3554_5844);
        assert_eq!(DXT1_FOURCC.to_le_bytes(), *b"DXT1");
        assert_eq!(DXT1_FOURCC, 0x3154_5844);
    }

    #[test]
    fn the_three_fourccs_are_distinct_and_all_have_a_frame() {
        // `0x15` is one word long and the other two are printable tags. None may
        // collide, or one record would decode under two rules.
        assert_eq!(LUMINANCE_FOURCC, 0x0000_0015);
        assert_eq!(LUMINANCE_FOURCC.to_le_bytes(), [0x15, 0x00, 0x00, 0x00]);
        for fourcc in [DXT5_FOURCC, DXT1_FOURCC, LUMINANCE_FOURCC] {
            assert!(frame_for(fourcc).is_some(), "{fourcc:#010x} has no frame");
        }
        assert_eq!(frame_for(0x1234_5678), None);
        // The two header lengths are the structural difference between the block
        // formats and the uncompressed family.
        assert_eq!(ENVELOPE_SIZE, 32);
        assert_eq!(LUMINANCE_HEADER_SIZE, 28);
        assert_eq!(ENVELOPE_SIZE - LUMINANCE_HEADER_SIZE, 4);
        // And the two block strides are the whole reason the previous decode was
        // wrong.
        assert_eq!(BLOCK_BYTES, 16);
        assert_eq!(BC1_BLOCK_BYTES, 8);
        assert_eq!(LUMINANCE_BYTES_PER_TEXEL, 4);
    }

    #[test]
    fn the_block_and_header_sizes_are_the_documented_ones() {
        assert_eq!(ENVELOPE_SIZE, 32);
        assert_eq!(LAYER_HEADER_SIZE, 16);
        assert_eq!(BLOCK_BYTES, 16);
        assert_eq!(BC1_BLOCK_BYTES, 8);
        assert_eq!(ENVELOPE_SIZE, 8 * 4, "eight little-endian u32 words");
    }
}
