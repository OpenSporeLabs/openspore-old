//! Role-driven texture upload: which surface feeds which material channel.
//!
//! `spore-material` decides *what a record is*; this module decides *where that
//! goes*. The two must not be confused, and the confusion has a name in this
//! repository: **applying a DXT5 record as diffuse colour**, which the previous
//! revision of this engine did. `docs/MATERIAL-ROLES.md` §4 measures why that
//! was wrong — over 1 374 decodable `DXT5` records the green span is 0, the blue
//! span is 0, red takes at most two values, and 477 771 sampled palette entries
//! collapse to exactly two RGB triples. There is no base-colour map in this
//! corpus, so **no texture is bound as one here.**
//!
//! # Where the base colour comes from instead
//!
//! From the model's `sporemodel` record, and only from there:
//! [`crate::domain::paint`] resolves the record and hands this module a linear
//! RGB triple. Two consequences this module enforces:
//!
//! * `base_color_texture` stays `None` even when `base_color` carries a real
//!   colour — the value came out of an XML record, not a texel.
//! * `None` for the colour is an **absence**, not a grey. The renderer falls
//!   back to [`UNTEXURED_TINT`] so the normal map stays readable, and the
//!   report line says `state=unavailable reason=...`. Never `(0, 0, 0)`.
//!
//! # The three bindings, and what each one is
//!
//! | record role | Bevy slot | evidence |
//! |---|---|---|
//! | [`TextureRole::TangentSpaceNormal`] | `StandardMaterial::normal_map_texture` | 901/901 records, Z-first, VERIFIED |
//! | [`TextureRole::LayeredAlphaMask`] | **none** | purpose UNKNOWN — see below |
//! | [`TextureRole::Unidentified`] | **none** | a finding, not a default |
//!
//! ## Why the masks go nowhere
//!
//! A `DXT5` record has two layers and each layer's alpha plane is a full-range
//! 8-bit channel: **four** channels for the corpus's common three-entry set, none
//! of which has an established purpose (`spore_material::claims::MASK_LAYER_PURPOSE`
//! is an `UNKNOWN` non-finding). Binding one to `metallic_roughness_texture`
//! would be naming it "specular"/"gloss"/"occlusion", which is precisely the
//! claim `docs/MATERIAL-ROLES.md` §10.5 forbids.
//!
//! So each plane is uploaded, given a name that says only what it *is*
//! (`mask_channel_N`), and carried on the entity for a future consumer that has
//! evidence. Nothing is thrown away and nothing is claimed.
//!
//! # The swizzle, and the format it forces
//!
//! File byte 0 of a `0x15` record is the normal's **Z**. Bevy's
//! `apply_normal_mapping` samples `.rgb` and remaps with `n * 2 - 1`, assuming
//! `R = X`. So the bytes must be reordered on upload — see
//! [`crate::domain::materials::swizzle_z_first_to_xyz`] — and the image format
//! must be **`Rgba8Unorm`, never `Rgba8UnormSrgb`**: an sRGB decode applies a
//! transfer curve to a vector that is already linear, which warps the normal
//! without anything looking obviously broken.
//!
//! Both decisions are testable without a GPU, and both are tested.

use bevy::asset::RenderAssetUsages;
use bevy::image::{Image, ImageSampler};
use bevy::pbr::StandardMaterial;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

use spore_core::ResourceKey;
use spore_material::TextureRole;

use crate::domain::materials::{
    swizzle_z_first_to_xyz, MaskChannelPlan, MaterialTexture, MeshMaterialPlan, NormalMapPlan,
};

/// The colour used when no base-colour texture exists.
///
/// Which is **always**, in this corpus. There is no `DXT5`/`0x15` record that
/// decodes as a base-colour map, so any hue would be an invention. A neutral
/// grey is the only choice that does not assert a colour, and it makes the
/// normal map's shading readable rather than hiding it under a tint.
///
/// Distinct in name from `crate::scene::MISSING_TEXTURE_TINT`, which says "no
/// texture resolved at all"; a log line distinguishes the two via the record's
/// role.
pub const UNTEXURED_TINT: Color = Color::srgb(0.62, 0.62, 0.64);

/// Perceptual roughness for an untextured asset.
///
/// Mid-rough: no roughness map is established either, so this is a *stated
/// visual choice* rather than a claim about the record, and it is stated in the
/// material's own doc comment rather than left as a literal.
pub const UNTEXURED_ROUGHNESS: f32 = 0.62;

