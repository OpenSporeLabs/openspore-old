//! Typed failures of material and texture resolution.
//!
//! One variant per failure mode, because the three things that can go wrong
//! here call for three different responses and an operator cannot tell them
//! apart from one string:
//!
//! * the record is **not in the store** — a missing asset, or a wrong assumed
//!   type, and the near-miss candidates `spore-assets` already collected;
//! * the record **is there but this crate will not decode it as a texture** —
//!   `png`- and `rw4`-typed records are real records that are not raster
//!   records, and conflating them is precisely the bug this type prevents;
//! * the record **is a raster of a format this build has no frame for** — a
//!   `fourcc` outside the three the corpus contains. All three known raster
//!   formats decode, so this is a genuinely open case rather than one of the
//!   families below.
//!
//! Each variant names the [`ResourceKey`] it is about, so "while loading X" can
//! be logged without matching every arm. [`MaterialError::key`] does that.
//!
//! # PartialEq, not Eq
//!
//! [`spore_gmdl::GmdlError`] carries `f32`s and derives `PartialEq` only
//! (`NaN != NaN` is the point there), so this type cannot derive `Eq`. Tests in
//! this workspace compare asset errors far more often than they hash them.

use spore_assets::AssetError;
use spore_core::ResourceKey;
use spore_gmdl::{GmdlError, GmdlTextureRef};
use spore_texture::TextureError;
use thiserror::Error;

use spore_texture::RASTER_TYPE;

/// Every way resolving a material or a texture reference can fail.
///
/// A resolver never publishes a partially decoded texture: an `Err` means
/// nothing was returned, and in particular it never means "an empty image".
/// `Clone` is deliberately absent: `spore_assets::AssetError` holds an
/// `std::io::Error` in one variant and does not implement it, and duplicating
/// that error is not worth a `#[derive]` this crate would otherwise not need.
#[derive(Debug, PartialEq, Error)]
pub enum MaterialError {
    /// The store could not hand back the referenced record.
    ///
    /// `source` carries the near-miss candidates `spore-assets` collected, so
    /// the common cause — a wrong `assumed_type`, which changes the type word
    /// of the key and therefore misses every row — is diagnosable from the
    /// error alone.
    #[error("material: no record `{key}` ({source})")]
    Lookup {
        /// The identity the lookup actually asked for, with the assumed type
        /// word already substituted in.
        key: ResourceKey,
        /// The store's own failure.
        #[source]
        source: AssetError,
    },

