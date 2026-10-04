//! What a mesh's material is made of, decided from the record's own content.
//!
//! This is the whole of Job 1, and it is Bevy-free on purpose: the arithmetic
//! that decides which texture feeds which material slot is the part most likely
//! to be wrong, and it must be testable by asserting **texel values**, not by
//! looking at a window.
//!
//! # What the corpus says, and what this module therefore does
//!
//! Measured over `SPORE/Data/Spore_Content.package`:
//!
//! * **All 2 954 raster records decode, and none of them is published as a
//!   base-colour map.** `spore-material` derives a role from the record's
//!   `fourcc` **and** a per-lane statistic over its own base mip, and it
//!   publishes exactly one bindable content class in this corpus: the `0x15`
//!   family as a **tangent-space normal map stored Z FIRST** (901 of 901,
//!   first byte's mean 232.51 … 255.00, leading the brighter lateral lane by
//!   at least 94.21). Every other record lands in `TextureRole::Unidentified`.
//! * **The colour of a Spore model is in its `sporemodel` record**, not in a
//!   raster — see [`crate::domain::paint`]. So `base_color_texture` is `None`
//!   always, and there is no `Diffuse` here: adding one would be a claim about
//!   a record that does not exist.
//!
//! # REFUTED: "a raster record carries no colour, only alpha"
//!
//! This module used to carry that claim in the paragraph above, and every test
//! in it was written to defend it: *"the corpus's common three-entry set offers
//! **four** planes per mesh"*, `layer_index` on every mask, `DegenerateColour`
//! for the colour lanes. All of it was a **tautology of a broken decoder**.
//! `spore-texture` decoded `DXT5` as an 8-byte-per-block format, whose colour
//! field cannot set bits 10..19 of a 16-bit word, so green and blue read as
//! zero *by construction* for any input at all. The corpus has since proved
//! the block stride is **16** (BC3), and the same records now carry **32
//! distinct greens and 32 distinct blues**. Measured on this record set: the
//! creature's `DXT5` texture takes **233 / 254 / 230 distinct values** in red,
//! green and blue over its 262 144 texels.
//!
//! Three things follow, and they are the reason this module no longer names a
//! role at all:
//!
//! 1. `layers` is **one chain, always** — there are no layer headers and no
//!    second surface, so `layer_index` is `0` everywhere and a per-layer loop
//!    was iterating an artefact;
//! 2. `TextureRole::LayeredAlphaMask`'s content signature is a statement about
//!    the *old* decoder, and with the decoder fixed no corpus record satisfies
//!    it. `spore-material` publishes `Unidentified` for all of them, which is
//!    an observation ("we checked and it is not that") rather than a default;
//! 3. the engine therefore binds **from the channel bindings
//!    `spore-material` publishes**, never from a role variant it expects. See
//!    [`MeshMaterialPlan`] and [`NormalMapPlan::from_channels`].
//!
//! # What is still NOT established, and is still refused
//!
//! Refuting "a raster carries no colour" is **not** the same as establishing
//! that a raster *is* a colour map. Nothing measured here says what a `DXT5` or
//! `DXT1` record is *for*; the purposes of every non-normal channel remain
//! `UNKNOWN` (`spore_material::claims::MASK_LAYER_PURPOSE`), and binding one
//! into `base_color_texture` or `metallic_roughness_texture` is the exact
//! confusion this module was written to prevent. So: the normal map binds, and
//! everything else is uploaded nowhere and *reported* — see
//! [`UnboundReason`], which is what the report line prints.
//! # The index math, and why it is not a zip
//!
//! `spore-gmdl` flattens every material-info block's texture references into
//! one list and loses the block boundaries. `spore-material` recovers a cut by
//! dividing by the mesh count, cross-checks it against the record's own
//! statement of each entry's position, and grades the whole thing **INFERRED**
//! (`BindingScope::is_inferred()` is `true` for every variant, including the
//! third). The mapping is:
//!
//! ```text
//! flat = mesh_index * refs_per_mesh + within      // PerMeshBlock { refs_per_mesh }
//! flat = within                                  // WholeModel | Undetermined { .. }
//! ```
//!
//! `ModelTextures::roles()` computes this. [`ModelMaterials::roles`] computes
//! the same thing from the same inputs, and
//! `tests/real_corpus.rs` asserts the two agree on the real documented asset —
//! so the engine has one algorithm and `spore-material` remains the reference.
//!
//! # Three scopes, and the third one is the one the creature takes
//!
//! `BindingScope` has **three** arms and this engine matches all three wherever
//! it touches a cut:
//!
//! | arm | when | what a mesh holds |
//! |---|---|---|
//! | `PerMeshBlock { refs_per_mesh }` | the division is even **and** the record's group-id ordinals agree with it | its own slice |
//! | `WholeModel` | the division is not even, or no slot states an ordinal to check with | the whole list |
//! | `Undetermined { collected, contradicted }` | the division is even but **the record contradicts it** | the whole (short) list |
//!
//! `Undetermined` is a **short** list seen from the outside: the gmdl
//! material-info walk stops on the undocumented shader-data id `0x218` in 619
//! of 4 209 gmdl records, and because what is missing is *whole blocks* the
//! truncated list still divides evenly — so the division used to succeed on
//! incomplete input and hand mesh 1 a texture out of mesh 0's block. The
//! documented creature is one of those records.
//!
//! [`ModelMaterials::flat_index`] gives every mesh the whole collected list
//! under `Undetermined`, which is the conservative answer, and
//! [`stated_ordinal`] states **no** ordinal under it, so the engine cannot
//! manufacture a conflict out of the refusal itself. On top of that,
//! [`ModelMaterials::plan_for`] refuses to bind a slot whose flat entry carries
//! a **different reference**: the cut can then never silently move a texture
//! between blocks, even if a future `spore-material` offers a cut this engine
//! would disagree with.
//!
//! # The swizzle, stated three times because it is the likeliest mistake here
//!
//! File byte 0 of a normal map is **Z**. Bevy's `NormalMap` path samples
//! `R = X, G = Y, B = Z` and remaps with `n * 2 - 1`. So:
//!
//! ```text
//! on disk        (Z, X, Y, ?)
//! after swizzle  (X, Y, Z, 255)
//! ```
//!
//! [`swizzle_z_first_to_xyz`] performs that permutation on the bytes, and
//! [`NormalMapPlan::swizzle`] records that it happened. A wrong swizzle looks
//! *almost* right — which is exactly why the unit tests assert the output bytes
//! of the flat normal `(255, 128, 128)`, not that the function ran.

use spore_assets::LoadedModel;
use spore_core::ResourceKey;
use spore_gmdl::GmdlModel;
use spore_material::{
    BindingScope, ChannelRole, Lane, MaterialError, ModelTextures, OrdinalAgreement,
    ResolvedTexture, TexSetShape, TextureRole,
};

/// How a mesh's normal map's lanes are stored on disk.
///
/// A closed set of the *layouts this repository has established*. `ZFirst` is
/// the one the corpus shows; the others exist so a future record with a
/// different order is a *finding* rather than a silent misread.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NormalLayout {
    /// `byte 0 = Z`, `byte 1 = X`, `byte 2 = Y`. `0x15` records.
    ///
    /// 901/901 records over `Spore_Content.package`, and the seven constant
    /// ones hold `(255, 128, 128)`, which is the encoding of the flat normal
    /// `(0, 0, 1)`.
    ZFirst,
}

impl NormalLayout {
    /// The canonical spelling, for logs and reports.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ZFirst => "z-first",
        }
    }

    /// `[byte of X, byte of Y, byte of Z]` — the **inverse** of
    /// [`lane_components`], and the form the channel bindings arrive in.
    ///
    /// Both directions exist because both are easy to write backwards, and a
    /// layout that is only stated one way is a layout that can only be checked
    /// one way. `tests` asserts they are inverses of each other.
    pub const fn lanes(self) -> [usize; 3] {
        match self {
            // `lane_components` says byte 0 holds Z (index 2), byte 1 holds X
            // (0), byte 2 holds Y (1).
            Self::ZFirst => [1, 2, 0],
        }
    }
}