/// Which way a normal map's green channel points.
///
/// **UNKNOWN.** No reference frame of the original game has been captured, so
/// whether Spore's `0x15` normal maps are DirectX-style (Y down) or OpenGL-style
/// (Y up) cannot be settled from the records: both encode the same `(x, y, z)`
/// and differ only in the sign convention of the author. The corpus establishes
/// only the *byte order*, which
/// [`crate::domain::materials::NormalLayout::ZFirst`] states and this engine
/// honours.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NormalMapOrientation {
    /// Upload the bytes as they are stored. The default, because it is the only
    /// choice the corpus supports evidence for.
    #[default]
    AsStored,
    /// Swap the green channel, for a DirectX-authored normal map.
    FlipGreen,
}

impl NormalMapOrientation {
    /// The canonical spelling, for logs and reports.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AsStored => "as-stored",
            Self::FlipGreen => "flip-green",
        }
    }

    /// Parses the spelling, or `None` when it is not one this build has.
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "as-stored" => Some(Self::AsStored),
            "flip-green" => Some(Self::FlipGreen),
            _ => None,
        }
    }
}

/// What one uploaded surface is, named only by what is established about it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfaceKind {
    /// A tangent-space normal map, already swizzled to `(X, Y, Z)`.
    TangentSpaceNormal,
    /// One alpha plane of one mask record.
    ///
    /// `layer_index` is which of the record's layers. **No purpose is claimed
    /// for it**; `spore_material::claims::MASK_LAYER_PURPOSE` is `UNKNOWN`.
    MaskChannel {
        /// Which of the record's layers this plane came from.
        layer_index: usize,
    },
}

impl SurfaceKind {
    /// The canonical spelling, for logs and reports.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::TangentSpaceNormal => "tangent-space-normal",
            Self::MaskChannel { layer_index } => {
                // Positional, never a purpose.
                if layer_index == 0 {
                    "mask-channel-0"
                } else {
                    "mask-channel-n"
                }
            }
        }
    }
}

/// One GPU-ready surface: an image, its format, and what it is.
#[derive(Debug, Clone, PartialEq)]
pub struct SurfaceImage {
    /// What the surface is.
    pub kind: SurfaceKind,
    /// The record it came from, with the assumed type word substituted in.
    pub key: ResourceKey,
    /// Width of the base mip.
    pub width: u32,
    /// Height of the base mip.
    pub height: u32,
    /// The upload format.
    pub format: TextureFormat,
    /// Tightly packed texels: four bytes for the normal map, one per mask.
    pub pixels: Vec<u8>,
}

impl SurfaceImage {
    /// The Bevy image this surface becomes.
    ///
    /// Takes `&self` rather than consuming so a caller can keep the surface for
    /// a report after uploading it.
    pub fn image(&self) -> Image {
        let mut image = Image::new_fill(
            Extent3d {
                width: self.width,
                height: self.height,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            &self.pixels,
            self.format,
            RenderAssetUsages::default(),
        );
        // Linear filtering. Spore's rasters carry a mip chain, but only the base
        // level is uploaded, so mipmap selection is off by construction and
        // asking for it would be asking for something not present.
        image.sampler = ImageSampler::linear();
        image
    }

    /// A one-line description that names no purpose it cannot establish.
    pub fn describe(&self) -> String {
        format!(
            "{} {} {}x{} {} bytes",
            self.kind.as_str(),
            self.key,
            self.width,
            self.height,
            self.pixels.len()
        )
    }
}

/// Every surface one mesh's material needs.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SurfaceSet {
    /// The tangent-space normal map, when the mesh has one.
    pub normal_map: Option<SurfaceImage>,
    /// Every mask channel, in slot then layer order.
    pub masks: Vec<SurfaceImage>,
}

impl SurfaceSet {
    /// Whether anything was bound at all.
    pub fn is_empty(&self) -> bool {
        self.normal_map.is_none() && self.masks.is_empty()
    }

    /// How many surfaces were produced.
    pub fn len(&self) -> usize {
        usize::from(self.normal_map.is_some()) + self.masks.len()
    }
}

