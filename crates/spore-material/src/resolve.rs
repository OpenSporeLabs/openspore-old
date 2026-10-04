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
//! 3. **The base image is published**, together with the mip count and the claim
//!    set. There is exactly **one** chain of mip levels per record — see
//!    [`claims::LAYER_COUNT_IS_DERIVED`], whose recorded value is the refutation
//!    of the old two-layer model — so nothing is flattened into a guess about
//!    which layer is which role.
//!
//! # Failures are per reference, never per batch
//!
//! [`resolve_model_textures`] returns one `Result` per reference, in encounter
//! order, and a failing reference never removes a succeeding one. That is not a
//! convenience: it is what the documented asset requires. `osptool describe` on
//! `0x00e6bce5:0x40637e03:0x067a07f0` (2026-10-04) reports three texture
//! references, and a resolver that stopped at the first failure would return
//! **no textures at all** for the one asset this repository documents end to end.

use spore_assets::ContentStore;
use spore_core::evidence::Fact;
use spore_core::record::{group_name, RecordType};
use spore_core::ResourceKey;
use spore_gmdl::{GmdlModel, GmdlTextureRef, GMDL_TYPE};
use spore_texture::{
    decode_raster, parse_envelope, MipImage, RasterEnvelope, RasterImage, TextureError,
    DXT5_FOURCC, LUMINANCE_FOURCC, RASTER_TYPE,
};

use crate::claims;
use crate::error::MaterialError;
use crate::material::{model_materials, BindingScope, MaterialClaim, MeshMaterial};
use crate::role::{self, ChannelBinding, Lane, MipStatistics, TextureRole};