/// The `file byte -> component` permutation a layout describes.
///
/// `values[i]` is the component stored in file byte `i`.
pub const fn lane_components(layout: NormalLayout) -> [u8; 3] {
    match layout {
        // byte 0 holds Z (index 2), byte 1 holds X (0), byte 2 holds Y (1).
        NormalLayout::ZFirst => [2, 0, 1],
    }
}

/// Reorders a normal map's texels from file order into `R = X, G = Y, B = Z`.
///
/// One `Vec<u8>` in, one out, same length. Bevy's `apply_normal_mapping`
/// samples `.rgb` and remaps with `* 2 - 1`, so the upload must already be in
/// that order; the alternative (telling Bevy the layout) does not exist in
/// 0.19 — the image's `TextureFormat` is the only lever, and no wgpu format
/// reinterprets channel order.
///
/// The alpha output byte is **255**, not the file's fourth lane: that lane is
/// `ChannelRole::UnidentifiedLane` for every `0x15` record (statistically
/// independent of the normal, correlation 0.032, meaning UNKNOWN), and a
/// normal map's alpha is not read. Carrying an unknown byte into a channel the
/// shader ignores would be noise dressed as data.
pub fn swizzle_z_first_to_xyz(pixels: &[u8]) -> Vec<u8> {
    let texels = pixels.len() / 4;
    let mut out = Vec::with_capacity(pixels.len());
    for texel in pixels.chunks_exact(4) {
        let (z, x, y) = (texel[0], texel[1], texel[2]);
        out.extend_from_slice(&[x, y, z, u8::MAX]);
    }
    // A trailing partial texel is carried through untouched rather than
    // dropped: the caller can then see that its buffer was not texel-aligned.
    out.extend_from_slice(&pixels[texels * 4..]);
    out
}

/// Extracts one byte lane of an RGBA8 image into a single-channel buffer.
///
/// Used for a channel `spore-material` published as
/// [`ChannelRole::MaskChannel`]. The lane is **not** chosen here: it arrives in
/// the plan, from the record's own binding, because the engine's job is to bind
/// what is published rather than to guess which lane was meant.
pub fn extract_lane(pixels: &[u8], lane: usize) -> Vec<u8> {
    pixels.iter().skip(lane).step_by(4).copied().collect()
}

/// One texture reference, resolved or refused.
#[derive(Debug, Clone, PartialEq)]
pub struct MaterialTexture {
    /// The reference the record carried, when the flat index has one.
    pub reference: Option<spore_gmdl::GmdlTextureRef>,
    /// The decoded record, when it decoded.
    pub resolved: Option<ResolvedTexture>,
    /// Why it did not, when it did not.
    ///
    /// Distinct from `resolved == None && refusal == None`, which means "no such
    /// flat index": an absence of a reference, not a failure to read one.
    pub refusal: Option<String>,
}

impl MaterialTexture {
    /// The role of this entry.
    ///
    /// [`TextureRole::Unidentified`] for a refusal and for an absent reference
    /// alike, because neither is a role the record has. `resolved` distinguishes
    /// them.
    pub fn role(&self) -> TextureRole {
        self.resolved
            .as_ref()
            .map_or(TextureRole::Unidentified, |t| t.role)
    }

    /// The identity actually looked up, when the record was found.
    pub fn key(&self) -> Option<ResourceKey> {
        self.resolved.as_ref().map(|t| t.key)
    }
}

/// Which of the two routes produced a model's role table.
///
/// `spore-material`'s [`spore_material::resolve_model_record`] re-reads the
/// record with the **strict** `spore_gmdl::parse`, which refuses 619 real
/// corpus records — including every creature model in `CreatureModelsHQ`
/// (`0x40627100`) — because their material-info tail names shader-data id
/// `0x218`, whose byte size no table holds. Those records still carry
/// complete, validated geometry behind the mesh table, which is what
/// `spore-assets`' recovering parse produced.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MaterialRoute {
    /// `resolve_model_record` answered. Also yields the corpus regularity
    /// checks ([`ModelMaterials::observed_shape`]).
    Strict,
    /// The strict route refused, and the roles were composed from the same
    /// public `spore-material` functions applied to the recovering parse's
    /// model: [`spore_material::resolve_model_textures`] +
    /// [`spore_material::model_materials`].
    ///
    /// Not a second algorithm — the identical three steps
    /// `resolve_model_record` itself performs, in the same order, minus the
    /// re-read that cannot succeed. `tests/real_corpus.rs` asserts the two
    /// routes agree field for field on a record the strict route accepts.
    Recovering,
}

impl MaterialRoute {
    /// The canonical spelling, for logs and reports.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Strict => "strict",
            Self::Recovering => "recovering",
        }
    }
}

/// Every role a model resolved to, plus the per-mesh cut.
#[derive(Debug, Clone, PartialEq)]
pub struct ModelMaterials {
    /// One entry per flat texture reference, in the record's own order.
    pub textures: Vec<MaterialTexture>,
    /// One entry per mesh, in the record's own order.
    pub meshes: Vec<MeshSlots>,
    /// How the cut was made.
    pub scope: BindingScope,
    /// Which route produced this table.
    pub route: MaterialRoute,
    /// The strict route's refusal, when it refused.
    pub strict_refusal: Option<String>,
    /// The corpus set-shape check. `None` on the recovering route, where the
    /// observation was not made.
    ///
    /// `None` is an absence of the observation, never a "shape that did not
    /// match": those are [`TexSetShape::Divergent`] and
    /// [`TexSetShape::Unavailable`] respectively.
    pub observed_shape: Option<TexSetShape>,
    /// The two independent ordinal statements agreeing. `None` on the
    /// recovering route.
    pub ordinal_agreement: Option<OrdinalAgreement>,
}

/// One mesh's slots.
#[derive(Debug, Clone, PartialEq)]
pub struct MeshSlots {
    /// The mesh's index in the record's mesh table.
    pub mesh_index: u32,
    /// The material id the record associates with this mesh.
    ///
    /// A bare word. `spore_material::material_name` returns `None` for every id
    /// and this engine does not substitute a colour for it.
    pub material_id: u32,
    /// The references in this mesh's slice, in order.
    pub slots: Vec<spore_gmdl::GmdlTextureRef>,
}

impl ModelMaterials {
    /// Builds the table from `spore-material`'s strict answer.
    pub fn from_model_textures(resolved: &ModelTextures) -> Self {
        Self {
            textures: resolved
                .textures
                .iter()
                .zip(resolved.model.texture_refs.iter())
                .map(|(entry, reference)| MaterialTexture {
                    reference: Some(*reference),
                    resolved: entry.as_ref().ok().cloned(),
                    refusal: entry.as_ref().err().map(ToString::to_string),
                })
                .collect(),
            meshes: resolved.meshes.iter().map(mesh_slots).collect(),
            scope: resolved.scope,
            route: MaterialRoute::Strict,
            strict_refusal: None,
            observed_shape: Some(resolved.observed_shape.clone()),
            ordinal_agreement: Some(resolved.ordinal_agreement),
        }
    }

