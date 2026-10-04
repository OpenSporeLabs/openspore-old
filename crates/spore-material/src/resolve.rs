//! Turning a gmdl record's texture references into images, and reporting what
//! had to be assumed to do it.
//!
//! # The three steps, and what each one costs in honesty
//!
//! 1. **A `GmdlTextureRef` is `{instance, group}`.** No type word. So the lookup
//!    key is completed with the caller's [`assumed_type`](resolve_texture)
//!    parameter, and
//!    [`claims::ASSUMED_RECORD_TYPE`] records that the type was **assumed**, at
//!    [`spore_core::evidence::EvidenceLevel::Inferred`].
//! 2. **The record is read and decoded as a raster.** A type word that names
//!    something other than [`spore_texture::RASTER_TYPE`] is refused *by type*,
//!    even when the record exists there: `png`-typed records are raw PNG and
//!    `rw4`-typed records are RenderWare containers, and neither is a raster.
//! 3. **The base image is published**, together with the mip and layer counts and
//!    the claim set. A record with more than one layer, or with a mip chain, is
//!    not flattened into a guess about which layer is which role.
//!
//! # Failures are per reference, never per batch
//!
//! [`resolve_model_textures`] returns one `Result` per reference, in encounter
//! order, and a failing reference never removes a succeeding one. That is not a
//! convenience: it is what the documented asset requires. `osptool describe` on
//! `0x00e6bce5:0x40637e03:0x067a07f0` (2026-10-04) reports three texture
//! references, and `osptool describe` on
//! `0x2f4e681c:0x40632902:0x067a07f0` refuses it with
//! `unsupported fourcc 0x00000015`. A resolver that stopped at the first failure
//! would return **no textures at all** for the one asset this repository
//! documents end to end.

use spore_assets::ContentStore;
use spore_core::evidence::Fact;
use spore_core::record::{group_name, RecordType};
use spore_core::ResourceKey;
use spore_gmdl::{GmdlModel, GmdlTextureRef, GMDL_TYPE};
use spore_texture::{
    decode_raster, parse_envelope, MipImage, RasterEnvelope, RasterImage, TextureError,
    DXT5_FOURCC, RASTER_TYPE,
};

use crate::claims;
use crate::error::MaterialError;
use crate::material::{model_materials, MaterialModel};

/// One resolved texture: bytes, provenance and the boundary of what is known.
///
/// `image` is the **base mip of the first layer**, cloned out of the decode.
/// Which layer a renderer should use is not a question this build can answer
/// (see [`crate::material::BindingScope`]), so only one image is published and
/// the counts that describe the rest are carried beside it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedTexture {
    /// The identity actually used for the lookup, with the assumed type word
    /// substituted in. Not a key the gmdl record carried.
    pub key: ResourceKey,
    /// Which record family this decoded as.
    pub kind: TextureKind,
    /// The base mip (mip 0) of layer 0, RGBA8 row-major.
    ///
    /// **Copy cost**: this is a clone, because the decode owns the whole mip
    /// chain. For a 512² texture that is 1 MiB per resolved texture, which is
    /// the price of a self-contained value. A caller that wants the whole chain
    /// should call [`spore_texture::decode_raster`] on
    /// [`ContentStore::read`]'s bytes itself.
    pub image: MipImage,
    /// The envelope the decode ran under, verbatim.
    pub envelope: RasterEnvelope,
    /// Mip levels per layer, from the envelope. Equals `envelope.mip_count`.
    pub mip_count: usize,
    /// Layers in the record. **Derived** from the record size, never read from a
    /// header field — see [`claims::LAYER_COUNT_IS_DERIVED`]. `0` from
    /// [`inspect_envelope`], which never derives it.
    pub layer_count: usize,
    /// Whether [`Self::image`] holds decoded texels. See
    /// [`Self::pixels_decoded`].
    pub pixels_decoded: bool,
    /// Every graded statement about this resolution, including the assumption
    /// about the record type.
    pub claims: Vec<TextureClaim>,
}

/// One graded statement about a resolved texture.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextureClaim {
    /// The stable subject string; see the [`claims`] module for the full list.
    pub subject: String,
    /// The statement, with its grade and provenance — or a non-finding.
    pub fact: Fact<&'static str>,
}

impl TextureClaim {
    /// Builds a claim from one of the crate's declared subjects.
    pub fn new(subject: &'static str, fact: Fact<&'static str>) -> Self {
        Self {
            subject: subject.to_owned(),
            fact,
        }
    }