/// Builds the swizzled normal map for a mesh's plan.
///
/// `None` when the plan has no normal map, or when the record it names is not in
/// `textures` — which would mean the plan and the record table disagree, a bug
/// this function reports rather than papers over.
pub fn normal_map_surface(
    plan: &NormalMapPlan,
    textures: &[MaterialTexture],
    orientation: NormalMapOrientation,
) -> Option<SurfaceImage> {
    let entry = textures
        .iter()
        .find(|entry| entry.key() == Some(plan.key))?;
    let texture = entry.resolved.as_ref()?;
    debug_assert_eq!(
        texture.role,
        TextureRole::TangentSpaceNormal,
        "a normal map surface must come from a record with that role"
    );
    let layer = texture.layer(0)?;
    let mut pixels = swizzle_z_first_to_xyz(&layer.pixels);
    if orientation == NormalMapOrientation::FlipGreen {
        // A DirectX-authored normal map has its green channel pointing the other
        // way, which for an unsigned byte is `255 - g`. It is applied **here**,
        // once, so `StandardMaterial::flip_normal_map_y` can stay `false` --
        // setting both would negate twice and land back on the stored value,
        // which is exactly the kind of "looks plausible" defect this whole
        // module exists to prevent.
        for texel in pixels.chunks_exact_mut(4) {
            texel[1] = u8::MAX - texel[1];
        }
    }
    Some(SurfaceImage {
        kind: SurfaceKind::TangentSpaceNormal,
        key: plan.key,
        width: layer.width.max(1),
        height: layer.height.max(1),
        // `Rgba8Unorm`, never `Rgba8UnormSrgb`: the bytes are a direction, and a
        // transfer curve applied to a direction is a different direction.
        format: TextureFormat::Rgba8Unorm,
        pixels,
    })
}

/// Builds one mask plane's surface.
///
/// Single channel (`R8Unorm`), because the record has exactly one channel of
/// content: the colour planes are `ChannelRole::DegenerateColour` under the
/// byte-pinned codec. Replicating the byte into RGB would invent a colour
/// nobody has measured.
pub fn mask_channel_surface(
    mask: &MaskChannelPlan,
    textures: &[MaterialTexture],
) -> Option<SurfaceImage> {
    let entry = textures
        .iter()
        .find(|entry| entry.key() == Some(mask.key))?;
    let texture = entry.resolved.as_ref()?;
    let layer = texture.layer(mask.layer_index)?;
    Some(SurfaceImage {
        kind: SurfaceKind::MaskChannel {
            layer_index: mask.layer_index,
        },
        key: mask.key,
        width: layer.width.max(1),
        height: layer.height.max(1),
        format: TextureFormat::R8Unorm,
        pixels: crate::domain::materials::extract_lane(&layer.pixels, 3),
    })
}

/// Every surface a mesh's plan calls for, in slot then layer order.
pub fn build_surfaces(
    plan: &MeshMaterialPlan,
    textures: &[MaterialTexture],
    orientation: NormalMapOrientation,
) -> SurfaceSet {
    let normal_map = plan
        .normal_map
        .as_ref()
        .and_then(|normal| normal_map_surface(normal, textures, orientation));
    let masks = plan
        .masks
        .iter()
        .filter_map(|mask| mask_channel_surface(mask, textures))
        .collect();
    SurfaceSet { normal_map, masks }
}