    /// The record exists under the assumed identity, but its type is not one
    /// this crate decodes as a texture.
    ///
    /// **This is the `png` vs `rw4` vs `raster` boundary.** A record whose type
    /// id is `0x2F7D0004` (`png`) is *raw PNG* — verified on real records,
    /// which begin `89 50 4E 47` — and a record typed `0x2F4E681B` (`rw4`) is a
    /// RenderWare container beginning `52 57 34 77` ("RW4w32"). Neither is a
    /// raster record, and neither decodes as one. This crate therefore refuses
    /// both **by type id** instead of handing them to the raster decoder and
    /// reporting plausible garbage.
    #[error(
        "material: record `{key}` was found under the assumed type 0x{assumed_type:08x} \
         ({name}), which this crate does not decode as a texture; \
         raster texture records are 0x{RASTER_TYPE:08x} (raster)"
    )]
    AssumedTypeNotDecodable {
        /// The identity that resolved.
        key: ResourceKey,
        /// The type word the caller assumed.
        assumed_type: u32,
        /// The canonical name of that type word, or its hex form when
        /// `spore-core` has no name for it.
        name: String,
    },

    /// The record is a raster, and its `fourcc` is a format this build does not
    /// decode.
    ///
    /// **Three** raster formats exist in the measured corpus and all three are
    /// decoded by `spore-texture`: `DXT5` (BC3), `DXT1` (BC1) and `0x15`. So this
    /// variant is for a record whose format word is *outside* the known set, and
    /// the message says so with the three that are known rather than naming one.
    ///
    /// It used to name `0x15` as the out-of-scope family. That was true when the
    /// `0x15` family was refused, and the corpus refuted that: 901 of 901 records
    /// decode. A message that calls a decodable format out of scope sends a
    /// caller looking for a bug that is not there.
    #[error(
        "material: record `{key}` is a raster with fourcc 0x{fourcc:08x}, which this \
         build has no frame for; the known raster formats are 0x35545844 = 'DXT5' (BC3), \
         0x31545844 = 'DXT1' (BC1) and 0x00000015 (the uncompressed normal-map family, \
         all three decoded)"
    )]
    UnsupportedFourcc {
        /// The identity of the refused record.
        key: ResourceKey,
        /// The `fourcc` word read from the 32-byte envelope.
        fourcc: u32,
    },

    /// The raster record was reached but refused by the texture layer.
    ///
    /// Truncation, a zero dimension, a zero mip count, a `field_18` that is not
    /// the base level's own byte count, a record length that is not the one its
    /// envelope implies, a level length prefix that disagrees with its level's
    /// geometry, and every block-codec refusal all land here, with `source`
    /// naming which.
    ///
    /// The old list mentioned "a payload that does not divide by the layer
    /// stride", which no longer exists: there are no layers, so there is no
    /// stride to divide by. `TextureError::PayloadNotMultiple` is gone from
    /// `spore-texture` with it.
    #[error("material: record `{key}`: texture: {source}")]
    Decode {
        /// The identity of the refused record.
        key: ResourceKey,
        /// The texture layer's own failure.
        #[source]
        source: TextureError,
    },

    /// The gmdl record was reached but refused by the gmdl layer.
    ///
    /// Relevant to this crate because the material info lives inside it: a
    /// shader-data id with no documented size (the table has gaps, `0x218`
    /// among them) stops the walk before any texture reference is read, so no
    /// texture list exists to resolve at all.
    #[error("material: record `{key}`: gmdl: {source}")]
    ModelDecode {
        /// The identity of the refused model record.
        key: ResourceKey,
        /// The gmdl layer's own failure.
        #[source]
        source: GmdlError,
    },

    /// The reference carries a [`spore_core::WILDCARD`] component, so it does not
    /// identify one record.
    ///
    /// A wildcard is a legitimate thing for a *reference* to contain — "some
    /// instance of this type" — but resolving it to a single image would mean
    /// silently picking the first row that matched. That choice belongs to the
    /// caller, so it is refused here.
    #[error(
        "material: texture reference (instance 0x{instance:08x}, group 0x{group:08x}) carries \
         a wildcard ({component} is 0xFFFFFFFF), so it names no single record",
        instance = .reference.instance_id,
        group = .reference.group_id
    )]
    IncompleteReference {
        /// The offending reference, as the gmdl record carried it.
        reference: GmdlTextureRef,
        /// Which component was the wildcard: `"instance"` or `"group"`.
        component: &'static str,
    },

    /// The record decoded as a raster that holds no base image.
    ///
    /// **Defensive.** `spore_texture::decode_raster` rejects a zero mip count
    /// and derives a layer count of at least one, so a decode cannot currently
    /// produce this. The variant exists so that a future change to that
    /// invariant degrades into a typed error instead of an index-out-of-bounds
    /// panic or a silently blank texture. Its message is pinned by
    /// `tests/errors.rs`.
    #[error("material: record `{key}` decoded to a raster with no base image (no layers, or an empty first layer)")]
    EmptyImage {
        /// The identity whose decode yielded no base image.
        key: ResourceKey,
    },
}

impl MaterialError {
    /// The record identity this error is about, when it is about one.
    ///
    /// [`Self::IncompleteReference`] has no single identity by construction —
    /// that is the variant's whole point — so it answers `None`.
    pub fn key(&self) -> Option<ResourceKey> {
        match self {
            Self::Lookup { key, .. }
            | Self::AssumedTypeNotDecodable { key, .. }
            | Self::UnsupportedFourcc { key, .. }
            | Self::Decode { key, .. }
            | Self::ModelDecode { key, .. }
            | Self::EmptyImage { key } => Some(*key),
            Self::IncompleteReference { .. } => None,
        }
    }