    /// Looks a claim up by subject.
    pub fn claim<'a>(claims: &'a [TextureClaim], subject: &str) -> Option<&'a TextureClaim> {
        claims.iter().find(|entry| entry.subject == subject)
    }
}

/// Which kind of record a reference resolved to.
///
/// One variant, because one decoder. A second variant would be a promise that
/// some other record family decodes here, and adding it before the decoder
/// exists is how `png`-and-`rw4`-are-not-rasters gets forgotten.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TextureKind {
    /// A `raster` record ([`spore_texture::RASTER_TYPE`]): a 32-byte envelope and
    /// DXT5 layers.
    Raster,
}

impl TextureKind {
    /// The type id this kind is stored under.
    pub const fn type_id(self) -> u32 {
        match self {
            Self::Raster => RASTER_TYPE,
        }
    }

    /// The canonical spelling, for logs and JSON.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Raster => "raster",
        }
    }
}

impl ResolvedTexture {
    /// Looks a claim up by subject.
    pub fn claim(&self, subject: &str) -> Option<&TextureClaim> {
        TextureClaim::claim(&self.claims, subject)
    }

    /// The claims that are non-findings, i.e. the things this build does not
    /// know about this texture.
    pub fn unknowns(&self) -> impl Iterator<Item = &TextureClaim> {
        self.claims
            .iter()
            .filter(|claim| claim.fact.is_unavailable())
    }

    /// Bytes of RGBA8 in the published image, `width * height * 4`.
    pub fn pixel_len(&self) -> usize {
        self.image.pixels.len()
    }

    /// Whether [`Self::image`] holds decoded texels.
    ///
    /// `false` only ever comes from [`inspect_envelope`], which reads a record's
    /// header and stops there. It exists because "an empty image" is otherwise
    /// ambiguous between *no pixels were asked for* and *the decode produced
    /// nothing*, and this crate's whole purpose is to keep those two apart: a
    /// refusal never returns an empty image, and a header read never pretends to
    /// be a decode.
    pub fn pixels_decoded(&self) -> bool {
        self.pixels_decoded
    }
}

/// Every texture reference of a model, resolved in encounter order, plus the
/// materials those references could belong to.
///
/// `PartialEq` but not `Clone`/`Eq`: the `Vec` of results holds a
/// [`MaterialError`], which holds an `AssetError` and is therefore neither.
#[derive(Debug, PartialEq)]
pub struct ModelTextures {
    /// The model record's own identity.
    pub key: ResourceKey,
    /// The decoded gmdl record the references came from.
    pub model: GmdlModel,
    /// One result per [`GmdlTextureRef`], in the order the record listed them.
    pub textures: Vec<Result<ResolvedTexture, MaterialError>>,
    /// One entry per material id the record declared, in record order.
    pub materials: Vec<MaterialModel>,
}

impl ModelTextures {
    /// How many references resolved into an image.
    pub fn resolved_count(&self) -> usize {
        self.textures.iter().filter(|entry| entry.is_ok()).count()
    }

    /// The failures, in encounter order, with the reference each belongs to.
    pub fn failures(&self) -> Vec<(GmdlTextureRef, &MaterialError)> {
        self.textures
            .iter()
            .zip(self.model.texture_refs.iter())
            .filter_map(|(result, reference)| match result {
                Ok(_) => None,
                Err(error) => Some((*reference, error)),
            })
            .collect()
    }
}

/// Resolves one texture reference into an image.
///
/// `assumed_type` is the **type word the reference does not carry**, supplied by
/// the caller and recorded as an assumption rather than buried. It must be
/// [`spore_texture::RASTER_TYPE`] for the resolve to succeed: a `png`- or
/// `rw4`-typed record is a different container and is refused by name.
///
/// # Errors
///
/// [`MaterialError::IncompleteReference`] for a wildcard component,
/// [`MaterialError::Lookup`] when the store has no such identity,
/// [`MaterialError::AssumedTypeNotDecodable`] when the identity exists but its
/// type is not a raster, [`MaterialError::UnsupportedFourcc`] for a raster of a
/// format this build does not decode, and [`MaterialError::Decode`] for every
/// other texture-layer refusal.
pub fn resolve_texture(
    store: &ContentStore,
    reference: GmdlTextureRef,
    assumed_type: u32,
) -> Result<ResolvedTexture, MaterialError> {
    let key = reference_key(reference, assumed_type)?;
    let record = store
        .read(&key)
        .map_err(|source| MaterialError::Lookup { key, source })?;

    // Checked *after* the read, deliberately. "There is no record at the key you
    // asked for" and "there is a record there and it is not a texture" are
    // different findings, and the first one is the usual consequence of guessing
    // the type wrong — so it must reach the caller as a miss, with the near-miss
    // candidates the store collected.
    if assumed_type != RASTER_TYPE {
        return Err(MaterialError::AssumedTypeNotDecodable {
            key,
            assumed_type,
            name: type_display(assumed_type),
        });
    }

    let image = decode_raster(&record).map_err(|source| map_texture_error(key, source))?;
    build_resolved(key, assumed_type, image)
}