/// One resolved texture: bytes, a content role, provenance, and the boundary of
/// what is known.
///
/// # One chain, not a layer stack
///
/// [`Self::layers`] holds the base mip of **every chain** in the record, in
/// record order, and for every raster record in this corpus that is **exactly
/// one element**. It is a `Vec` because the decode's shape is, and because a
/// caller written against the old two-layer reading keeps compiling — but
/// `layers.len() == layer_count == 1` is the *finding*, not an accident. See
/// [`claims::LAYER_COUNT_IS_DERIVED`], whose recorded value is the refutation of
/// the "two layers" model, and `channels()` for the consequence: **four** channel
/// bindings per texture, not eight.
///
/// [`Self::image`] is that single base mip, and it is what existing callers read.
///
/// # The role is checked against this record's own bytes
///
/// [`Self::role`] is derived by [`role::classify`] from the format word **and**
/// [`Self::statistics`], which is measured over this record's own base mip. A
/// record whose content does not match its format word gets
/// [`TextureRole::Unidentified`] rather than keeping a role.
///
/// That is a *content* role. Which sampler stage the game binds the texture to
/// is a different question and stays unknown — see
/// [`crate::SamplerRole`] and [`claims::SAMPLER_ROLE`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedTexture {
    /// The identity actually used for the lookup, with the assumed type word
    /// substituted in. Not a key the gmdl record carried.
    pub key: ResourceKey,
    /// Which record family this decoded as.
    pub kind: TextureKind,
    /// The base mip (mip 0) of the record's one chain, RGBA8 row-major.
    ///
    /// **Copy cost**: this is a clone, because the decode owns the whole mip
    /// chain. For a 512² texture that is 1 MiB per resolved texture, which is
    /// the price of a self-contained value. A caller that wants the whole chain
    /// should call [`spore_texture::decode_raster`] on
    /// [`ContentStore::read`]'s bytes itself.
    pub image: MipImage,
    /// The base mip of **every** chain, in record order.
    ///
    /// Always length [`Self::layer_count`], except from [`inspect_envelope`],
    /// which reads a header and stops: there it is empty and
    /// [`Self::pixels_decoded`] is `false`. For a decoded raster record in this
    /// corpus it is length **1**.
    pub layers: Vec<MipImage>,
    /// What this record's own texels are. See [`TextureRole`].
    pub role: TextureRole,
    /// Per-lane statistics of the base mip, gathered in one pass: distinct-value
    /// counts, extremes, means, and the cross-lane sums behind
    /// [`MipStatistics::correlation`].
    ///
    /// The evidence behind [`Self::role`], kept so a caller can see *why* a role
    /// was assigned rather than having to trust it. `texels == 0` when
    /// [`inspect_envelope`] produced this value, and the role is then
    /// [`TextureRole::Unidentified`] — an unchecked record is not a checked one.
    pub statistics: MipStatistics,
    /// The envelope the decode ran under, verbatim.
    pub envelope: RasterEnvelope,
    /// Mip levels in the record's one chain, from the envelope. Equals
    /// `envelope.mip_count`.
    pub mip_count: usize,
    /// Chains in the record. **Always 1** for a decoded raster record — see
    /// [`claims::LAYER_COUNT_IS_DERIVED`]. `0` from [`inspect_envelope`], which
    /// reads no payload and therefore states no count at all rather than
    /// reporting the count it would have derived.
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

    /// The base mip of one chain, or `None` when the index is out of range.
    ///
    /// An out-of-range index is `None` rather than a panic or a substitute
    /// chain: a renderer asking for chain 1 of a 1-chain record is asking a
    /// question the record does not answer. Under the refuted two-layer model
    /// this returned a real image for every `DXT5` record, which is why the
    /// corpus census for this file asserts that it returns `None` at index 1.
    pub fn layer(&self, index: usize) -> Option<&MipImage> {
        self.layers.get(index)
    }

    /// Whether this record's fourth lane varies.
    ///
    /// `Some(true)` and `Some(false)` for [`TextureRole::ColourAndMask`], and
    /// `None` for every other role — including [`TextureRole::Unidentified`] and
    /// the `0x15` family's normal map, whose fourth lane is a *different* lane
    /// with a different question attached (see
    /// [`claims::MASK_LAYER_PURPOSE`] for why it is unknown).
    ///
    /// This exists because the role may not assume the answer: **46 of the 1 747
    /// `DXT5` records of `Spore_Content.package` hold a constant, all-zero
    /// fourth plane.** A signature that required a varying mask would have
    /// refused them, so the per-record answer is reported here instead and the
    /// signature deliberately says nothing about this lane.
    pub fn mask_plane_varies(&self) -> Option<bool> {
        if self.role != TextureRole::ColourAndMask || self.statistics.texels == 0 {
            return None;
        }
        Some(!self.statistics.is_constant(Lane::Alpha))
    }

    /// Every channel of every chain, in `(chain, file-lane)` order.
    ///
    /// `4 * layer_count` entries, so **4** for a decoded raster record in this
    /// corpus. The order is the file's, so a `0x15` record's first channel is the
    /// normal's **Z**; see [`crate::ChannelRole::NormalZ`].
    pub fn channels(&self) -> Vec<ChannelBinding> {
        self.role.channels(self.layers.len())
    }

    /// This texture's role, and the grade it was recorded at.
    ///
    /// The two are never separable in practice, but they are separate values
    /// here so a caller can assert on one without re-deriving the other.
    pub fn role_evidence(&self) -> spore_core::evidence::EvidenceLevel {
        role::role_evidence(self.role)
    }

    /// The record's own claim that its lanes 0-2 are a tangent-space normal map
    /// stored Z first.
    ///
    /// Delegated to `spore_texture`'s claim table rather than restated: see
    /// [`crate::claims::NORMAL_MAP_CONTENT`].
    pub fn normal_map_claim(&self) -> Option<&TextureClaim> {
        self.claim(claims::NORMAL_MAP_CONTENT)
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
/// per-mesh materials those references could belong to.
///
/// # The three views, and how they line up
///
/// ```text
///   model.meshes         N     the mesh table, in record order
///   model.material_ids   N     one word per mesh, immediately after it
///   model.texture_refs   M     every material-info block, FLATTENED
///   meshes               N     one MeshMaterial per mesh
///   textures             M     one result per reference, positionally aligned
/// ```
///
/// `meshes[i].slots[j]` names `model.texture_refs[i * M/N + j]` when
/// [`Self::scope`] is [`BindingScope::PerMeshBlock`], and
/// `model.texture_refs[j]` when it is [`BindingScope::WholeModel`] or
/// [`BindingScope::Undetermined`]. All three are **inferred** —
/// [`BindingScope::is_inferred`] is `true` for every variant, because nothing
/// establishes that material-info block `i` is mesh `i`, and because
/// [`BindingScope::Undetermined`] additionally means the list `M` is not the
/// record's whole texture set.
///
/// # The reference list may be shorter than the record's
///
/// `M` is whatever the gmdl walk collected. On 619 of the 4 209 gmdl records of
/// `Spore_Content.package` that walk stops inside the material-info table on the
/// undocumented shader-data id `0x218`, and in 100 of them it stops before the
/// last declared block — so for those, `model.texture_refs` is a strict prefix of
/// the record's texture references.
///
/// [`Self::scope`] is where that shows up: it is
/// [`BindingScope::Undetermined`] when the record's own group-id ordinals
/// contradict the cut, which is what a short list looks like from inside this
/// crate. Read it with [`Self::ordinal_agreement`], which under
/// [`BindingScope::PerMeshBlock`] reports `N` of `N` agreeing — the record was
/// asked and it agreed — and reports `checked() == false` for the other two
/// scopes, where no per-mesh ordinal was derived to check.
///
/// A caller that used [`spore_gmdl::parse_recovering`] has the typed
/// [`spore_gmdl::StopReason`] in hand and should report it itself; this struct
/// does not restate it, because this crate's strict entry point
/// ([`resolve_model_record`]) can only produce a model whose table closed.
///
/// # Roles
///
/// [`Self::roles`] gives every mesh's content roles in slot order, and
/// [`Self::role_of`] the role of one flat index. Both report
/// [`TextureRole::Unidentified`] for a reference that failed to resolve; the
/// failure itself is in [`Self::failures`], and
/// [`ResolvedTexture::pixels_decoded`] is the field that distinguishes a
/// refusal from an empty decode.
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
    /// One entry per mesh, in record order.
    pub meshes: Vec<MeshMaterial>,
    /// How [`Self::meshes`]' slots were cut out of [`Self::textures`].
    pub scope: BindingScope,
    /// Corpus-level statements re-checked against **this** model: the shape of
    /// its texture set, and whether its own ordinals agree with the two
    /// independent statements of them.
    ///
    /// These are the claims a caller reads to decide how much to trust
    /// [`Self::scope`]. Each reports what was observed for this model, so a
    /// divergence from the corpus regularity is visible instead of averaged
    /// away.
    pub claims: Vec<MaterialClaim>,
    /// The shape of this model's first texture set, from its resolved roles.
    pub observed_shape: TexSetShape,
    /// Whether this model's slot ordinals agree with the group ids' low bytes.
    pub ordinal_agreement: OrdinalAgreement,
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

    /// The content role of the reference at flat position `index`.
    ///
    /// [`TextureRole::Unidentified`] for an out-of-range index or a reference
    /// that failed: both are absences, and neither is reported as a role the
    /// record does not have.
    pub fn role_of(&self, index: usize) -> TextureRole {
        self.textures
            .get(index)
            .and_then(|entry| entry.as_ref().ok())
            .map_or(TextureRole::Unidentified, |texture| texture.role)
    }

    /// Every mesh's content roles, in slot order.
    ///
    /// One `Vec` per mesh, positionally aligned with
    /// [`MeshMaterial::slots`]. A mesh's `Vec` is empty only when the mesh holds
    /// no slots; a slot whose record failed contributes
    /// [`TextureRole::Unidentified`], which is how a renderer can tell "this
    /// mesh has no textures" from "this mesh's texture did not decode".
    ///
    /// Under [`BindingScope::WholeModel`] and [`BindingScope::Undetermined`]
    /// every mesh's `Vec` is the model's **whole** collected list, so the `Vec`s
    /// are identical to one another. That is the point: the two scopes say
    /// "these textures belong to this model", not "this mesh uses this texture",
    /// and a renderer that binds per mesh must read [`Self::scope`] first.
    pub fn roles(&self) -> Vec<Vec<TextureRole>> {
        self.meshes
            .iter()
            .map(|mesh| {
                mesh.slots
                    .iter()
                    .enumerate()
                    .map(|(within, _slot)| self.role_of(self.flat_index(mesh.mesh_index, within)))
                    .collect()
            })
            .collect()
    }

    /// The flat position of mesh `mesh_index`'s `within`-th slot.
    ///
    /// The slot's **position in its own mesh's slice**, never its ordinal: under
    /// [`BindingScope::WholeModel`] a slot carries no ordinal, and under
    /// [`BindingScope::PerMeshBlock`] the two coincide anyway because a block's
    /// ordinals run `0..refs_per_mesh`.
    fn flat_index(&self, mesh_index: u32, within: usize) -> usize {
        match self.scope {
            BindingScope::PerMeshBlock { refs_per_mesh } => {
                mesh_index as usize * refs_per_mesh + within
            }
            // Both of these hand every mesh the whole collected list, so a slot's
            // flat position is its index in that mesh's slice. `Undetermined` is
            // NOT folded into the first arm: it is precisely the case where
            // multiplying by a per-mesh width would put one mesh's references on
            // another.
            BindingScope::WholeModel | BindingScope::Undetermined { .. } => within,
        }
    }

    /// Looks a model-level claim up by subject.
    pub fn claim(&self, subject: &str) -> Option<&MaterialClaim> {
        MaterialClaim::claim(&self.claims, subject)
    }

    /// Every distinct role this model's textures resolved to, with counts.
    ///
    /// A summary line, for a log or a report: "1 tangent-space-normal,
    /// 2 colour-and-mask, 0 unidentified".
    pub fn role_census(&self) -> Vec<(TextureRole, usize)> {
        let mut counts: Vec<(TextureRole, usize)> = Vec::new();
        for entry in &self.textures {
            let role = entry.as_ref().map_or(TextureRole::Unidentified, |t| t.role);
            match counts.iter_mut().find(|(seen, _)| *seen == role) {
                Some((_, count)) => *count += 1,
                None => counts.push((role, 1)),
            }
        }
        counts
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
            claims::PROV_TEXTURE_CLAIMS,
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
        // Likewise no chains: `layer_count` is 0 and `layers` is empty, which
        // agree. `layers.len()` is what `channels()` iterates, so an empty list
        // here means "no channel is claimed", not "every channel is unknown".
        layers: Vec::new(),
        // `Unidentified` because no content was looked at. A header read is not
        // a check: `statistics.texels == 0` and `classify` declines on that, so
        // the role cannot be `TangentSpaceNormal` on the strength of a fourcc
        // alone. This is the crate's "we did not look", not "we looked and found
        // nothing".
        role: TextureRole::Unidentified,
        statistics: MipStatistics::empty(),
        envelope,
        mip_count: envelope.mip_count as usize,
        // Zero, and deliberately not 1: a full decode answers this with 1, and a
        // header read has not looked at anything that could answer it. `None`
        // would have been better than a field, but the field is the API.
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
    let textures = resolve_model_textures(store, &model, assumed_type);
    let meshes = model_materials(&model);
    let scope = meshes
        .first()
        .map_or(BindingScope::WholeModel, |mesh| mesh.scope);
    let observed_shape = observe_shape(&textures, &meshes);
    let ordinal_agreement = observe_ordinal_agreement(&meshes);
    let claims = model_claims();
    Ok(ModelTextures {
        key: *key,
        textures,
        meshes,
        scope,
        claims,
        observed_shape,
        ordinal_agreement,
        model,
    })
}

/// The corpus regularities this crate can state, plus the per-model observation
/// that checks them.
///
/// The claim values are `'static` because [`claims::graded`] is, and a `Fact`
/// cannot hold an owned string. The per-model numbers therefore live in plain
/// fields ([`ModelTextures::observed_shape`] and
/// [`ModelTextures::ordinal_agreement`]) rather than being interpolated into
/// prose — which is the better place for them anyway, because a caller can read
/// a number without parsing a sentence.
fn model_claims() -> Vec<MaterialClaim> {
    vec![
        MaterialClaim::new(
            claims::TEXTURE_SET_SHAPES,
            claims::graded(
                claims::TEXTURE_SET_SHAPES,
                claims::PROV_MATERIAL_ROLES,
                SHAPE_TEXT,
            ),
        ),
        MaterialClaim::new(
            claims::SAMPLER_WORD_IS_ENTRY_ORDINAL,
            claims::graded(
                claims::SAMPLER_WORD_IS_ENTRY_ORDINAL,
                claims::PROV_MATERIAL_ROLES,
                ORDINAL_TEXT,
            ),
        ),
        MaterialClaim::new(
            claims::GROUP_LOW_BYTE_IS_SLOT,
            claims::graded(
                claims::GROUP_LOW_BYTE_IS_SLOT,
                claims::PROV_MATERIAL_ROLES,
                ORDINAL_TEXT,
            ),
        ),
    ]
}

/// The corpus statement about texture-set shapes.
const SHAPE_TEXT: &str =
    "A gmdl texture set occurs in exactly two shapes in Spore_Content.package: (DXT5, DXT5, \
     0x15) in 1922 sets and (DXT5, 0x15) in 459 sets, out of the 2381 sets carried by the 1819 \
     records the repository's Python oracle walks. Recorded here as a corpus regularity, NOT as \
     the basis of a role: the second entry is DXT5 in 1922 sets and 0x15 in 459, so an ordinal \
     does not determine the content. What this model actually holds is in \
     ModelTextures::observed_shape. The names are fourccs, not roles: a DXT5 entry is graded \
     colour-and-mask here and a 0x15 entry a tangent-space normal map, both read from each \
     record's own bytes.";

/// The corpus statement about the two independent ordinals.
const ORDINAL_TEXT: &str =
    "A texture-set entry's first header word and the referenced record's group id low byte are \
     two independent statements of the same ordinal, and they agree in 6684 of 6684 references \
     measured over Spore_Content.package. This crate derives the ordinal from position within a \
     mesh's slice, so the group id's low byte is available as a free cross-check; whether they \
     agreed for THIS model is in ModelTextures::ordinal_agreement.";

/// The shape of one model's texture set, as far as this crate can observe it.
///
/// Derived from the **resolved roles**, so it is a check rather than a
/// quotation of the corpus. Two variants match the corpus measurement; the
/// third is what a divergence looks like, and it is a variant rather than an
/// error because a divergence is a finding.
///
/// The two named variants were **renamed on 2026-10-05**, from `MaskNormal` and
/// `MaskMaskNormal`. "Mask" named the refuted two-layer model: the records in
/// those slots are colour images with a fourth plane, and the two slots of a
/// three-entry set are two *records*, not two layers of one. `as_str` keeps the
/// old shapes visible in a log line so a reader can match a line printed before
/// the rename against one printed after it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TexSetShape {
    /// `(colour-and-mask, normal)` — the 459-set shape in the corpus.
    ColourAndMaskThenNormal,
    /// `(colour-and-mask, colour-and-mask, normal)` — the 1922-set shape in the
    /// corpus.
    ColourAndMaskTwiceThenNormal,
    /// Something else. Carries the roles actually observed, in slot order.
    Divergent(Vec<TextureRole>),
    /// The shape cannot be isolated: no mesh, no references, or no even cut, so
    /// no block boundary is known.
    ///
    /// Distinct from [`TexSetShape::Divergent`] on purpose. "We could not look"
    /// is not "we looked and found something else", and collapsing the two is
    /// how a corpus regularity starts being reported as a per-record fact.
    Unavailable,
}

