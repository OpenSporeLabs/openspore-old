//! Spore raster-texture decoding: the 32-byte DXT5 envelope and the `0x15`
//! family's own 28-byte header, with both block/uncompressed payloads resolved to
//! RGBA8 mip images.
//!
//! Spore's `raster` record — type id [`spore_core::record::type_id::RASTER`] — is
//! the game's texture container. This crate turns one of those records into RGBA8
//! mip images and nothing else: no I/O, no package access, no renderer, no image crate. The
//! contract is deliberately narrow — a decoded mip is `width * height * 4` bytes
//! of RGBA8 in a `Vec<u8>` — and `spore-assets` adapts that to whatever the
//! renderer needs.
//!
//! # Two formats, one record type
//!
//! The corpus's 2 954 raster records split across three `fourcc` values, and
//! [`decode_raster`] dispatches on it:
//!
//! | `fourcc` | records in `Spore_Content.package` | status |
//! |---|---|---|
//! | `0x35545844` `DXT5` | 1 747 | decoded |
//! | `0x00000015` | 901 | decoded — uncompressed, `Z, X, Y, mask` per texel |
//! | `0x31545844` `DXT1` | 306 | **refused by name** — its framing is measured not to fit either rule |
//!
//! The `0x15` family is the second row and is what `docs/LUMINANCE-RASTER.md` is
//! about. The repository's existing vocabulary calls it "luminance"; the bytes
//! say otherwise (it is a tangent-space normal map plus a fourth lane), and the
//! bytes win — see [`claims::LUMINANCE_FOURCC`], which records the correction
//! rather than deleting the old claim.
//!
//! # What this crate is a port of
//!
//! For DXT5, four sources describe the same bytes, all in this repository and
//! all in agreement:
//!
//! * `tools/spore/dxt5/dxt5.py` and `tools/spore/raster/raster.py` — stdlib-only
//!   Python oracles, independent implementations;
//! * `src/assets/Dxt5.{cpp,hpp}` and `src/assets/Texture.{cpp,hpp}` — the C++
//!   reference, whose header states it "must match [the Python oracle]
//!   byte-for-byte";
//! * `docs/MATERIALS-DESIGN.md` §1 — the envelope byte-pinned on real records;
//! * `tests/test_textures.py::TestDxt5Synthetic` — hand-computed expected RGBA
//!   values for synthetic blocks, reused verbatim by `tests/dxt5_golden.rs`.
//!
//! For `0x15` the sources **disagreed with the bytes** and were corrected: the
//! Python oracle calls the family "out of scope, hard error here" and
//! `docs/MATERIALS-DESIGN.md` §1 says `field_1c` holds "a float 1.0". Neither is
//! true of the records. The decoder is derived from the records and the derivation
//! is written out in `docs/LUMINANCE-RASTER.md`.
//!
//! Where this port and the references disagree, the references win and the
//! divergence is a bug here. Where the *format* is not understood, the crate
//! says so rather than filling the gap: see [`claims`].
//!
//! # The one thing to read before trusting the DXT5 decoder
//!
//! **Spore's blocks are shaped like BC3/DXT5 and are not decoded like BC3/DXT5.**
//! Four documented departures from the published specification are reproduced
//! on purpose — an inverted palette-size test, R5G5B5 channel reads, a 6/2 blend
//! where the spec puts the endpoints, and no `alpha0 == alpha1` collapse — and
//! the `dxt5` module's own source documentation explains each one with the
//! evidence that fixes it. The visible consequence is that every decoded texel
//! has **red in `{0, 8}` and green/blue of 0**: the black/red alpha-mask signature
//! of Spore's own texture records. A decoder that "corrected" the codec to the published
//! spec would change every pixel the game draws.
//!
//! The `0x15` family has no such trap, and that is a *finding* rather than an
//! absence of one: it is uncompressed, so there is nothing to get subtly wrong,
//! and what it contains is stated in [`luminance`] with its evidence grade.
//!
//! # No committed fixture
//!
//! Real raster records live only in the git-ignored `SPORE/` tree, so this
//! crate's tests are synthetic and self-contained: hand-computed blocks, a
//! synthetic raster record built byte by byte in a test helper, and a
//! truncation sweep over every prefix length of both. Nothing here depends on a
//! game installation, a package or a network.
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
mod luminance;
mod raster;

use spore_core::record::type_id;

pub use crate::dxt5::{decode_dxt5_mip, dxt5_chain_size, dxt5_mip_size, BLOCK_BYTES, MIP_FLOOR};
pub use crate::error::TextureError;
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

/// Bytes in the **DXT5** raster envelope: eight little-endian `u32` words.
///
/// The `0x15` family's header is **28** bytes, not 32 — see
/// [`LUMINANCE_HEADER_SIZE`]. This constant names the DXT5 length, which is what
/// [`parse_envelope`] still requires; no real record of either format is
/// shorter than 32 bytes, so that requirement costs nothing.
pub const ENVELOPE_SIZE: usize = 32;

/// Bytes in one **DXT5** layer header, which precedes every layer payload and is
/// **skipped, not interpreted** — see the `raster` module's documentation in the
/// source and [`claims::LAYER_HEADER_CONTENTS`].
///
/// The `0x15` family has no layer headers at all, which is one of the two ways
/// it differs structurally from DXT5.
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
    fn the_fourcc_is_dxt5_stored_little_endian() {
        assert_eq!(DXT5_FOURCC.to_le_bytes(), *b"DXT5");
        assert_eq!(DXT5_FOURCC, 0x3554_5844);
    }

    #[test]
    fn the_luminance_fourcc_is_a_distinct_format_not_a_dxt5_spelling() {
        // `0x15` is one word long and the other three are zero. It must not
        // collide with `DXT5` or with the `DXT1` records that are present in the
        // corpus and deliberately not decoded.
        assert_eq!(LUMINANCE_FOURCC, 0x0000_0015);
        assert_eq!(LUMINANCE_FOURCC.to_le_bytes(), [0x15, 0x00, 0x00, 0x00]);
        assert_ne!(LUMINANCE_FOURCC, DXT5_FOURCC);
        assert_ne!(LUMINANCE_FOURCC, 0x3154_5844, "DXT1 is refused by name");
        // The two headers are different lengths, which is the structural
        // difference that makes them two formats rather than one.
        assert_eq!(ENVELOPE_SIZE, 32);
        assert_eq!(LUMINANCE_HEADER_SIZE, 28);
        assert_eq!(ENVELOPE_SIZE - LUMINANCE_HEADER_SIZE, 4);
        // Four bytes per texel, and the same four the output carries.
        assert_eq!(LUMINANCE_BYTES_PER_TEXEL, 4);
        assert_eq!(LUMINANCE_PREFIX_SIZE, 4);
    }

    #[test]
    fn the_block_and_header_sizes_are_the_documented_ones() {
        assert_eq!(ENVELOPE_SIZE, 32);
        assert_eq!(LAYER_HEADER_SIZE, 16);
        assert_eq!(BLOCK_BYTES, 8);
        assert_eq!(ENVELOPE_SIZE, 8 * 4, "eight little-endian u32 words");
    }
}