/// Reads a raster record's 32-byte envelope without decoding its pixels.
///
/// The cheap question — "how big is this, and is it a format we read?" — asked of
/// an asset atlas that may hold thousands of records. It carries the same claim
/// set as [`resolve_texture`], minus the ones that need pixels.
pub fn inspect_envelope(
    store: &ContentStore,
    reference: GmdlTextureRef,
    assumed_type: u32,
) -> Result<ResolvedTexture, MaterialError> {
    let key = reference_key(reference, assumed_type)?;
    let record = store
        .read(&key)
        .map_err(|source| MaterialError::Lookup { key, source })?;
    if assumed_type != RASTER_TYPE {
        return Err(MaterialError::AssumedTypeNotDecodable {
            key,
            assumed_type,
            name: type_display(assumed_type),
        });
    }
    let envelope =
        parse_envelope(&record).map_err(|source| MaterialError::Decode { key, source })?;
    let mut facts = header_claims(&key, assumed_type);
    facts.push(TextureClaim::new(
        claims::LAYER_COUNT_IS_DERIVED,
        claims::graded(
            claims::LAYER_COUNT_IS_DERIVED,
            claims::PROV_MATERIALS_DESIGN,
            LAYER_COUNT_TEXT,
        ),
    ));
    Ok(ResolvedTexture {
        key,
        kind: TextureKind::Raster,
        // No pixels were decoded, so the image is empty — and that is stated
        // rather than implied: `pixel_len()` is 0 and no claim says a texel was
        // observed. The alternative, refusing to answer the cheap question, would
        // be a worse lie.
        image: MipImage {
            width: envelope.width,
            height: envelope.height,
            pixels: Vec::new(),
        },
        envelope,
        mip_count: envelope.mip_count as usize,
        layer_count: 0,
        pixels_decoded: false,
        claims: facts,
    })
}

/// Resolves every texture a model referenced, in encounter order.
///
/// One `Result` per [`GmdlModel::texture_refs`] entry, positionally aligned with
/// it, and a failure never removes a later success. See the module
/// documentation for why the documented asset needs exactly this behaviour.
pub fn resolve_model_textures(
    store: &ContentStore,
    model: &GmdlModel,
    assumed_type: u32,
) -> Vec<Result<ResolvedTexture, MaterialError>> {
    model
        .texture_refs
        .iter()
        .map(|reference| resolve_texture(store, *reference, assumed_type))
        .collect()
}

/// Reads a gmdl record from the store, decodes it, and resolves its textures.
///
/// The gmdl layer's refusals surface as [`MaterialError::ModelDecode`]. That
/// matters for materials specifically: the texture references live *inside* the
/// material-info section, so a shader-data id with no documented size stops the
/// walk before any reference exists to resolve.
pub fn resolve_model_record(
    store: &ContentStore,
    key: &ResourceKey,
    assumed_type: u32,
) -> Result<ModelTextures, MaterialError> {
    if key.type_id != GMDL_TYPE {
        return Err(MaterialError::AssumedTypeNotDecodable {
            key: *key,
            assumed_type: key.type_id,
            name: type_display(key.type_id),
        });
    }
    let record = store
        .read(key)
        .map_err(|source| MaterialError::Lookup { key: *key, source })?;
    let model = spore_gmdl::parse(&record)
        .map_err(|source| MaterialError::ModelDecode { key: *key, source })?;
    Ok(ModelTextures {
        key: *key,
        textures: resolve_model_textures(store, &model, assumed_type),
        materials: model_materials(&model),
        model,
    })
}