impl TexSetShape {
    /// Whether this shape is one of the two the corpus measured.
    pub fn matches_corpus(&self) -> bool {
        matches!(
            self,
            Self::ColourAndMaskThenNormal | Self::ColourAndMaskTwiceThenNormal
        )
    }

    /// The canonical spelling, for logs and JSON.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ColourAndMaskThenNormal => "(colour-and-mask, normal)",
            Self::ColourAndMaskTwiceThenNormal => "(colour-and-mask, colour-and-mask, normal)",
            Self::Divergent(_) => "divergent",
            Self::Unavailable => "unavailable",
        }
    }
}

/// How many of a model's slot ordinals the group id agreed with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OrdinalAgreement {
    /// Slots that carried an ordinal to compare.
    pub compared: usize,
    /// Slots where the derived ordinal equalled the group id's low byte.
    pub agreed: usize,
}

impl OrdinalAgreement {
    /// Whether every comparison agreed.
    ///
    /// `true` for **zero** comparisons: "nothing to contradict" is not a
    /// contradiction, and a renderer must not treat an uncheckable model as a
    /// broken one.
    pub fn all_agree(&self) -> bool {
        self.agreed == self.compared
    }

    /// Whether there was anything to compare at all.
    pub fn checked(&self) -> bool {
        self.compared > 0
    }
}