    /// Whether the failure is about a *format this build does not decode*
    /// rather than about a record that is broken or absent.
    ///
    /// A caller that wants "skip this asset, it is not our format" rather than
    /// "this asset is corrupt" matches on this instead of on two variants.
    pub fn is_unsupported_format(&self) -> bool {
        matches!(
            self,
            Self::AssumedTypeNotDecodable { .. } | Self::UnsupportedFourcc { .. }
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use spore_texture::{DXT1_FOURCC, DXT5_FOURCC};

    const KEY: ResourceKey = ResourceKey::new(RASTER_TYPE, 0x4063_2900, 0x067A_07F0);

    #[test]
    fn every_variant_names_its_record_or_says_why_it_cannot() {
        let missing = MaterialError::Lookup {
            key: KEY,
            source: AssetError::EmptyStore,
        };
        assert_eq!(missing.key(), Some(KEY));
        assert!(!missing.is_unsupported_format());

        let wrong_type = MaterialError::AssumedTypeNotDecodable {
            key: KEY,
            assumed_type: 0x2F7D_0004,
            name: String::from("png"),
        };
        assert_eq!(wrong_type.key(), Some(KEY));
        assert!(wrong_type.is_unsupported_format());

        // A fourcc outside the three the corpus contains. It is deliberately NOT
        // `0x00000015`: that family decodes, so naming it here would be the
        // refuted premise this crate stopped asserting.
        let fourcc = MaterialError::UnsupportedFourcc {
            key: KEY,
            fourcc: 0x1234_5678,
        };
        assert_eq!(fourcc.key(), Some(KEY));
        assert!(fourcc.is_unsupported_format());

        let decode = MaterialError::Decode {
            key: KEY,
            source: TextureError::TruncatedEnvelope { available: 4 },
        };
        assert_eq!(decode.key(), Some(KEY));
        assert!(!decode.is_unsupported_format());

        let model = MaterialError::ModelDecode {
            key: KEY,
            source: GmdlError::EmptyInput,
        };
        assert_eq!(model.key(), Some(KEY));
        assert!(!model.is_unsupported_format());

        let empty = MaterialError::EmptyImage { key: KEY };
        assert_eq!(empty.key(), Some(KEY));
        assert!(!empty.is_unsupported_format());

        let wildcard = MaterialError::IncompleteReference {
            reference: GmdlTextureRef {
                instance_id: spore_core::WILDCARD,
                group_id: 0x4063_2900,
            },
            component: "instance",
        };
        assert_eq!(
            wildcard.key(),
            None,
            "a wildcard reference identifies no record, so there is no key to report"
        );
        assert!(!wildcard.is_unsupported_format());
    }

    #[test]
    fn a_refused_fourcc_names_the_word_and_every_format_this_build_does_read() {
        // The message used to name `DXT5` as the only wanted word and call the
        // `0x15` family "out of scope". Both were refuted: the `0x15` family
        // decodes (901/901 records) and `DXT1` decodes too. A refusal message
        // that misreports what IS supported sends a caller hunting a bug that is
        // not there, so all three are named and the test pins them.
        let text = MaterialError::UnsupportedFourcc {
            key: KEY,
            fourcc: 0x1234_5678,
        }
        .to_string();
        assert!(text.contains("0x12345678"), "got {text:?}");
        for (fourcc, spelling) in [
            (DXT5_FOURCC, "0x35545844"),
            (DXT1_FOURCC, "0x31545844"),
            (spore_texture::LUMINANCE_FOURCC, "0x00000015"),
        ] {
            assert!(
                text.contains(spelling),
                "the message must name {spelling} (0x{fourcc:08x}) as a format this build \
                 reads; got {text:?}"
            );
        }
        assert!(
            text.contains("all three decoded"),
            "and must say they are decoded rather than merely known; got {text:?}"
        );
    }
}