/// Completes a reference into a lookup key, refusing a wildcard component.
fn reference_key(
    reference: GmdlTextureRef,
    assumed_type: u32,
) -> Result<ResourceKey, MaterialError> {
    // Order is instance then group, matching the order the two checks read in,
    // and matching the order `ResourceKey` declares its fields after the type.
    if reference.instance_id == spore_core::WILDCARD {
        return Err(MaterialError::IncompleteReference {
            reference,
            component: "instance",
        });
    }
    if reference.group_id == spore_core::WILDCARD {
        return Err(MaterialError::IncompleteReference {
            reference,
            component: "group",
        });
    }
    Ok(ResourceKey::new(
        assumed_type,
        reference.group_id,
        reference.instance_id,
    ))
}

/// Maps a texture-layer refusal onto this crate's variants.
///
/// `UnsupportedFourcc` is split out because it is not a broken record: it is a
/// format this build does not decode, and a caller wants to skip the asset
/// rather than report corruption.
fn map_texture_error(key: ResourceKey, source: TextureError) -> MaterialError {
    match source {
        TextureError::UnsupportedFourcc { fourcc } => {
            MaterialError::UnsupportedFourcc { key, fourcc }
        }
        other => MaterialError::Decode { key, source: other },
    }
}

/// Assembles the resolved value and its claim set from a successful decode.
fn build_resolved(
    key: ResourceKey,
    assumed_type: u32,
    image: RasterImage,
) -> Result<ResolvedTexture, MaterialError> {
    let envelope = image.envelope;
    let base = image
        .layers
        .first()
        .and_then(|layer| layer.first())
        .cloned()
        .ok_or(MaterialError::EmptyImage { key })?;
    let layer_count = image.layers.len();
    let mip_count = envelope.mip_count as usize;

    let mut facts = header_claims(&key, assumed_type);
    facts.push(TextureClaim::new(
        claims::LAYER_COUNT_IS_DERIVED,
        claims::graded(
            claims::LAYER_COUNT_IS_DERIVED,
            claims::PROV_MATERIALS_DESIGN,
            LAYER_COUNT_TEXT,
        ),
    ));
    facts.push(TextureClaim::new(
        claims::DXT5_SPEC_DEVIATIONS,
        claims::graded(
            claims::DXT5_SPEC_DEVIATIONS,
            claims::PROV_DXT5_ORACLE,
            DXT5_DEVIATION_TEXT,
        ),
    ));

    Ok(ResolvedTexture {
        key,
        kind: TextureKind::Raster,
        image: base,
        envelope,
        mip_count,
        layer_count,
        pixels_decoded: true,
        claims: facts,
    })
}

/// The claims every raster resolution carries, whatever the record's contents.
///
/// The ordering is: the two facts about the *reference* (it has no type word;
/// the type word was therefore assumed), then the two about *identity* (the
/// type's name, the group's name), then the re-published format claims.
fn header_claims(key: &ResourceKey, assumed_type: u32) -> Vec<TextureClaim> {
    vec![
        TextureClaim::new(
            claims::TEXTURE_REFERENCE_TYPE_WORD,
            claims::non_finding(claims::TEXTURE_REFERENCE_TYPE_WORD),
        ),
        TextureClaim::new(
            claims::ASSUMED_RECORD_TYPE,
            claims::graded(
                claims::ASSUMED_RECORD_TYPE,
                claims::PROV_RESOLVE,
                ASSUMED_TYPE_TEXT,
            ),
        ),
        type_name_claim(assumed_type),
        group_name_claim(key.group_id),
        TextureClaim::new(
            claims::ENVELOPE_FIELD_10_MEANING,
            claims::non_finding(claims::ENVELOPE_FIELD_10_MEANING),
        ),
        TextureClaim::new(
            claims::ENVELOPE_FIELD_18_MEANING,
            claims::non_finding(claims::ENVELOPE_FIELD_18_MEANING),
        ),
        TextureClaim::new(
            claims::ENVELOPE_FIELD_1C_MEANING,
            claims::non_finding(claims::ENVELOPE_FIELD_1C_MEANING),
        ),
        // Republished from spore-texture's own claims rather than restated here,
        // so this crate cannot drift from the decoder that established it. It
        // used to be a hardcoded non-finding ("a format we refuse"); it is now a
        // VERIFIED value, and restating it locally is exactly how it went stale.
        TextureClaim::new(claims::LUMINANCE_FOURCC_FAMILY, luminance_family_claim()),
    ]
}