    /// Builds the table from the recovering parse's model.
    ///
    /// The steps are `resolve_model_record`'s, in its order:
    /// [`spore_material::resolve_model_textures`] then
    /// [`spore_material::model_materials`]. What is **not** reproduced is
    /// `observe_shape` / `observe_ordinal_agreement`, which are private to
    /// `spore-material`; those observations are therefore reported as `None`
    /// rather than re-derived here. Inventing a second shape check is exactly
    /// the drift this workspace keeps paying for.
    pub fn from_recovered_model(
        model: &GmdlModel,
        results: Vec<Result<ResolvedTexture, MaterialError>>,
        strict_refusal: Option<String>,
    ) -> Self {
        let meshes = spore_material::model_materials(model);
        let scope = meshes
            .first()
            .map_or(BindingScope::WholeModel, |mesh| mesh.scope);
        // The scope is a property of the model, not of one mesh, so every mesh
        // states the same one. If that ever stopped being true the engine would
        // be about to divide a list by a scope it read off mesh 0 while binding
        // mesh 1's slots, so the assumption is asserted rather than trusted.
        debug_assert!(
            meshes.iter().all(|mesh| mesh.scope == scope),
            "every mesh of one model must report the same BindingScope"
        );
        let textures = results
            .into_iter()
            .zip(model.texture_refs.iter())
            .map(|(entry, reference)| match entry {
                Ok(texture) => MaterialTexture {
                    reference: Some(*reference),
                    resolved: Some(texture),
                    refusal: None,
                },
                Err(error) => MaterialTexture {
                    reference: Some(*reference),
                    resolved: None,
                    refusal: Some(error.to_string()),
                },
            })
            .collect();
        Self {
            textures,
            meshes: meshes.iter().map(mesh_slots).collect(),
            scope,
            route: MaterialRoute::Recovering,
            strict_refusal,
            observed_shape: None,
            ordinal_agreement: None,
        }
    }

    /// Resolves a model's materials, taking the documented route when it works.
    ///
    /// On the strict route the record is read and its geometry decoded a second
    /// time — a real cost, paid only where it is not needed for the renderer,
    /// because the recovering route never pays it.
    pub fn resolve(
        store: &spore_assets::ContentStore,
        key: &ResourceKey,
        model: &LoadedModel,
        assumed_type: u32,
    ) -> Self {
        // The strict refusal is *the finding*, not an error to hide: it is why
        // the second route exists.
        let strict_refusal = match spore_material::resolve_model_record(store, key, assumed_type) {
            Ok(resolved) => return Self::from_model_textures(&resolved),
            Err(error) => Some(error.to_string()),
        };
        let Some(gmdl) = model.gmdl.as_ref() else {
            return Self::empty(strict_refusal);
        };
        let results = spore_material::resolve_model_textures(store, gmdl, assumed_type);
        Self::from_recovered_model(gmdl, results, strict_refusal)
    }

    /// A table with no references at all, for a record with no `gmdl`.
    pub fn empty(strict_refusal: Option<String>) -> Self {
        Self {
            textures: Vec::new(),
            meshes: Vec::new(),
            scope: BindingScope::WholeModel,
            route: MaterialRoute::Recovering,
            strict_refusal,
            observed_shape: None,
            ordinal_agreement: None,
        }
    }

    /// How many references resolved.
    pub fn resolved_count(&self) -> usize {
        self.textures
            .iter()
            .filter(|t| t.resolved.is_some())
            .count()
    }

    /// How many references were attempted.
    pub fn attempted(&self) -> usize {
        self.textures.len()
    }

    /// The refusals, in encounter order.
    pub fn refusals(&self) -> impl Iterator<Item = (&Option<spore_gmdl::GmdlTextureRef>, &String)> {
        self.textures
            .iter()
            .filter_map(|t| t.refusal.as_ref().map(|r| (&t.reference, r)))
    }

    /// The flat position of mesh `mesh_index`'s `within`-th slot.
    ///
    /// The slot's **position in its own mesh's slice**, never its stated
    /// ordinal — under [`BindingScope::WholeModel`] a slot carries no ordinal,
    /// and under [`BindingScope::PerMeshBlock`] the two coincide because a
    /// block's ordinals run `0..refs_per_mesh`.
    pub fn flat_index(&self, mesh_index: u32, within: usize) -> usize {
        match self.scope {
            BindingScope::PerMeshBlock { refs_per_mesh } => {
                mesh_index as usize * refs_per_mesh + within
            }
            // `Undetermined` is the "the record contradicted the cut" case: the
            // reference list is short, every mesh holds all of it, and a slot's
            // flat position is its own index. `spore_material::BindingScope`
            // states this as `applies_whole_set()`; it is the same arithmetic as
            // `WholeModel`, and folding the two would hide the contradiction
            // that the variant exists to keep visible.
            BindingScope::WholeModel | BindingScope::Undetermined { .. } => within,
        }
    }

    /// The entry at a flat position, or `None` when the index is out of range.
    pub fn entry(&self, flat: usize) -> Option<&MaterialTexture> {
        self.textures.get(flat)
    }

    /// The role at a flat position.
    pub fn role_of(&self, flat: usize) -> TextureRole {
        self.entry(flat)
            .map_or(TextureRole::Unidentified, MaterialTexture::role)
    }

    /// Every mesh's roles, in slot order.
    ///
    /// The same arithmetic as [`spore_material::ModelTextures::roles`], so the
    /// two must agree; see [`tests::the_roles_table_reproduces_the_reference_cut`].
    pub fn roles(&self) -> Vec<Vec<TextureRole>> {
        self.meshes
            .iter()
            .map(|mesh| {
                (0..mesh.slots.len())
                    .map(|within| self.role_of(self.flat_index(mesh.mesh_index, within)))
                    .collect()
            })
            .collect()
    }

    /// One material plan per mesh, in record order.
    pub fn plans(&self) -> Vec<MeshMaterialPlan> {
        self.meshes.iter().map(|mesh| self.plan_for(mesh)).collect()
    }

    /// One mesh's plan.
    pub fn plan_for(&self, mesh: &MeshSlots) -> MeshMaterialPlan {
        let mut plan = MeshMaterialPlan {
            mesh_index: mesh.mesh_index,
            material_id: mesh.material_id,
            // `spore_material::material_name` is `None` for every id and this
            // engine does not invent one.
            material_name: None,
            scope: self.scope,
            normal_map: None,
            masks: Vec::new(),
            unidentified: Vec::new(),
            ordinal_conflicts: 0,
        };
        for (within, reference) in mesh.slots.iter().enumerate() {
            let flat = self.flat_index(mesh.mesh_index, within);
            self.bind_slot(&mut plan, flat, within, *reference);
        }
        plan
    }