/// Observes one model's texture-set shape from its resolved roles.
///
/// Only the **first** block is inspected. Every block of a model contributed the
/// same number of references in the corpus (`2381`/`2381`), but "the same
/// number" is not "the same content", so reading further would assert something
/// unmeasured. What this returns is one set's shape, and says so.
fn observe_shape(
    textures: &[Result<ResolvedTexture, MaterialError>],
    meshes: &[MeshMaterial],
) -> TexSetShape {
    let Some(mesh) = meshes.first() else {
        return TexSetShape::Unavailable;
    };
    let per_mesh = match mesh.scope {
        BindingScope::PerMeshBlock { refs_per_mesh } if refs_per_mesh > 0 => refs_per_mesh,
        // `refs_per_mesh == 0` is a real, consistent answer -- every block was
        // empty -- and it has no shape to report. `WholeModel` has no boundary.
        _ => return TexSetShape::Unavailable,
    };
    let roles: Vec<TextureRole> = (0..per_mesh)
        .map(|within| {
            textures
                .get(within)
                .and_then(|entry| entry.as_ref().ok())
                .map_or(TextureRole::Unidentified, |texture| texture.role)
        })
        .collect();
    let colour = TextureRole::ColourAndMask;
    let normal = TextureRole::TangentSpaceNormal;
    match roles.as_slice() {
        [a, b] if *a == colour && *b == normal => TexSetShape::ColourAndMaskThenNormal,
        [a, b, c] if *a == colour && *b == colour && *c == normal => {
            TexSetShape::ColourAndMaskTwiceThenNormal
        }
        _ => TexSetShape::Divergent(roles),
    }
}