/// The `0x15` family's fact, taken from `spore-texture`'s own claim table.
///
/// Delegating is deliberate. When this was a local hardcoded non-finding, a
/// change in the texture crate left the material crate asserting the opposite of
/// its own decoder -- and FIVE tests failed rather than one, because the stale
/// claim was load-bearing for the batch error handling. There is now exactly one
/// place where this fact is stated, and it is the crate that measured it.
fn luminance_family_claim() -> Fact<&'static str> {
    use spore_texture::claims as tex;
    debug_assert_eq!(
        tex::LUMINANCE_FOURCC,
        claims::LUMINANCE_FOURCC_FAMILY,
        "the republished subject must be the one this claim is filed under"
    );
    tex::claim(tex::LUMINANCE_FOURCC)
        .expect("spore-texture always declares the luminance family claim")
        .meaning
        .clone()
}

/// The canonical name of the assumed type, or a non-finding when the canonical
/// table has no entry.
fn type_name_claim(type_id: u32) -> TextureClaim {
    let fact = match RecordType::new(type_id).name() {
        Some(name) => claims::graded_persisted(
            claims::ASSUMED_RECORD_TYPE_NAME,
            claims::PROV_TYPE_NAMES,
            name,
        ),
        None => claims::non_finding(claims::ASSUMED_RECORD_TYPE_NAME),
    };
    TextureClaim::new(claims::ASSUMED_RECORD_TYPE_NAME, fact)
}

/// The canonical name of the record's group, or a non-finding.
///
/// This is a non-finding for every texture of the documented asset: `0x40632900`,
/// `0x40632901` and `0x40632902` are absent from
/// [`spore_core::record::GROUP_NAMES`]. See the crate documentation for why
/// this crate does not add them.
fn group_name_claim(group_id: u32) -> TextureClaim {
    let fact = match group_name(group_id) {
        Some(name) => {
            claims::graded_persisted(claims::RESOLVED_GROUP_NAME, claims::PROV_GROUP_NAMES, name)
        }
        None => claims::non_finding(claims::RESOLVED_GROUP_NAME),
    };
    TextureClaim::new(claims::RESOLVED_GROUP_NAME, fact)
}

/// Renders a type id as its canonical name, or as bare hex when the canonical
/// table has none.
///
/// Deliberately **not** `RecordType`'s own `Display`, which spells both
/// (`"rw4 (0x2f4e681b)"`): the error message prints the hex itself, so a second
/// copy of it would only make the message harder to scan.
fn type_display(type_id: u32) -> String {
    match RecordType::new(type_id).name() {
        Some(name) => name.to_owned(),
        None => format!("0x{type_id:08x}"),
    }
}

/// The observation recorded for the assumed record type.
const ASSUMED_TYPE_TEXT: &str =
    "ASSUMED, not read from the record: the gmdl texture reference carries no type word, so \
     the caller supplied one and this decode succeeded under it. A successful decode shows \
     the bytes fit; it does not make the reference declare the type.";

/// The observation recorded for the derived layer count.
const LAYER_COUNT_TEXT: &str =
    "A raster record stores no layer count, so the count is derived by dividing the payload \
     by the layer stride, and a remainder is refused. This yields 2 on every real 512x512 \
     DXT5 record measured: 0x2f4e681c:0x40632900:0x067a07f0 and \
     0x2f4e681c:0x40632901:0x067a07f0 (2026-10-04).";

/// The observation recorded for the block codec's departures from the spec.
const DXT5_DEVIATION_TEXT: &str =
    "Spore's blocks are shaped like BC3/DXT5 and are NOT decoded like BC3/DXT5: four \
     documented departures from the published specification are reproduced on purpose, so \
     every decoded texel has red in {0,8}. A codec 'corrected' to the spec would change \
     every pixel the game draws.";

/// The fourcc a raster must carry for [`spore_texture::decode_raster`] to decode
/// it, re-exported so a caller can assert on it without depending on
/// `spore-texture` directly.
pub const REQUIRED_FOURCC: u32 = DXT5_FOURCC;

#[cfg(test)]
mod tests {
    use super::*;
    use spore_core::evidence::EvidenceLevel;

    #[test]
    fn texture_kind_declares_the_type_it_decodes() {
        assert_eq!(TextureKind::Raster.type_id(), RASTER_TYPE);
        assert_eq!(TextureKind::Raster.as_str(), "raster");
        assert_eq!(TextureKind::Raster.type_id(), 0x2F4E_681C);
    }