    /// Decides what one slot binds, and records what it did not.
    ///
    /// The order of the checks is the order of the trust:
    ///
    /// 1. **does a flat entry exist at all** — if not, that is an absence and it
    ///    is recorded;
    /// 2. **is that entry this slot's own reference** — if not, the cut is wrong
    ///    for this mesh and nothing binds. This is the invariant
    ///    [`Self::flat_index`] exists to protect, enforced at the point of
    ///    binding rather than assumed from the arithmetic: a slot is only ever
    ///    bound to the texture the record named *for that slot*;
    /// 3. **does the derived ordinal agree with the group id's low byte** — a
    ///    disagreement is counted, never resolved in favour of either statement;
    /// 4. **what does the record publish** — and this is the only thing that
    ///    decides a binding. A record that publishes a normal triple binds a
    ///    normal map; one that publishes mask channels binds those; one that
    ///    publishes neither binds nothing and says so.
    fn bind_slot(
        &self,
        plan: &mut MeshMaterialPlan,
        flat: usize,
        within: usize,
        reference: spore_gmdl::GmdlTextureRef,
    ) {
        let Some(entry) = self.entry(flat) else {
            // No such flat index. Recorded as an unbound surface with no key,
            // which is the honest shape of "the record referenced something we
            // cannot line up".
            plan.unidentified.push(UnidentifiedSurface {
                reference: Some(reference),
                key: None,
                flat,
                role: TextureRole::Unidentified,
                reason: UnboundReason::NoFlatEntry,
            });
            return;
        };
        if entry.reference != Some(reference) {
            // The engine's own last line of defence. Every arm of
            // `BindingScope` that keeps whole sets passes this trivially; a
            // `PerMeshBlock` cut reaches it only when the division names a
            // reference the mesh did not carry, which is precisely "a texture
            // from another mesh's block". Nothing binds.
            plan.unidentified.push(UnidentifiedSurface {
                reference: Some(reference),
                key: entry.key(),
                flat,
                role: entry.role(),
                reason: UnboundReason::EntryReferenceMismatch,
            });
            return;
        }
        if let Some(ordinal) = stated_ordinal(self.scope, within) {
            if ordinal != reference.group_id & 0xFF {
                plan.ordinal_conflicts += 1;
            }
        }
        let Some(texture) = entry.resolved.as_ref() else {
            plan.unidentified.push(UnidentifiedSurface {
                reference: Some(reference),
                key: None,
                flat,
                role: TextureRole::Unidentified,
                reason: UnboundReason::Refused,
            });
            return;
        };

        // Bound-or-reported: once either happens, this slot is accounted for and
        // a second "nothing bound" report must not follow it.
        let mut accounted = false;
        match NormalMapPlan::from_channels(texture) {
            Ok(Some(plane)) if plan.normal_map.is_none() => {
                plan.normal_map = Some(plane);
                accounted = true;
            }
            Ok(Some(_)) => {
                // Two normal maps on one mesh is not a corpus shape, so the
                // first wins and the fact is stated rather than the second being
                // silently preferred.
                plan.unidentified.push(UnidentifiedSurface {
                    reference: Some(reference),
                    key: Some(texture.key),
                    flat,
                    role: texture.role,
                    reason: UnboundReason::DuplicateNormalMap,
                });
                accounted = true;
            }
            // Not a normal map at all. Not a finding on its own: most records
            // are not.
            Ok(None) => {}
            Err(reason) => {
                plan.unidentified.push(UnidentifiedSurface {
                    reference: Some(reference),
                    key: Some(texture.key),
                    flat,
                    role: texture.role,
                    reason,
                });
                accounted = true;
            }
        }
        for (channel_index, binding) in texture.channels().iter().enumerate() {
            if binding.role != ChannelRole::MaskChannel {
                continue;
            }
            let (width, height) = texture
                .layer(binding.layer_index)
                .map_or((0, 0), |layer| (layer.width, layer.height));
            plan.masks.push(MaskChannelPlan {
                key: texture.key,
                channel_index,
                layer_index: binding.layer_index,
                lane: binding.lane,
                width,
                height,
            });
            accounted = true;
        }
        if !accounted {
            // The record decoded and published nothing this build can bind.
            // Its role and statistics are on the entry and in the report's role
            // census; nothing is guessed at here.
            plan.unidentified.push(UnidentifiedSurface {
                reference: Some(reference),
                key: Some(texture.key),
                flat,
                role: texture.role,
                reason: UnboundReason::NoPublishedChannel,
            });
        }
    }

    /// Every distinct role in the model, with counts, in slot order of first
    /// appearance.
    pub fn role_census(&self) -> Vec<(TextureRole, usize)> {
        let mut counts: Vec<(TextureRole, usize)> = Vec::new();
        for entry in &self.textures {
            let role = entry.role();
            match counts.iter_mut().find(|(seen, _)| *seen == role) {
                Some((_, count)) => *count += 1,
                None => counts.push((role, 1)),
            }
        }
        counts
    }
}

/// The ordinal a slot would carry under a scope, if it carries one.
///
/// `Undetermined` states `None`: the cut was refused precisely *because* the
/// record's ordinals contradict a derived one, so manufacturing one here would
/// re-create the defect the variant was added to stop.
fn stated_ordinal(scope: BindingScope, within: usize) -> Option<u32> {
    match scope {
        BindingScope::PerMeshBlock { .. } => Some(within as u32),
        BindingScope::WholeModel | BindingScope::Undetermined { .. } => None,
    }
}

fn mesh_slots(mesh: &spore_material::MeshMaterial) -> MeshSlots {
    MeshSlots {
        mesh_index: mesh.mesh_index,
        material_id: mesh.material_id,
        slots: mesh.slots.iter().map(|slot| slot.reference).collect(),
    }
}

/// One mesh's normal map, with the evidence behind the binding.
#[derive(Debug, Clone, PartialEq)]
pub struct NormalMapPlan {
    /// The record's identity, with the **assumed** type word substituted in.
    pub key: ResourceKey,
    /// Which of the record's layers the normal's three channels came from.
    ///
    /// **`0` for every record measured**: `spore-texture` proved there is exactly
    /// one mip chain per raster record. It is a field rather than a constant
    /// because `channels()` carries a layer index and this module reads what it
    /// is given rather than assuming.
    pub layer_index: usize,
    /// Width of the base mip.
    pub width: u32,
    /// Height of the base mip.
    pub height: u32,
    /// Mip levels the record carries. Only the base mip is uploaded.
    pub mip_count: usize,
    /// Where each component lives on disk.
    pub layout: NormalLayout,
    /// The file lane of each of X, Y and Z, taken from the record's own
    /// `channels()` rather than assumed.
    pub lanes: [usize; 3],
    /// Mean of each component lane over the base mip, as published by
    /// `spore-material`. The evidence for the binding, kept beside it.
    pub component_means: [f64; 3],
    /// The record's own first texel, in file order. Useful when comparing a
    /// rendered frame against the bytes.
    pub first_texel: [u8; 4],
}

impl NormalMapPlan {
    /// Builds a normal-map binding from a record's own published channels.
    ///
    /// A thin wrapper over [`Self::from_bindings`], which is the part with the
    /// decisions in it.
    pub fn from_channels(texture: &ResolvedTexture) -> Result<Option<Self>, UnboundReason> {
        Self::from_bindings(&texture.channels(), texture)
    }

    /// Builds a normal-map binding from `bindings`, read out of `texture`.
    ///
    /// Three outcomes, and the difference between them is the whole point:
    ///
    /// * `Ok(Some(plan))` — `bindings` publish a complete normal triple whose
    ///   lanes are an established layout ([`NormalLayout::ZFirst`]). This is the
    ///   only path that binds a normal map, and it never consults the role
    ///   variant: a role this build has not seen that publishes
    ///   `NormalZ`/`NormalX`/`NormalY` in the right lanes binds correctly with
    ///   no line changed here, and a role *called* normal that publishes
    ///   something else does not.
    /// * `Ok(None)` — nothing here is a normal. **Not** a finding: most records
    ///   in this corpus are not.
    /// * `Err(reason)` — the bindings publish normal components this build
    ///   cannot place, so binding would mean guessing at a permutation. It binds
    ///   nothing and is reported.
    ///
    /// Taking the bindings as an argument rather than reading them off the
    /// texture is what makes the two `Err` arms reachable from a test at all:
    /// `spore-material` publishes exactly one normal layout, so through
    /// `channels()` alone they are unreachable defensive code.
    pub fn from_bindings(
        bindings: &[spore_material::ChannelBinding],
        texture: &ResolvedTexture,
    ) -> Result<Option<Self>, UnboundReason> {
        // Per layer, not per record: a record that published a normal triple in
        // two layers would be two planes, and taking whichever binding came last
        // would be an arbitrary choice between them.
        let mut chosen: Option<(usize, [usize; 3], [f64; 3])> = None;
        let mut incomplete: Option<usize> = None;
        for layer_index in 0..texture.layers.len().max(1) {
            let mut lanes = [usize::MAX; 3];
            let mut means = [f64::NAN; 3];
            let mut published = 0usize;
            for binding in bindings {
                if binding.layer_index != layer_index {
                    continue;
                }
                let component = match binding.role {
                    ChannelRole::NormalX => 0,
                    ChannelRole::NormalY => 1,
                    ChannelRole::NormalZ => 2,
                    _ => continue,
                };
                lanes[component] = binding.lane.index();
                means[component] = texture.statistics.mean(binding.lane).unwrap_or(f64::NAN);
                published += 1;
            }
            if published == 0 {
                continue;
            }
            if published < 3 {
                // A partial triple cannot be swizzled: two of three components
                // leave the third undefined.
                incomplete = incomplete.or(Some(layer_index));
                continue;
            }
            chosen = Some((layer_index, lanes, means));
            break;
        }
        let Some((layer_index, lanes, means)) = chosen else {
            return match incomplete {
                Some(_) => Err(UnboundReason::IncompleteNormalTriple),
                None => Ok(None),
            };
        };
        if lanes != NormalLayout::ZFirst.lanes() {
            // The record publishes a normal in some order this build has not
            // established. Adding the layout to [`NormalLayout`] would be a claim
            // that a record with that order exists; guessing the permutation
            // would be a wrong normal map that looks almost right.
            return Err(UnboundReason::UnestablishedNormalLayout);
        }
        let mut first_texel = [0u8; 4];
        if texture.image.pixels.len() >= 4 {
            first_texel.copy_from_slice(&texture.image.pixels[..4]);
        }
        Ok(Some(Self {
            key: texture.key,
            layer_index,
            width: texture.image.width,
            height: texture.image.height,
            mip_count: texture.mip_count,
            layout: NormalLayout::ZFirst,
            lanes,
            component_means: means,
            first_texel,
        }))
    }