/// Counts the slots whose derived ordinal the record's own group id agreed with.
///
/// A slot whose group id states **no** ordinal — one naming group id `0`, which
/// 150 corpus references do — is skipped rather than counted as agreeing or as
/// conflicting: it said nothing. `compared` is therefore "slots that were
/// actually checked", and [`OrdinalAgreement::checked`] is `false` when it is
/// zero, which is not the same as "nothing disagreed".
fn observe_ordinal_agreement(meshes: &[MeshMaterial]) -> OrdinalAgreement {
    let mut agreement = OrdinalAgreement {
        compared: 0,
        agreed: 0,
    };
    for mesh in meshes {
        for slot in &mesh.slots {
            if slot.ordinal.is_none() || !slot.group_states_an_ordinal() {
                continue;
            }
            agreement.compared += 1;
            if slot.ordinal_agrees() {
                agreement.agreed += 1;
            }
        }
    }
    agreement
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
    // The base mip of EVERY chain in the record, which for a raster record in
    // this corpus is exactly one: `spore-texture` verifies the record's length
    // against a one-chain framing identity rather than dividing a payload to find
    // a layer count, so there is nothing here to derive and nothing to discard.
    // See `claims::LAYER_COUNT_IS_DERIVED`, whose recorded value is the
    // refutation of the old two-chain reading. Cloned out of the decode, which is
    // the price of a self-contained value; a caller that wants the whole chain
    // should call `spore_texture::decode_raster` itself.
    let mut layers: Vec<MipImage> = Vec::with_capacity(image.layers.len());
    for layer in &image.layers {
        layers.push(
            layer
                .first()
                .cloned()
                .ok_or(MaterialError::EmptyImage { key })?,
        );
    }
    let base = layers
        .first()
        .cloned()
        .ok_or(MaterialError::EmptyImage { key })?;
    let layer_count = layers.len();
    let mip_count = envelope.mip_count as usize;

    // The role is decided by this record's own bytes, not by its family's name:
    // the format word picks which signature to try and `classify` still has to
    // pass it. The statistics are published beside the role so a caller can see
    // the evidence instead of trusting the verdict.
    let statistics = MipStatistics::of(&base);
    let role = role::classify(envelope.fourcc, &statistics);
    debug_assert!(
        role_text_agrees_with_statistics(envelope.fourcc, role, &statistics),
        "the recorded role text must agree with the statistics it describes"
    );

    let mut facts = header_claims(&key, assumed_type);
    facts.push(TextureClaim::new(
        claims::LAYER_COUNT_IS_DERIVED,
        claims::graded(
            claims::LAYER_COUNT_IS_DERIVED,
            claims::PROV_TEXTURE_CLAIMS,
            LAYER_COUNT_TEXT,
        ),
    ));
    facts.push(TextureClaim::new(
        claims::DXT5_SPEC_DEVIATIONS,
        claims::graded(
            claims::DXT5_SPEC_DEVIATIONS,
            claims::PROV_TEXTURE_CLAIMS,
            DXT5_DEVIATION_TEXT,
        ),
    ));
    facts.push(TextureClaim::new(
        claims::TEXTURE_CONTENT_ROLE,
        claims::graded(
            claims::TEXTURE_CONTENT_ROLE,
            claims::PROV_RESOLVE,
            role_text(role),
        ),
    ));
    facts.push(normal_map_claim());
    facts.push(TextureClaim::new(
        claims::COLOUR_AND_MASK_CONTENT,
        claims::graded(
            claims::COLOUR_AND_MASK_CONTENT,
            claims::PROV_MATERIAL_ROLES,
            COLOUR_AND_MASK_TEXT,
        ),
    ));
    facts.push(TextureClaim::new(
        claims::MASK_PLANE_INDEPENDENT_OF_COLOUR,
        claims::graded(
            claims::MASK_PLANE_INDEPENDENT_OF_COLOUR,
            claims::PROV_MATERIAL_ROLES,
            MASK_PLANE_INDEPENDENCE_TEXT,
        ),
    ));
    facts.push(TextureClaim::new(
        claims::DXT5_CARRIES_NO_COLOUR,
        claims::graded(
            claims::DXT5_CARRIES_NO_COLOUR,
            claims::PROV_MATERIAL_ROLES,
            DXT5_NO_COLOUR_TEXT,
        ),
    ));
    facts.push(TextureClaim::new(
        claims::DXT5_LAYERS_ARE_DISTINCT,
        claims::graded(
            claims::DXT5_LAYERS_ARE_DISTINCT,
            claims::PROV_MATERIAL_ROLES,
            DXT5_LAYERS_TEXT,
        ),
    ));
    facts.push(TextureClaim::new(
        claims::MASK_LAYER_PURPOSE,
        claims::non_finding(claims::MASK_LAYER_PURPOSE),
    ));

    Ok(ResolvedTexture {
        key,
        kind: TextureKind::Raster,
        image: base,
        layers,
        role,
        statistics,
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
        // Delegated, like the luminance family below. `field_18` used to be a
        // local non-finding ("meaning not established") whose real state was
        // already measured upstream: it is the base level's own byte count in
        // all three formats, verified over 4 386 records, and the decoder reads
        // it as a cross-check. Restating it as unknown would be this crate
        // disagreeing with its own decoder for the second time.
        TextureClaim::new(claims::ENVELOPE_FIELD_18_MEANING, envelope_field_18_claim()),
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

/// `field_18`'s meaning, taken from `spore_texture`'s own claim table.
///
/// The third time this crate has delegated a fact about the raster envelope, and
/// the second time it was about to contradict its own decoder. The grade must
/// survive delegation: this crate files the republished fact under a subject, and
/// a silent downgrade upstream would contradict this table.
fn envelope_field_18_claim() -> Fact<&'static str> {
    use spore_texture::claims as tex;
    debug_assert_eq!(
        tex::ENVELOPE_FIELD_18_MEANING,
        claims::ENVELOPE_FIELD_18_MEANING,
        "the republished subject must be the one this claim is filed under"
    );
    tex::claim(tex::ENVELOPE_FIELD_18_MEANING)
        .expect("spore-texture always declares the field_18 claim")
        .meaning
        .clone()
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

/// The observation recorded for the refuted block codec.
const DXT5_DEVIATION_TEXT: &str =
    "REFUTED, 2026-10-05. Spore's blocks ARE BC3. The four departures from the published \
     specification that this crate previously reproduced on purpose -- an inverted \
     palette-size test, R5G5B5 channel reads, a 6/2 alpha blend on the endpoint codes and \
     no alpha0 == alpha1 collapse -- were real and byte-pinned, and that is exactly the \
     problem: two implementations agreeing with each other on synthetic input produced a \
     specification no record supports. Every recorded level prefix in every DXT5 record is \
     the 16-byte-per-block figure, and that byte count is what refuted the 8-byte rule.";

/// The observation recorded for the refuted layer count.
const LAYER_COUNT_TEXT: &str =
    "REFUTED, 2026-10-05: there is exactly ONE chain of mip levels per record, and nothing \
     is derived. The previous model divided the payload by the layer stride and reported 2 \
     on every DXT5 record; that was a double error which cancelled exactly -- 8 bytes per \
     4x4 block instead of 16 makes one picture look like two half-pictures. spore-texture \
     closes an exact record-length identity in 4386 of 4386 records across \
     Spore_Content.package and Spore_EP1_Content_01.package, so layers.len() is 1 and this \
     crate has no layer count to derive. Inspecting an envelope reports 0, not 1, because a \
     header read has looked at nothing that could answer it.";

/// The observation recorded for the refuted 'DXT5 carries no colour' claim.
const DXT5_NO_COLOUR_TEXT: &str =
    "REFUTED, 2026-10-05, and this text is the refutation. The claim was a tautology of a \
     broken codec rather than a measurement of any record: green and blue read as zero under \
     R5G5B5 on a 6-BIT word BY CONSTRUCTION, because a 6-bit value cannot set bits 10..19. \
     Read as BC3, over all 1747 DXT5 records of Spore_Content.package, every record's lane 0 \
     takes at least 32 distinct values (max 235), lane 1 at least 64 (max 255) and lane 2 at \
     least 32 (max 235); 0 of 1747 have a colour lane of two values or fewer; and the \
     documented record 0x2f4e681c:0x40632900:0x067a07f0 carries 4436 distinct RGB triples in \
     one 512x512 base mip. The old 'exactly 2 distinct RGB triples, (0,0,0) and (8,0,0)' was \
     what the broken codec produced.";

/// The observation recorded for the refuted 'two layers are distinct' claim.
const DXT5_LAYERS_TEXT: &str =
    "REFUTED, 2026-10-05, and this text is the refutation. There are no two layers: every \
     raster record in the corpus is one chain, so this crate has nothing to compare. The old \
     1233-of-1233 measurement was real arithmetic over the wrong thing -- it compared the two \
     HALVES OF ONE PICTURE that an 8-byte block stride had split in two, which is why they \
     differed byte for byte. What survives is a different statement: a model's two DXT5 \
     records are two distinct RECORDS, and the two alpha planes the old model called 'layer \
     0' and 'layer 1' were the first and second halves of a single base mip.";

/// The observation recorded for the colour-and-mask reading of a BC3 record.
const COLOUR_AND_MASK_TEXT: &str =
    "Read as BC3, a DXT5 record's lanes 0-2 are a three-channel colour image and lane 3 is a \
     fourth plane that does not vary with them. Assigned by checking THIS record's own base \
     mip -- each colour lane takes at least 8 distinct values, against a corpus minimum of 32 \
     -- not by its position in the model's texture list and not by its format word alone. The \
     fourth lane is deliberately NOT part of the check: 46 of the 1747 corpus records hold a \
     constant, all-zero fourth plane, so requiring one would refuse 46 real records. Whether \
     this record's own fourth plane varies is on ResolvedTexture::mask_plane_varies. Graded at \
     the BC3 codec's own grade and no higher; see colour_and_mask_content.";

/// The observation recorded for the fourth plane's independence from the colour planes.
const MASK_PLANE_INDEPENDENCE_TEXT: &str =
    "Over all 1747 DXT5 records of Spore_Content.package the mean absolute colour-to-alpha \
     correlation has median 0.067 and maximum 0.539, against a median mutual correlation of \
     0.510 among the colour lanes themselves. The numbers behind this are per-record and \
     published, not quoted here: MipStatistics::mean_alpha_correlation_abs and \
     MipStatistics::mean_colour_correlation are where a caller re-derives them. NOT used as a \
     gate -- 467 of 1747 records fall below a 0.30 mean colour correlation, so any correlation \
     threshold would be a statistic rather than a property.";

/// The fourcc a raster must carry for [`spore_texture::decode_raster`] to decode
/// it, re-exported so a caller can assert on it without depending on
/// `spore-texture` directly.
pub const REQUIRED_FOURCC: u32 = DXT5_FOURCC;

/// The observation text for the role this record's own bytes earned.
///
/// The numbers behind the verdict are already on
/// [`ResolvedTexture::statistics`], so this text names the *condition* that was
/// checked rather than restating the measurement — and
/// [`role_text_agrees_with_statistics`] asserts that the condition and the
/// verdict cannot drift apart.
fn role_text(role: TextureRole) -> &'static str {
    match role {
        TextureRole::TangentSpaceNormal => NORMAL_ROLE_TEXT,
        TextureRole::ColourAndMask => COLOUR_ROLE_TEXT,
        TextureRole::Unidentified => UNIDENTIFIED_ROLE_TEXT,
    }
}

/// The text recorded for a normal-map role.
const NORMAL_ROLE_TEXT: &str =
    "Tangent-space normal map, stored Z FIRST: lane 0 is Z, not R. Assigned by checking this \
     record's own base mip -- lane 0 mean >= 224, lane 0 exceeding the brighter lateral lane by \
     >= 64, and lanes 1 and 2 within 16 of 127.5 -- not by its position in the model's texture \
     list and not by its format word alone. Corpus extremes over 901 records: lane 0 mean \
     232.51..255.00, smallest lead 94.2, largest lateral deviation 10.80. The seven records \
     holding the constant (255, 128, 128), which is the flat normal (0, 0, 1), satisfy the \
     check: a constant field is a degenerate normal map, not a failed one.";

/// The text recorded for a DXT5 role.
const COLOUR_ROLE_TEXT: &str =
    "A three-channel colour image in lanes 0-2 over a fourth plane that does not vary with \
     them, in ONE chain of mip levels. Assigned by checking this record's own base mip -- \
     each of lanes 0, 1 and 2 takes at least 8 distinct values -- and not by its format word \
     alone. The fourth lane is not part of the check because 46 of the 1747 corpus records \
     hold a constant one; read this record's own answer from \
     ResolvedTexture::mask_plane_varies. The colour planes are REAL: see \
     dxt5_carries_no_colour, which now records the refutation of the claim that they were \
     not. What either the colour or the fourth plane is FOR is unknown; see \
     mask_layer_purpose.";

/// The text recorded when no signature held.
///
/// Says "for its format word" on purpose. The *other* signature may well hold —
/// a `0x15` record whose bytes are a colour field is `Unidentified` while
/// satisfying the colour signature perfectly — and the previous wording, "matched
/// no signature", claimed a negative the check did not make.
const UNIDENTIFIED_ROLE_TEXT: &str =
    "The record decoded and its content matched no signature FOR ITS FORMAT WORD: as a 0x15 \
     record its base mip is not a tangent-space normal map, and as a DXT5 record its lanes are \
     not three colour lanes that each take at least 8 distinct values. The other family's \
     signature may still hold -- that is what makes the fourcc necessary rather than decorative. \
     Reported rather than defaulted, because the format word alone would have named a role this \
     record's bytes did not earn.";

/// Whether the text recorded for `role` agrees with the statistics behind it.
///
/// Checked in debug builds inside [`build_resolved`]. This is the guard against
/// the failure this crate exists for: prose that keeps asserting something the
/// numbers stopped supporting.
///
/// `fourcc` is part of the question, and the reason is a real case rather than a
/// tidiness one. [`TextureRole::Unidentified`] means **this record's own format
/// word's** signature declined. The other signature may well hold: a `0x15`
/// record whose bytes are a colour field classifies as `Unidentified` while
/// satisfying [`role::colour_and_mask_signature`] perfectly. An earlier version
/// of this guard required *both* signatures to decline, which is false — and the
/// debug assertion caught it, which is the whole reason the guard exists.
fn role_text_agrees_with_statistics(fourcc: u32, role: TextureRole, stats: &MipStatistics) -> bool {
    let normal = role::normal_map_signature(stats);
    let colour = role::colour_and_mask_signature(stats);
    match role {
        TextureRole::TangentSpaceNormal => fourcc == LUMINANCE_FOURCC && normal,
        TextureRole::ColourAndMask => fourcc == DXT5_FOURCC && colour,
        TextureRole::Unidentified => match fourcc {
            LUMINANCE_FOURCC => !normal,
            DXT5_FOURCC => !colour,
            // A fourcc with no signature at all: declining is all it can do.
            _ => !normal && !colour,
        },
    }
}

/// The normal-map family's content fact, delegated to `spore_texture`.
///
/// Delegating is deliberate, and it is the second time this crate has done it
/// for a fact about this family. The first version restated it locally; when
/// `spore-texture` corrected the family's description the material crate
/// asserted the opposite of its own decoder and five tests failed rather than
/// one, because the stale claim was load-bearing for the batch error handling.
/// There is now exactly one place where this fact is stated, and it is the
/// crate that measured it.
fn normal_map_claim() -> TextureClaim {
    use spore_texture::claims as tex;
    let republished = tex::claim(tex::LUMINANCE_TEXEL_LAYOUT)
        .expect("spore-texture always declares the 0x15 texel-layout claim")
        .meaning
        .clone();
    // The grade must survive delegation: this crate files the republished fact
    // under a VERIFIED subject, so a downgrade upstream would silently
    // contradict the table here.
    assert_eq!(
        republished.level(),
        spore_core::evidence::EvidenceLevel::Verified,
        "the republished normal-map fact must stay VERIFIED, or \
         claims::NORMAL_MAP_CONTENT is misgraded"
    );
    TextureClaim::new(claims::NORMAL_MAP_CONTENT, republished)
}

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