    #[test]
    fn required_fourcc_is_dxt5() {
        assert_eq!(REQUIRED_FOURCC, 0x3554_5844);
    }

    #[test]
    fn a_wildcard_reference_is_refused_before_any_lookup() {
        let wildcard_instance = GmdlTextureRef {
            instance_id: spore_core::WILDCARD,
            group_id: 0x4063_2900,
        };
        assert_eq!(
            reference_key(wildcard_instance, RASTER_TYPE),
            Err(MaterialError::IncompleteReference {
                reference: wildcard_instance,
                component: "instance",
            })
        );
        let wildcard_group = GmdlTextureRef {
            instance_id: 0x067A_07F0,
            group_id: spore_core::WILDCARD,
        };
        assert_eq!(
            reference_key(wildcard_group, RASTER_TYPE),
            Err(MaterialError::IncompleteReference {
                reference: wildcard_group,
                component: "group",
            })
        );
    }

    #[test]
    fn the_fourcc_refusal_is_split_out_from_the_other_texture_failures() {
        let key = ResourceKey::new(RASTER_TYPE, 1, 2);
        assert_eq!(
            map_texture_error(key, TextureError::UnsupportedFourcc { fourcc: 0x15 }),
            MaterialError::UnsupportedFourcc { key, fourcc: 0x15 }
        );
        assert_eq!(
            map_texture_error(key, TextureError::ZeroMipCount),
            MaterialError::Decode {
                key,
                source: TextureError::ZeroMipCount
            }
        );
    }

    #[test]
    fn the_assumed_type_is_recorded_as_an_assumption() {
        let key = ResourceKey::new(RASTER_TYPE, 0x4063_2900, 0x067A_07F0);
        let facts = header_claims(&key, RASTER_TYPE);
        let assumed = TextureClaim::claim(&facts, claims::ASSUMED_RECORD_TYPE).unwrap();
        assert!(assumed.fact.is_available());
        assert_eq!(assumed.fact.level(), EvidenceLevel::Inferred);
        assert!(assumed
            .fact
            .value()
            .unwrap()
            .starts_with("ASSUMED, not read from the record"));
        assert_eq!(assumed.fact.provenance().len(), 1);

        let absent = TextureClaim::claim(&facts, claims::TEXTURE_REFERENCE_TYPE_WORD).unwrap();
        assert!(absent.fact.is_unavailable());
        assert_eq!(
            absent.fact.reason(),
            Some(claims::TEXTURE_REF_HAS_NO_TYPE_WORD)
        );
        assert!(absent.fact.provenance().is_empty());
    }

    #[test]
    fn the_documented_asset_groups_have_no_canonical_name() {
        // Measured with `osptool`: the three groups the documented asset
        // references are absent from spore-core's GROUP_NAMES, so the claim is a
        // non-finding. This test is the reason the crate does not add names.
        for group in [0x4063_2900u32, 0x4063_2901, 0x4063_2902] {
            assert_eq!(group_name(group), None, "0x{group:08x}");
            let key = ResourceKey::new(RASTER_TYPE, group, 0x067A_07F0);
            let claim = group_name_claim(key.group_id);
            assert!(claim.fact.is_unavailable());
            assert_eq!(
                claim.fact.reason(),
                Some(claims::NO_GROUP_NAME_IN_CANONICAL_TABLE)
            );
        }
        // A group the table *does* name is a persisted claim, not a non-finding.
        let named = group_name_claim(0x4061_6200);
        assert!(named.fact.is_available());
        assert_eq!(named.fact.value(), Some(&"CellModels"));
        assert_eq!(named.fact.provenance().len(), 1);
    }

    #[test]
    fn an_unnamed_type_is_a_non_finding_not_a_hex_guess() {
        let claim = type_name_claim(0xDEAD_BEEF);
        assert!(claim.fact.is_unavailable());
        assert_eq!(claim.fact.reason(), Some(claims::NO_CANONICAL_TYPE_NAME));
        assert_eq!(type_display(0xDEAD_BEEF), "0xdeadbeef");
        assert_eq!(type_display(RASTER_TYPE), "raster");
        assert_eq!(
            RecordType::new(RASTER_TYPE).to_string(),
            "raster (0x2f4e681c)",
            "the canonical Display spells both, which is why errors do not use it"
        );
    }
}