    /// Whether the record's own statistics name lane 0 as Z.
    ///
    /// A cross-check on the binding, computed from the same numbers
    /// `spore-material` graded the role with. `true` says "lane 0's mean is far
    /// above both lateral lanes'"; a plan whose lanes say `[0, 1, 2]` while this
    /// says `false` is a contradiction, and `tests` asserts it never happens.
    pub fn lane_zero_is_z(&self) -> bool {
        let [x, y, z] = self.component_means;
        x.is_finite() && y.is_finite() && z.is_finite() && z - x.max(y) >= 64.0
    }

    /// The swizzle that turns this record's bytes into `(X, Y, Z)`.
    ///
    /// Always [`NormalLayout::ZFirst`]'s, and named as a value so a log line
    /// can state it instead of leaving it to be inferred from the code.
    pub fn swizzle(&self) -> &'static str {
        self.layout.as_str()
    }

    /// One line for a log, with the evidence beside the verdict.
    pub fn describe(&self) -> String {
        format!(
            "{} {}x{} mips={} layout={} lanes=[x:{},y:{},z:{}] means=[{:.2},{:.2},{:.2}] first_texel={:?}",
            self.key, self.width, self.height, self.mip_count, self.swizzle(),
            self.lanes[0], self.lanes[1], self.lanes[2],
            self.component_means[0], self.component_means[1], self.component_means[2],
            self.first_texel,
        )
    }
}

/// One channel `spore-material` published as [`ChannelRole::MaskChannel`].
///
/// **Its purpose is unknown** and this engine does not name one. What the
/// corpus establishes is the *lane* the value sits in, so that is what is
/// recorded — enough to read the record's bytes back out of the surface, and
/// nothing more.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MaskChannelPlan {
    /// The record's identity, with the assumed type word substituted in.
    pub key: ResourceKey,
    /// The binding's position in [`ResolvedTexture::channels`], in
    /// `(layer, lane)` order.
    pub channel_index: usize,
    /// Which of the record's layers the channel belongs to. **`0` for every
    /// record measured**: `spore-texture` proved there is exactly one mip chain
    /// per raster record, so the earlier per-layer loop was iterating an
    /// artefact of a decoder that charged 8 bytes per 4×4 block *and* divided
    /// by a 16-byte layer header — two errors that cancelled exactly when the
    /// geometry lined up.
    pub layer_index: usize,
    /// The byte of the texel the value lives in, as published.
    pub lane: Lane,
    /// Width of the base mip.
    pub width: u32,
    /// Height of the base mip.
    pub height: u32,
}

/// Why a slot fed no material channel.
///
/// Seven reasons, kept apart because each is a different statement about the
/// record. Collapsing them into a count is what made `unidentified=1` look like
/// a decoding failure when it was usually a decoder that had just been fixed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UnboundReason {
    /// The flat index the cut produced is past the end of the reference list.
    NoFlatEntry,
    /// The record was looked up and refused, or could not be found.
    Refused,
    /// The entry at that flat index carries a **different reference** than the
    /// slot, so the cut is not this mesh's block. Nothing binds.
    EntryReferenceMismatch,
    /// A second normal map on one mesh, which is not a corpus shape.
    DuplicateNormalMap,
    /// The record publishes some but not all of a normal's three components.
    IncompleteNormalTriple,
    /// The record publishes a normal in an order this build has not
    /// established.
    UnestablishedNormalLayout,
    /// The record decoded and published nothing this build can bind.
    NoPublishedChannel,
}

impl UnboundReason {
    /// The canonical spelling, for logs and reports.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NoFlatEntry => "no-flat-entry",
            Self::Refused => "refused",
            Self::EntryReferenceMismatch => "entry-reference-mismatch",
            Self::DuplicateNormalMap => "duplicate-normal-map",
            Self::IncompleteNormalTriple => "incomplete-normal-triple",
            Self::UnestablishedNormalLayout => "unestablished-normal-layout",
            Self::NoPublishedChannel => "no-published-channel",
        }
    }
}

impl std::fmt::Display for UnboundReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A slot that fed no material channel.
///
/// Named by position and by the reason, never by a purpose:
/// `spore_material::claims::MASK_LAYER_PURPOSE` is an `UNKNOWN` non-finding, so
/// calling a slot "specular", "gloss" or "occlusion" would be a claim about a
/// record nobody has read a shader for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnidentifiedSurface {
    /// The reference the record carried.
    pub reference: Option<spore_gmdl::GmdlTextureRef>,
    /// The record's identity, when it resolved.
    pub key: Option<ResourceKey>,
    /// The flat position it was cut from.
    pub flat: usize,
    /// The role `spore-material` published for it, so a report can say *what the
    /// record was* as well as *why nothing bound it*. `Unidentified` for a
    /// refusal and for an absent entry alike: neither is a role the record has.
    pub role: TextureRole,
    /// Why nothing bound.
    pub reason: UnboundReason,
}

/// One mesh's material, as the corpus describes it.
#[derive(Debug, Clone, PartialEq)]
pub struct MeshMaterialPlan {
    /// The mesh this plan belongs to.
    pub mesh_index: u32,
    /// The material id the record associated with the mesh. A bare word.
    pub material_id: u32,
    /// Always `None`. No registry maps a material id to a name.
    pub material_name: Option<&'static str>,
    /// How the per-mesh cut was made. `is_inferred()` is `true` for **every**
    /// variant, `Undetermined` included — there is no proven binding in this
    /// build.
    pub scope: BindingScope,
    /// The tangent-space normal map, when one of this mesh's slots published a
    /// normal triple in an established layout.
    pub normal_map: Option<NormalMapPlan>,
    /// Every channel this mesh's slots published as
    /// [`ChannelRole::MaskChannel`], in slot then channel order.
    ///
    /// **Zero for every record in the current corpus**: with `DXT5` read as BC3
    /// rather than as an 8-byte format, no raster record satisfies the old
    /// layered-mask signature, so `spore-material` publishes
    /// [`ChannelRole::UnidentifiedLane`] for all four of its lanes. The
    /// machinery stays, because it binds whatever the next role derivation does
    /// publish — with no line changed here — and because a plan that could only
    /// ever be empty would be untestable.
    pub masks: Vec<MaskChannelPlan>,
    /// Slots that fed nothing, each with the reason. A slot that bound a mask
    /// channel is **not** in here.
    pub unidentified: Vec<UnidentifiedSurface>,
    /// Slots where the derived ordinal disagreed with the group id's low byte.
    pub ordinal_conflicts: usize,
}