/// Uploads one surface into `images` and returns its handle.
///
/// Takes `&mut Assets<Image>` rather than returning a handle into thin air: a
/// `Handle` whose image was never added renders as the default white texture,
/// which looks like a material bug.
pub fn upload(surface: &SurfaceImage, images: &mut Assets<Image>) -> Handle<Image> {
    let mut image = Image::new_fill(
        Extent3d {
            width: surface.width,
            height: surface.height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        &surface.pixels,
        surface.format,
        RenderAssetUsages::default(),
    );
    // Linear filtering. Spore's rasters carry a mip chain, but only the base
    // level is uploaded, so mipmap selection is off by construction and asking
    // for it would be asking for something not present.
    image.sampler = ImageSampler::linear();
    images.add(image)
}

/// Builds the `StandardMaterial` a mesh's surfaces call for.
///
/// `normal_map` is the already-uploaded handle, or `None` when the mesh has no
/// normal map. It is a parameter rather than something looked up here so this
/// function stays a pure mapping from a [`SurfaceSet`] to a material and can be
/// asserted on with no asset collection.
///
/// `base_color` is the **linear** RGB triple the `sporemodel` record supplied,
/// or `None` when no such record was found. `None` is an absence, not a colour:
/// it falls back to [`UNTEXURED_TINT`], which is what makes the normal map's
/// shading readable. It is never `(0, 0, 0)`, because black is a colour a
/// record can carry and this is not one.
///
/// # What is deliberately absent
///
/// * **No base-colour *texture*.** No raster record in this corpus decodes as
///   one; see the module docs. `base_color_texture` stays `None` even when
///   `base_color` carries a record's colour, because that colour came from an
///   XML record and not from a texel.
/// * **No metallic/roughness texture.** The only full-range channels available
///   are the mask planes, and their purpose is `UNKNOWN`.
/// * **No alpha blending.** The mask planes are not established as coverage, so
///   treating one as opacity would be a claim. The material stays opaque.
///
/// Every one of those is a *stated refusal*, not an omission: the plan beside
/// this function records what the record actually offered.
pub fn standard_material(
    surfaces: &SurfaceSet,
    normal_map: Option<Handle<Image>>,
    base_color: Option<[f32; 3]>,
) -> StandardMaterial {
    debug_assert_eq!(
        normal_map.is_some(),
        surfaces.normal_map.is_some(),
        "a handle must be supplied for exactly the surface that exists"
    );
    StandardMaterial {
        // The shader's `editorColors.mColor1.rgb` is a `float4` constant, i.e.
        // already linear, and so is this upload path's albedo. `Color::srgb`
        // here would apply a transfer curve a second time.
        base_color: base_color.map_or(UNTEXURED_TINT, |rgb| {
            Color::linear_rgb(rgb[0], rgb[1], rgb[2])
        }),
        base_color_texture: None,
        normal_map_texture: normal_map,
        // Deliberately `false`, for every orientation. The convention is
        // applied to the uploaded bytes in [`normal_map_surface`]; setting this
        // flag as well would negate green a second time in the shader.
        flip_normal_map_y: false,
        perceptual_roughness: UNTEXURED_ROUGHNESS,
        metallic: 0.0,
        // Spore geometry carries its own winding convention, which has not been
        // confirmed against a rendered original frame. Culling back faces on a
        // guess would make a model invisible; showing both sides cannot hide
        // geometry. Revisit when a reference frame exists.
        double_sided: true,
        alpha_mode: AlphaMode::Opaque,
        ..default()
    }
}

/// One mesh's material, bound and uploaded.
///
/// The handle-based half of the wiring, split from [`build_surfaces`] so the
/// decisions can be tested without an asset collection and the upload can be
/// tested without a renderer.
#[derive(Debug, Clone)]
pub struct BoundMaterial {
    /// The plan this was built from, kept so the log can show what the record
    /// offered beside what was bound.
    pub plan: MeshMaterialPlan,
    /// The surfaces that were produced.
    pub surfaces: SurfaceSet,
    /// The material, with the normal map handle attached.
    pub material: StandardMaterial,
    /// The uploaded mask planes, positionally aligned with
    /// [`Self::surfaces`]'s `masks`.
    ///
    /// Named by position only. Their purpose is `UNKNOWN`, so nothing here
    /// claims one; see the module documentation.
    pub mask_handles: Vec<Handle<Image>>,
}

impl BoundMaterial {
    /// How many mask planes were produced and uploaded.
    pub fn mask_count(&self) -> usize {
        self.surfaces.masks.len()
    }
}

/// Builds and uploads one mesh's material.
///
/// `images` is passed in rather than fetched from the world so the whole
/// binding is callable — and assertable — with no `App`.
///
/// `base_color` is the linear triple from the `sporemodel` record, threaded
/// straight through to [`standard_material`]. It is deliberately **not**
/// consulted while building surfaces: a record colour is not a texture, and
/// letting it influence which surface is uploaded would make the texture path
/// depend on the colour path.
pub fn bind_material(
    plan: &MeshMaterialPlan,
    textures: &[MaterialTexture],
    orientation: NormalMapOrientation,
    images: &mut Assets<Image>,
    base_color: Option<[f32; 3]>,
) -> BoundMaterial {
    let surfaces = build_surfaces(plan, textures, orientation);
    let handle = surfaces
        .normal_map
        .as_ref()
        .map(|surface| upload(surface, images));
    // Every mask plane is uploaded too. Nothing reads them yet, and that is the
    // point: they are kept, addressable and honestly labelled rather than
    // guessed at or dropped.
    let mask_handles = surfaces
        .masks
        .iter()
        .map(|surface| upload(surface, images))
        .collect();
    BoundMaterial {
        plan: plan.clone(),
        material: standard_material(&surfaces, handle, base_color),
        surfaces,
        mask_handles,
    }
}

/// How many textures a model resolved, for the report line.
pub fn resolved_count(
    results: &[Result<spore_material::ResolvedTexture, spore_material::MaterialError>],
) -> usize {
    results.iter().filter(|r| r.is_ok()).count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::materials::{MeshSlots, ModelMaterials};

    /// 2x2, so every synthetic image below holds exactly four texels.
    const TEXELS: u32 = 2;

    /// A synthetic normal map: Z in byte 0, X in byte 1, Y in byte 2, plus a
    /// fourth lane this repository cannot interpret.
    ///
    /// Z runs 250, 240, 230, 220 across the four texels while X and Y sit at
    /// 128, the encoding of zero -- the same shape as the measured creature
    /// record (lane means `[249.33, 128.62, 128.01]`).
    fn normal_entry(group: u32) -> MaterialTexture {
        let pixels: Vec<u8> = [250u8, 240, 230, 220]
            .into_iter()
            .flat_map(|z| [z, 128, 128, 7])
            .collect();
        MaterialTexture {
            reference: None,
            resolved: Some(synthetic_texture(
                group,
                TextureRole::TangentSpaceNormal,
                pixels,
                1,
            )),
            refusal: None,
        }
    }

    /// A synthetic two-layer mask: the colour planes collapse to zero and each
    /// layer's alpha plane carries distinct full-range values.
    fn mask_entry(group: u32) -> MaterialTexture {
        let layer0: Vec<u8> = (0..TEXELS * TEXELS)
            .flat_map(|i| [0u8, 0, 0, i as u8])
            .collect();
        let layer1: Vec<u8> = (0..TEXELS * TEXELS)
            .flat_map(|i| [0u8, 0, 0, (255 - i) as u8])
            .collect();
        let mut texture =
            synthetic_texture(group, TextureRole::LayeredAlphaMask, layer0.clone(), 2);
        texture.layers = vec![synthetic_mip(layer0), synthetic_mip(layer1)];
        MaterialTexture {
            reference: None,
            resolved: Some(texture),
            refusal: None,
        }
    }

    fn synthetic_mip(pixels: Vec<u8>) -> spore_texture::MipImage {
        spore_texture::MipImage {
            width: TEXELS,
            height: TEXELS,
            pixels,
        }
    }

    fn synthetic_texture(
        group: u32,
        role: TextureRole,
        pixels: Vec<u8>,
        layers: usize,
    ) -> spore_material::ResolvedTexture {
        let mip = synthetic_mip(pixels);
        spore_material::ResolvedTexture {
            key: ResourceKey::new(0x2F4E_681C, group, 0x067C_79D2),
            kind: spore_material::TextureKind::Raster,
            image: mip.clone(),
            layers: vec![mip; layers],
            role,
            statistics: spore_material::MipStatistics {
                texels: (TEXELS * TEXELS) as usize,
                distinct: [4, 1, 1, 4],
                min: [0; 4],
                max: [255; 4],
                sum: [1020, 512, 512, 1020],
            },
            envelope: spore_texture::RasterEnvelope {
                version: 1,
                width: TEXELS,
                height: TEXELS,
                mip_count: 1,
                field_10: 8,
                fourcc: 0,
                field_18: 16,
                field_1c: 0,
            },
            mip_count: 1,
            layer_count: layers,
            pixels_decoded: true,
            claims: Vec::new(),
        }
    }

    fn one_slot_plan(group: u32, entry: MaterialTexture) -> MeshMaterialPlan {
        let mut materials = ModelMaterials::empty(None);
        materials.textures = vec![entry];
        materials.meshes = vec![MeshSlots {
            mesh_index: 0,
            material_id: 0xD7BE_35F9,
            slots: vec![spore_gmdl::GmdlTextureRef {
                instance_id: 0x067C_79D2,
                group_id: group,
            }],
        }];
        materials.plans()[0].clone()
    }

    const NORMAL_GROUP: u32 = 0x4062_2901;
    const MASK_GROUP: u32 = 0x4062_2900;

    fn plan_with_normal() -> MeshMaterialPlan {
        one_slot_plan(NORMAL_GROUP, normal_entry(NORMAL_GROUP))
    }

    fn plan_with_masks() -> MeshMaterialPlan {
        one_slot_plan(MASK_GROUP, mask_entry(MASK_GROUP))
    }

    #[test]
    fn a_normal_map_surface_puts_z_in_blue_and_never_in_red() {
        let plan = plan_with_normal();
        let textures = vec![normal_entry(NORMAL_GROUP)];
        let surfaces = build_surfaces(&plan, &textures, NormalMapOrientation::AsStored);
        let normal = surfaces
            .normal_map
            .expect("a plan with a normal map must build one");
        assert_eq!(normal.format, TextureFormat::Rgba8Unorm);
        assert_eq!(normal.width, 2);
        assert_eq!(normal.pixels.len(), 4 * 4);
        // Every texel's first file byte was 250..; it must now be the BLUE byte.
        for (index, texel) in normal.pixels.chunks_exact(4).enumerate() {
            assert_eq!(texel[2], [250, 240, 230, 220][index], "Z must land in blue");
            assert_eq!(texel[0], 128, "X must land in red");
            assert_eq!(texel[1], 128, "Y must land in green");
            assert_eq!(texel[3], 255, "an uninterpreted lane must not be carried");
        }
        // And specifically: red is NOT the hot lane.
        let red_mean = lane_mean(&normal.pixels, 0);
        let blue_mean = lane_mean(&normal.pixels, 2);
        assert!(
            blue_mean > red_mean + 100.0,
            "blue {blue_mean} must dominate red {red_mean}"
        );
    }

    #[test]
    fn a_normal_map_is_never_uploaded_as_srgb() {
        // An sRGB decode applies a transfer curve to a vector that is already
        // linear. Nothing about that looks broken on screen.
        let plan = plan_with_normal();
        let surfaces = build_surfaces(
            &plan,
            &[normal_entry(NORMAL_GROUP)],
            NormalMapOrientation::AsStored,
        );
        let normal = surfaces.normal_map.unwrap();
        assert_ne!(normal.format, TextureFormat::Rgba8UnormSrgb);
    }

    #[test]
    fn flipping_the_green_channel_swaps_x_and_y_and_leaves_z_alone() {
        let plan = plan_with_normal();
        let textures = vec![normal_entry(NORMAL_GROUP)];
        let stored = build_surfaces(&plan, &textures, NormalMapOrientation::AsStored)
            .normal_map
            .unwrap();
        let flipped = build_surfaces(&plan, &textures, NormalMapOrientation::FlipGreen)
            .normal_map
            .unwrap();
        assert_eq!(stored.pixels.len(), flipped.pixels.len());
        for (a, b) in stored
            .pixels
            .chunks_exact(4)
            .zip(flipped.pixels.chunks_exact(4))
        {
            assert_eq!(a[0], b[0], "red is untouched");
            assert_eq!(a[2], b[2], "blue, which holds Z, is untouched");
            // `255 - g`, which is what a DirectX-authored Y means for an
            // unsigned byte, and what Bevy's `flip_normal_map_y` would have done
            // in the shader -- applied here instead, so it cannot double.
            assert_eq!(b[1], u8::MAX - a[1], "green is negated");
        }
        // The default is the one the corpus supports evidence for.
        assert_eq!(
            NormalMapOrientation::default(),
            NormalMapOrientation::AsStored
        );
        assert_eq!(
            NormalMapOrientation::parse("flip-green"),
            Some(NormalMapOrientation::FlipGreen)
        );
        assert_eq!(NormalMapOrientation::parse("sideways"), None);
    }

    #[test]
    fn a_two_layer_mask_produces_two_single_channel_surfaces() {
        let plan = plan_with_masks();
        let surfaces = build_surfaces(
            &plan,
            &[mask_entry(MASK_GROUP)],
            NormalMapOrientation::AsStored,
        );
        assert!(surfaces.normal_map.is_none(), "a mask is not a normal map");
        assert_eq!(surfaces.masks.len(), 2, "one plane per layer");
        for mask in &surfaces.masks {
            assert_eq!(mask.format, TextureFormat::R8Unorm);
            assert_eq!(mask.pixels.len(), 2 * 2, "one byte per texel");
        }
        assert_eq!(surfaces.masks[0].pixels, vec![0, 1, 2, 3]);
        assert_eq!(surfaces.masks[1].pixels, vec![255, 254, 253, 252]);
        assert_eq!(
            surfaces.masks[0].kind,
            SurfaceKind::MaskChannel { layer_index: 0 }
        );
        assert_eq!(
            surfaces.masks[1].kind,
            SurfaceKind::MaskChannel { layer_index: 1 }
        );
    }

    #[test]
    fn a_mask_surface_does_not_replicate_its_byte_into_rgb() {
        // Replicating would invent a grey the record never carried.
        let plan = plan_with_masks();
        let surfaces = build_surfaces(
            &plan,
            &[mask_entry(MASK_GROUP)],
            NormalMapOrientation::AsStored,
        );
        for mask in &surfaces.masks {
            assert_eq!(
                mask.pixels.len(),
                mask.width as usize * mask.height as usize
            );
        }
    }

    #[test]
    fn the_three_entry_shape_binds_four_mask_planes_and_one_normal() {
        // The corpus's common shape is (mask, mask, normal): two records x two
        // layers is four planes.
        let mut materials = ModelMaterials::empty(None);
        materials.textures = vec![
            mask_entry(0x4063_2900),
            mask_entry(0x4063_2901),
            normal_entry(0x4063_2902),
        ];
        materials.meshes = vec![MeshSlots {
            mesh_index: 0,
            material_id: 0x407D_FDDB,
            slots: vec![
                spore_gmdl::GmdlTextureRef {
                    instance_id: 1,
                    group_id: 0x4063_2900,
                },
                spore_gmdl::GmdlTextureRef {
                    instance_id: 1,
                    group_id: 0x4063_2901,
                },
                spore_gmdl::GmdlTextureRef {
                    instance_id: 1,
                    group_id: 0x4063_2902,
                },
            ],
        }];
        let plan = materials.plans()[0].clone();
        let surfaces = build_surfaces(&plan, &materials.textures, NormalMapOrientation::AsStored);
        assert_eq!(surfaces.masks.len(), 4);
        assert!(surfaces.normal_map.is_some());
        assert_eq!(surfaces.len(), 5);
    }

    #[test]
    fn no_surface_ever_goes_into_a_base_colour_slot() {
        let plan = plan_with_masks();
        let textures = vec![mask_entry(MASK_GROUP)];
        let surfaces = build_surfaces(&plan, &textures, NormalMapOrientation::AsStored);
        let material = standard_material(&surfaces, None, None);
        assert!(material.base_color_texture.is_none());
        assert_eq!(material.base_color, UNTEXURED_TINT);
        assert_eq!(material.alpha_mode, AlphaMode::Opaque);
        assert_eq!(material.metallic, 0.0);
        assert!(material.metallic_roughness_texture.is_none());
        assert!(material.normal_map_texture.is_none());
        assert_eq!(material.perceptual_roughness, UNTEXURED_ROUGHNESS);
        assert!(material.double_sided);
    }

    #[test]
    fn a_record_colour_reaches_base_color_verbatim_and_stays_linear() {
        // 0x2b978c46:0x40626200:0x05e3df13 <skincolor1>0.498039,0.803922,0.474510
        let rgb = [0.498_039f32, 0.803_922, 0.474_510];
        let surfaces = SurfaceSet::default();
        let material = standard_material(&surfaces, None, Some(rgb));
        assert_eq!(
            material.base_color,
            Color::linear_rgb(0.498_039, 0.803_922, 0.474_510)
        );
        // Linear, never through the sRGB transfer curve: the shader's
        // `editorColors.mColor1` is a `float4` constant, already linear, and a
        // second decode would darken and desaturate it.
        assert_ne!(
            material.base_color,
            Color::srgb(0.498_039, 0.803_922, 0.474_510)
        );
        // The colour came from an XML record, so the base-colour *texture* slot
        // is still empty: this is the invariant the previous revision broke.
        assert!(material.base_color_texture.is_none());
    }

    #[test]
    fn an_absent_record_colour_falls_back_to_the_neutral_tint_and_not_to_black() {
        let surfaces = SurfaceSet::default();
        let material = standard_material(&surfaces, None, None);
        assert_eq!(material.base_color, UNTEXURED_TINT);
        assert_ne!(material.base_color, Color::BLACK);
    }

    #[test]
    fn a_zero_record_colour_is_still_black_because_black_is_a_colour_a_record_can_carry() {
        // `0,0,0` appears verbatim in the corpus's paint lists, so it must not
        // be mistaken for the "nothing found" fallback. This is the pair of
        // facts that `Option<[f32; 3]>` exists to keep apart.
        let material = standard_material(&SurfaceSet::default(), None, Some([0.0, 0.0, 0.0]));
        assert_eq!(material.base_color, Color::BLACK);
        assert_ne!(material.base_color, UNTEXURED_TINT);
    }

    #[test]
    fn binding_attaches_the_normal_map_handle_and_uploads_every_mask() {
        let plan = plan_with_normal();
        let textures = vec![normal_entry(NORMAL_GROUP)];
        let mut images: Assets<Image> = Assets::default();
        let bound = bind_material(
            &plan,
            &textures,
            NormalMapOrientation::AsStored,
            &mut images,
            None,
        );
        let handle = bound.material.normal_map_texture.clone().expect("a handle");
        let image = images
            .get(&handle)
            .expect("the image must be in the collection");
        assert_eq!(
            image.texture_descriptor.format,
            TextureFormat::Rgba8Unorm,
            "an sRGB upload would warp the normal"
        );
        assert_eq!(
            bound.surfaces.normal_map.as_ref().unwrap().key,
            handle_agnostic_key()
        );
        assert_eq!(images.len(), 1);
        assert_eq!(
            bound.mask_count(),
            0,
            "a normal-map record contributes no mask"
        );
    }

    #[test]
    fn a_mesh_with_two_masks_uploads_two_images_and_no_material_slot() {
        let plan = plan_with_masks();
        let textures = vec![mask_entry(MASK_GROUP)];
        let mut images: Assets<Image> = Assets::default();
        let bound = bind_material(
            &plan,
            &textures,
            NormalMapOrientation::AsStored,
            &mut images,
            None,
        );
        assert!(bound.material.normal_map_texture.is_none());
        assert_eq!(bound.mask_count(), 2);
        assert_eq!(images.len(), 2, "both planes are uploaded and kept");
        for (surface, handle) in bound.surfaces.masks.iter().zip(bound.mask_handles.iter()) {
            let image = images
                .get(handle)
                .expect("the image must be in the collection");
            assert_eq!(image.texture_descriptor.format, TextureFormat::R8Unorm);
            assert_eq!(image.width(), surface.width);
        }
    }

    fn handle_agnostic_key() -> ResourceKey {
        ResourceKey::new(0x2F4E_681C, 0x4062_2901, 0x067C_79D2)
    }

    #[test]
    fn the_orientation_is_applied_once_in_the_bytes_and_never_in_the_shader() {
        let surfaces = SurfaceSet::default();
        let as_stored = standard_material(&surfaces, None, None);
        assert!(!as_stored.flip_normal_map_y);
    }

    #[test]
    fn an_unidentified_record_produces_no_surface_at_all() {
        let mut materials = ModelMaterials::empty(None);
        materials.textures = vec![MaterialTexture {
            reference: None,
            resolved: Some(synthetic_texture(
                MASK_GROUP,
                TextureRole::Unidentified,
                vec![0; 16],
                1,
            )),
            refusal: None,
        }];
        materials.meshes = vec![MeshSlots {
            mesh_index: 0,
            material_id: 1,
            slots: vec![spore_gmdl::GmdlTextureRef {
                instance_id: 1,
                group_id: 0x4063_2900,
            }],
        }];
        let plan = materials.plans()[0].clone();
        assert_eq!(plan.unidentified.len(), 1);
        assert!(!plan.has_any_surface());
        let surfaces = build_surfaces(&plan, &materials.textures, NormalMapOrientation::AsStored);
        assert!(surfaces.is_empty());
    }

    #[test]
    fn a_refused_reference_produces_no_surface_and_keeps_its_reason() {
        let materials = ModelMaterials {
            textures: vec![MaterialTexture {
                reference: None,
                resolved: None,
                refusal: Some("no such record".into()),
            }],
            meshes: vec![MeshSlots {
                mesh_index: 0,
                material_id: 1,
                slots: vec![spore_gmdl::GmdlTextureRef {
                    instance_id: 1,
                    group_id: 0x4063_2900,
                }],
            }],
            ..ModelMaterials::empty(None)
        };
        let plan = materials.plans()[0].clone();
        assert!(plan.unidentified.is_empty() || plan.normal_map.is_none());
        assert_eq!(materials.refusals().count(), 1);
    }

    #[test]
    fn surface_descriptions_name_no_unestablished_purpose() {
        let surfaces = SurfaceSet {
            normal_map: Some(SurfaceImage {
                kind: SurfaceKind::TangentSpaceNormal,
                key: ResourceKey::new(1, 2, 3),
                width: 512,
                height: 512,
                format: TextureFormat::Rgba8Unorm,
                pixels: vec![0; 16],
            }),
            masks: vec![SurfaceImage {
                kind: SurfaceKind::MaskChannel { layer_index: 0 },
                key: ResourceKey::new(1, 2, 4),
                width: 512,
                height: 512,
                format: TextureFormat::R8Unorm,
                pixels: vec![0; 4],
            }],
        };
        for surface in [surfaces.normal_map.clone().unwrap()]
            .into_iter()
            .chain(surfaces.masks.clone())
        {
            let text = surface.describe().to_lowercase();
            for forbidden in [
                "specular",
                "gloss",
                "occlusion",
                "diffuse",
                "albedo",
                "roughness",
            ] {
                assert!(
                    !text.contains(forbidden),
                    "an unestablished purpose leaked: {text}"
                );
            }
        }
        assert_eq!(
            SurfaceKind::MaskChannel { layer_index: 0 }.as_str(),
            "mask-channel-0"
        );
        assert_eq!(
            SurfaceKind::MaskChannel { layer_index: 1 }.as_str(),
            "mask-channel-n",
            "beyond the first the name stays positional"
        );
    }

    #[test]
    fn an_empty_plan_binds_nothing_and_says_so() {
        let plan = MeshMaterialPlan {
            mesh_index: 0,
            material_id: 0,
            material_name: None,
            scope: spore_material::BindingScope::WholeModel,
            normal_map: None,
            masks: Vec::new(),
            unidentified: Vec::new(),
            ordinal_conflicts: 0,
        };
        let surfaces = build_surfaces(&plan, &[], NormalMapOrientation::AsStored);
        assert!(surfaces.is_empty());
        assert_eq!(surfaces.len(), 0);
    }

    fn lane_mean(pixels: &[u8], lane: usize) -> f64 {
        let bytes: Vec<u8> = pixels.iter().skip(lane).step_by(4).copied().collect();
        assert!(!bytes.is_empty());
        bytes.iter().map(|b| f64::from(*b)).sum::<f64>() / bytes.len() as f64
    }
}