impl MeshMaterialPlan {
    /// Whether any surface fed a Bevy material channel.
    pub fn has_any_surface(&self) -> bool {
        self.normal_map.is_some() || !self.masks.is_empty()
    }

    /// The unbound slots' reasons, with counts, in encounter order.
    pub fn unbound_reasons(&self) -> Vec<(UnboundReason, usize)> {
        let mut counts: Vec<(UnboundReason, usize)> = Vec::new();
        for surface in &self.unidentified {
            match counts.iter_mut().find(|(seen, _)| *seen == surface.reason) {
                Some((_, count)) => *count += 1,
                None => counts.push((surface.reason, 1)),
            }
        }
        counts
    }

    /// One line for a log.
    pub fn describe(&self) -> String {
        let reasons = self
            .unbound_reasons()
            .iter()
            .map(|(reason, count)| format!("{reason}:{count}"))
            .collect::<Vec<_>>()
            .join(",");
        format!(
            "mesh {} material=0x{:08x} scope={} normal_maps={} mask_channels={} \
             unbound={} unbound_reasons={} ordinal_conflicts={} inferred={}",
            self.mesh_index,
            self.material_id,
            self.scope.as_str(),
            usize::from(self.normal_map.is_some()),
            self.masks.len(),
            self.unidentified.len(),
            if reasons.is_empty() { "none" } else { &reasons },
            self.ordinal_conflicts,
            self.scope.is_inferred(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat(pixels: &[u8]) -> Vec<u8> {
        pixels.to_vec()
    }

    #[test]
    fn the_flat_normal_swizzles_to_blue() {
        // The seven constant `0x15` records hold `(255, 128, 128)` in every
        // texel, which is the encoding of the flat normal (0, 0, 1). If the
        // swizzle is right, the OUTPUT blue byte must be 255.
        let out = swizzle_z_first_to_xyz(&flat(&[255, 128, 128, 0]));
        assert_eq!(out, vec![128, 128, 255, 255]);
        assert_eq!(
            out[2], 255,
            "Z must land in blue; a wrong swizzle looks almost right"
        );
    }

    #[test]
    fn the_swizzle_places_each_component_at_its_conventional_index() {
        // `components[byte] = component`, so component `c` is read from file
        // byte `byte_of[c]`.
        let components = lane_components(NormalLayout::ZFirst);
        let byte_of = [
            components.iter().position(|c| *c == 0).unwrap(),
            components.iter().position(|c| *c == 1).unwrap(),
            components.iter().position(|c| *c == 2).unwrap(),
        ];
        assert_eq!(byte_of, [1, 2, 0], "file byte 0 holds component 2 (Z)");

        // The measured creature record's first texel: Z=255, X=128, Y=126.
        let file = [255u8, 128, 126, 181];
        let out = swizzle_z_first_to_xyz(&flat(&file));
        // After the swizzle the component index IS the output byte index, which
        // is what Bevy samples: R = X, G = Y, B = Z.
        for component in 0..3usize {
            assert_eq!(
                out[component], file[byte_of[component]],
                "component {component}"
            );
        }
        assert_eq!(out, vec![128, 126, 255, 255]);
    }

    #[test]
    fn the_swizzle_is_not_the_identity_on_any_asymmetric_texel() {
        let out = swizzle_z_first_to_xyz(&flat(&[10, 20, 30, 40]));
        assert_ne!(out[..3], [10, 20, 30]);
        assert_eq!(out[..3], [20, 30, 10]);
    }

    #[test]
    fn the_swizzled_blue_lane_carries_the_file_first_lane() {
        // Over the corpus the FIRST file lane's mean is 232.51 .. 255.00. After
        // the swizzle it must be the BLUE lane's mean, not the red one. This is
        // the measurement that catches a swizzle written "backwards".
        let pixels: Vec<u8> = (0..64u32)
            .flat_map(|i| [(250 - i % 8) as u8, 128, 128, 200])
            .collect();
        let out = swizzle_z_first_to_xyz(&pixels);
        let mean = |lane: usize| -> f64 {
            let bytes: Vec<u8> = out.iter().skip(lane).step_by(4).copied().collect();
            bytes.iter().map(|b| f64::from(*b)).sum::<f64>() / bytes.len() as f64
        };
        let file_first: f64 = (0..64u32)
            .map(|i| f64::from((250 - i % 8) as u8))
            .sum::<f64>()
            / 64.0;
        assert!(
            (mean(2) - file_first).abs() < 1e-9,
            "blue must carry file byte 0"
        );
        assert!(
            mean(0) + 100.0 < file_first,
            "red must not carry Z: {} vs {file_first}",
            mean(0)
        );
        // And Z is above the 224 floor the role check uses, which is the whole
        // reason the lane is identified as Z.
        assert!(mean(2) >= 224.0, "{}", mean(2));
    }

    #[test]
    fn the_swizzle_preserves_length_and_carries_a_short_trailer() {
        assert_eq!(swizzle_z_first_to_xyz(&[1, 2, 3, 4, 5, 6, 7, 8]).len(), 8);
        // Not texel-aligned: the tail is carried rather than dropped, so a
        // mis-sized buffer is visible instead of quietly shortened.
        let ragged = [1u8, 2, 3, 4, 5, 6];
        assert_eq!(swizzle_z_first_to_xyz(&ragged), vec![2, 3, 1, 255, 5, 6]);
    }

    #[test]
    fn a_lane_extraction_picks_the_alpha_plane() {
        let rgba = [9u8, 8, 7, 6, 5, 4, 3, 2];
        assert_eq!(extract_lane(&rgba, 3), vec![6, 2]);
        assert_eq!(extract_lane(&rgba, 0), vec![9, 5]);
        assert!(extract_lane(&rgba, 3).iter().all(|v| *v > 1));
    }

    #[test]
    fn an_empty_table_reports_no_references_and_does_not_invent_a_shape() {
        let materials = ModelMaterials::empty(None);
        assert_eq!(materials.attempted(), 0);
        assert_eq!(materials.resolved_count(), 0);
        assert!(
            materials.observed_shape.is_none(),
            "absence of an observation"
        );
        assert!(materials.plans().is_empty());
        assert_eq!(materials.roles(), Vec::<Vec<TextureRole>>::new());
    }

    #[test]
    fn the_flat_index_follows_the_documented_cut() {
        let materials = ModelMaterials {
            scope: BindingScope::PerMeshBlock { refs_per_mesh: 3 },
            ..ModelMaterials::empty(None)
        };
        assert_eq!(materials.flat_index(0, 0), 0);
        assert_eq!(materials.flat_index(0, 2), 2);
        assert_eq!(materials.flat_index(1, 0), 3);
        assert_eq!(materials.flat_index(2, 2), 8);
        let whole = ModelMaterials {
            scope: BindingScope::WholeModel,
            ..ModelMaterials::empty(None)
        };
        for mesh in 0..4u32 {
            for within in 0..3usize {
                assert_eq!(whole.flat_index(mesh, within), within);
            }
        }
    }

    #[test]
    fn both_scopes_report_their_inference() {
        // There is no proven binding in this build, so `is_inferred()` must be
        // `true` for every variant or a caller would start trusting one.
        assert!(BindingScope::PerMeshBlock { refs_per_mesh: 3 }.is_inferred());
        assert!(BindingScope::WholeModel.is_inferred());
        assert!(BindingScope::Undetermined {
            collected: 2,
            contradicted: 2
        }
        .is_inferred());
    }

    #[test]
    fn an_undetermined_scope_hands_every_mesh_the_whole_set_and_states_no_ordinal() {
        // The documented creature record: the material-info walk stops at the
        // undocumented shader-data id `0x218`, so the flat list is short, the
        // even division is wrong, and `spore-material` refuses the cut. Handing
        // every mesh the whole short list is the conservative answer; slicing it
        // would bind mesh 1 to a texture out of mesh 0's block.
        let materials = ModelMaterials {
            scope: BindingScope::Undetermined {
                collected: 2,
                contradicted: 2,
            },
            meshes: vec![MeshSlots {
                mesh_index: 0,
                material_id: 1,
                slots: vec![
                    spore_gmdl::GmdlTextureRef {
                        instance_id: 1,
                        group_id: 0x4062_2900,
                    },
                    spore_gmdl::GmdlTextureRef {
                        instance_id: 1,
                        group_id: 0x4062_2901,
                    },
                ],
            }],
            ..ModelMaterials::empty(None)
        };
        assert_eq!(materials.flat_index(0, 1), 1);
        assert_eq!(materials.flat_index(0, 0), 0);
        // A refused cut states no ordinal, so the engine cannot manufacture a
        // conflict out of the refusal itself.
        let plan = materials.plans()[0].clone();
        assert_eq!(plan.ordinal_conflicts, 0);
        assert_eq!(materials.scope.as_str(), "undetermined");
        assert!(!materials.scope.divides_by_mesh());
        assert!(materials.scope.applies_whole_set());
    }

    #[test]
    fn an_out_of_range_flat_index_is_unidentified_and_not_an_entry() {
        let materials = ModelMaterials::empty(None);
        assert!(materials.entry(0).is_none());
        assert_eq!(materials.role_of(0), TextureRole::Unidentified);
    }

    #[test]
    fn a_slot_with_no_lineable_entry_is_recorded_rather_than_dropped() {
        let materials = ModelMaterials {
            scope: BindingScope::PerMeshBlock { refs_per_mesh: 1 },
            meshes: vec![MeshSlots {
                mesh_index: 0,
                material_id: 0x40DE_AD00,
                slots: vec![spore_gmdl::GmdlTextureRef {
                    instance_id: 1,
                    group_id: 0x4063_2900,
                }],
            }],
            ..ModelMaterials::empty(None)
        };
        let plans = materials.plans();
        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].unidentified.len(), 1);
        assert!(!plans[0].has_any_surface());
    }

    #[test]
    fn an_ordinal_conflict_is_counted_not_resolved() {
        // The creature record measured on 2026-10-04: `refs_per_mesh` is 1, so
        // mesh 1's slot is derived ordinal 0 -- while the referenced record's
        // group id low byte says 1. Two independent statements disagree, and the
        // engine reports the disagreement rather than picking one.
        let reference = spore_gmdl::GmdlTextureRef {
            instance_id: 0x067C_79D2,
            group_id: 0x4062_2901,
        };
        let materials = ModelMaterials {
            scope: BindingScope::PerMeshBlock { refs_per_mesh: 1 },
            meshes: vec![MeshSlots {
                mesh_index: 0,
                material_id: 0xD7BE_35F9,
                slots: vec![reference],
            }],
            textures: vec![MaterialTexture {
                reference: Some(reference),
                resolved: None,
                refusal: Some("synthetic".into()),
            }],
            ..ModelMaterials::empty(None)
        };
        assert_eq!(materials.plans()[0].ordinal_conflicts, 1);

        // The agreeing case: same record, group low byte 0.
        let agreeing = spore_gmdl::GmdlTextureRef {
            instance_id: 0x067C_79D2,
            group_id: 0x4062_2900,
        };
        let materials = ModelMaterials {
            meshes: vec![MeshSlots {
                mesh_index: 0,
                material_id: 0xD7BE_35F9,
                slots: vec![agreeing],
            }],
            textures: vec![MaterialTexture {
                reference: Some(agreeing),
                resolved: None,
                refusal: Some("synthetic".into()),
            }],
            ..ModelMaterials::empty(None)
        };
        assert_eq!(materials.plans()[0].ordinal_conflicts, 0);
    }

    #[test]
    fn a_whole_model_scope_carries_no_ordinal_to_conflict_with() {
        let materials = ModelMaterials {
            scope: BindingScope::WholeModel,
            meshes: vec![MeshSlots {
                mesh_index: 0,
                material_id: 1,
                slots: vec![spore_gmdl::GmdlTextureRef {
                    instance_id: 1,
                    group_id: 0x4063_2902,
                }],
            }],
            ..ModelMaterials::empty(None)
        };
        assert_eq!(materials.plans()[0].ordinal_conflicts, 0);
    }

    #[test]
    fn a_material_id_never_gets_a_name() {
        let plan = MeshMaterialPlan {
            mesh_index: 0,
            material_id: 0x407D_FDDB,
            material_name: None,
            scope: BindingScope::WholeModel,
            normal_map: None,
            masks: Vec::new(),
            unidentified: Vec::new(),
            ordinal_conflicts: 0,
        };
        assert_eq!(plan.material_name, None);
        assert!(spore_material::material_name(0x407D_FDDB).is_none());
        assert!(plan.describe().contains("0x407dfddb"));
        assert!(!plan.describe().contains("name"));
    }

    #[test]
    fn lane_zero_is_z_reads_the_published_statistics() {
        let good = NormalMapPlan {
            key: ResourceKey::new(1, 2, 3),
            layer_index: 0,
            width: 256,
            height: 256,
            mip_count: 9,
            layout: NormalLayout::ZFirst,
            lanes: [1, 2, 0],
            // The measured creature record: means [249.33, 128.62, 128.01].
            component_means: [128.62, 128.01, 249.33],
            first_texel: [255, 128, 126, 181],
        };
        assert!(good.lane_zero_is_z());
        // The documented asset's: [127.65, 127.14, 248.75].
        let other = NormalMapPlan {
            component_means: [127.65, 127.14, 248.75],
            ..good.clone()
        };
        assert!(other.lane_zero_is_z());
        // A rotated record: lane 0 near 127, lane 1 near 250. Must FAIL.
        let rotated = NormalMapPlan {
            component_means: [248.75, 127.14, 127.65],
            ..good.clone()
        };
        assert!(!rotated.lane_zero_is_z());
        // Unmeasured is not "false by coincidence": NaN means falsy here, and
        // the caller can see NaN in the numbers.
        let unmeasured = NormalMapPlan {
            component_means: [f64::NAN; 3],
            ..good
        };
        assert!(!unmeasured.lane_zero_is_z());
    }

    #[test]
    fn the_plan_describes_itself_with_the_swizzle_named() {
        let plan = NormalMapPlan {
            key: ResourceKey::new(0x2F4E_681C, 0x4062_2901, 0x067C_79D2),
            layer_index: 0,
            width: 256,
            height: 256,
            mip_count: 9,
            layout: NormalLayout::ZFirst,
            lanes: [1, 2, 0],
            component_means: [128.62, 128.01, 249.33],
            first_texel: [255, 128, 126, 181],
        };
        let text = plan.describe();
        assert!(text.contains("layout=z-first"), "{text}");
        assert!(text.contains("lanes=[x:1,y:2,z:0]"), "{text}");
        assert!(text.contains("249.33"), "{text}");
        assert!(text.contains("[255, 128, 126, 181]"), "{text}");
    }

    #[test]
    fn there_is_exactly_one_normal_layout_and_it_is_z_first() {
        // Adding a second variant is a claim that a record exists with that
        // order. Nothing in the corpus shows one.
        assert_eq!(NormalLayout::ZFirst.as_str(), "z-first");
        assert_eq!(lane_components(NormalLayout::ZFirst), [2, 0, 1]);
    }

    #[test]
    fn a_material_plan_names_its_surfaces_by_count_never_by_purpose() {
        let plan = MeshMaterialPlan {
            mesh_index: 0,
            material_id: 1,
            material_name: None,
            scope: BindingScope::PerMeshBlock { refs_per_mesh: 3 },
            normal_map: None,
            masks: vec![MaskChannelPlan {
                key: ResourceKey::new(1, 2, 3),
                channel_index: 3,
                layer_index: 0,
                lane: Lane::Alpha,
                width: 512,
                height: 512,
            }],
            unidentified: Vec::new(),
            ordinal_conflicts: 0,
        };
        let text = plan.describe();
        for forbidden in ["specular", "gloss", "occlusion", "diffuse", "roughness"] {
            assert!(
                !text.to_lowercase().contains(forbidden),
                "an unestablished purpose must not be named: {text}"
            );
        }
        assert!(text.contains("mask_channels=1"), "{text}");
        assert!(text.contains("unbound=0"), "{text}");
        assert!(
            text.contains("unbound_reasons=none"),
            "no unbound slot means no reason is invented: {text}"
        );
    }

    /// A 2x2 record whose lanes the engine has not been told about, standing in
    /// for a real decode: this module's tests never need the package.
    fn synthetic(
        role: TextureRole,
        lanes: [(Lane, [u8; 4]); 4],
    ) -> spore_material::ResolvedTexture {
        let mut pixels = Vec::with_capacity(lanes.len() * 4);
        for (_, values) in lanes {
            pixels.extend_from_slice(&values);
        }
        let mip = spore_texture::MipImage {
            width: 2,
            height: 2,
            pixels,
        };
        spore_material::ResolvedTexture {
            key: ResourceKey::new(0x2F4E_681C, 0x4062_2901, 0x067C_79D2),
            kind: spore_material::TextureKind::Raster,
            statistics: spore_material::MipStatistics::of(&mip),
            image: mip.clone(),
            // One chain, always: `spore-texture` proved the two-layer reading
            // was two cancelling errors.
            layers: vec![mip],
            role,
            envelope: spore_texture::RasterEnvelope {
                version: 1,
                width: 2,
                height: 2,
                mip_count: 1,
                field_10: 8,
                fourcc: 0,
                field_18: 16,
                field_1c: 0,
            },
            mip_count: 1,
            layer_count: 1,
            pixels_decoded: true,
            claims: Vec::new(),
        }
    }

    fn flat_normal_texels() -> [(Lane, [u8; 4]); 4] {
        // (Z, X, Y, ?) exactly as the `0x15` family stores it.
        [
            (Lane::Red, [255, 128, 128, 0]),
            (Lane::Green, [255, 128, 128, 0]),
            (Lane::Blue, [255, 128, 128, 0]),
            (Lane::Alpha, [255, 128, 128, 0]),
        ]
    }

    #[test]
    fn the_layout_lane_table_is_the_inverse_of_the_component_table() {
        // Both directions of one permutation, stated in one place and checked
        // against each other. `lane_components` says which component each file
        // byte holds; `lanes` says which byte each component is read from. A
        // normal map that is swizzled with one and looked up with the other is a
        // normal map whose Z is in red, which looks almost right.
        for layout in [NormalLayout::ZFirst] {
            let components = lane_components(layout);
            let lanes = layout.lanes();
            for (component, lane) in lanes.iter().enumerate() {
                assert_eq!(
                    components[*lane], component as u8,
                    "{layout:?}: component {component} must be read from byte {lane}"
                );
            }
        }
        assert_eq!(NormalLayout::ZFirst.lanes(), [1, 2, 0]);
    }

    #[test]
    fn a_normal_binding_comes_from_the_published_channels_and_nothing_else() {
        let texture = synthetic(TextureRole::TangentSpaceNormal, flat_normal_texels());
        let plan = NormalMapPlan::from_channels(&texture)
            .expect("the published lanes are an established layout")
            .expect("a normal publishes a normal");
        assert_eq!(plan.lanes, [1, 2, 0], "file byte 0 holds Z");
        assert_eq!(plan.layer_index, 0);
        assert_eq!(plan.first_texel, [255, 128, 128, 0]);
        assert!(plan.lane_zero_is_z(), "read from the published statistics");
    }

    #[test]
    fn a_record_that_publishes_no_normal_is_not_a_finding() {
        // The common case, and it must be `Ok(None)` rather than an error: the
        // great majority of records in this corpus publish no normal.
        let texture = synthetic(
            TextureRole::Unidentified,
            [
                (Lane::Red, [0; 4]),
                (Lane::Green, [0; 4]),
                (Lane::Blue, [0; 4]),
                (Lane::Alpha, [0; 4]),
            ],
        );
        assert_eq!(
            NormalMapPlan::from_channels(&texture),
            Ok(None),
            "nothing published a normal, so nothing is reported"
        );
    }

    #[test]
    fn a_normal_in_an_unestablished_order_is_refused_rather_than_swizzled() {
        // The same texels, published as X, Y, Z in file order — a normal map
        // written the other way round. Binding it with ZFirst's permutation
        // would produce a wrong normal that looks almost right, so it is
        // refused and reported instead.
        let texture = synthetic(TextureRole::TangentSpaceNormal, flat_normal_texels());
        let rotated = vec![
            spore_material::ChannelBinding {
                layer_index: 0,
                lane: Lane::Red,
                role: ChannelRole::NormalX,
            },
            spore_material::ChannelBinding {
                layer_index: 0,
                lane: Lane::Green,
                role: ChannelRole::NormalY,
            },
            spore_material::ChannelBinding {
                layer_index: 0,
                lane: Lane::Blue,
                role: ChannelRole::NormalZ,
            },
        ];
        assert_eq!(
            NormalMapPlan::from_bindings(&rotated, &texture),
            Err(UnboundReason::UnestablishedNormalLayout)
        );
        // Two of three components is the same class of defect: the third is
        // undefined, and a swizzle would invent it.
        assert_eq!(
            NormalMapPlan::from_bindings(&rotated[..2], &texture),
            Err(UnboundReason::IncompleteNormalTriple)
        );
        assert_eq!(
            UnboundReason::UnestablishedNormalLayout.as_str(),
            "unestablished-normal-layout"
        );
    }

    #[test]
    fn a_mesh_is_never_bound_to_a_texture_from_another_meshes_block() {
        // The cut is wrong for this mesh: mesh 1's slice claims a reference the
        // mesh does not carry, so the flat entry it points at belongs to another
        // block. Before this check the engine bound it, which is exactly the
        // failure the documented creature used to exhibit.
        let mine = spore_gmdl::GmdlTextureRef {
            instance_id: 1,
            group_id: 0x4063_2901,
        };
        let someone_elses = spore_gmdl::GmdlTextureRef {
            instance_id: 1,
            group_id: 0x4063_2900,
        };
        let materials = ModelMaterials {
            scope: BindingScope::PerMeshBlock { refs_per_mesh: 1 },
            meshes: vec![
                MeshSlots {
                    mesh_index: 0,
                    material_id: 1,
                    slots: vec![mine],
                },
                // The truncated-list shape: mesh 1's slot names the texture that
                // belongs to mesh 0's block, and the flat index it was cut from
                // holds mesh 1's own reference instead.
                MeshSlots {
                    mesh_index: 1,
                    material_id: 1,
                    slots: vec![mine],
                },
            ],
            textures: vec![
                MaterialTexture {
                    reference: Some(mine),
                    resolved: None,
                    refusal: Some("synthetic".into()),
                },
                MaterialTexture {
                    reference: Some(someone_elses),
                    resolved: None,
                    refusal: Some("synthetic".into()),
                },
            ],
            ..ModelMaterials::empty(None)
        };
        // Mesh 0 is fine: its slot names the reference the cut handed it, so it
        // is only unbound because the synthetic record refused to decode.
        assert_eq!(
            materials.plans()[0].unidentified[0].reason,
            UnboundReason::Refused
        );
        let plan = &materials.plans()[1];
        assert_eq!(materials.flat_index(1, 0), 1);
        assert_eq!(plan.unidentified.len(), 1);
        assert_eq!(
            plan.unidentified[0].reason,
            UnboundReason::EntryReferenceMismatch,
            "a slot whose entry names another reference binds nothing"
        );
        assert!(
            !plan.has_any_surface(),
            "and there is no surface to attach to the wrong mesh"
        );
    }
}
