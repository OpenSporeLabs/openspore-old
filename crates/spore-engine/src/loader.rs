//! The runtime boundary: a decoded [`Scene`] becomes entities.
//!
//! # Ownership, and why the split with [`crate::scene`] is deliberate
//!
//! * [`crate::scene::ScenePlugin`] owns the **world**: camera, lights, ground.
//!   Those are OpenSpore's choices and change when the renderer changes.
//! * [`crate::loader::SporeAssetPlugin`] owns the **content**: the entities that
//!   carry Spore geometry and materials.
//!
//! Two owners, one entity kind each. When both spawned a camera, Bevy warned
//! "Camera order ambiguities detected" and the model rendered from whichever
//! camera it picked, so the framing query found "multiple entities" and the
//! symptom looked like a framing bug rather than a duplicate spawn. That is
//! recorded here so it is not repeated.
//!
//! # Nothing is loaded inside a system
//!
//! A `.package` is memory-mapped and its index parsed once; `Spore_Content` is
//! **995 268 083 bytes** over **17 119** index rows. [`crate::prepare`] does that
//! work before any `App` exists, so `--info` is a CI gate and a load failure is
//! reportable without a window. The systems here start from decoded data.
//!
//! # Deferred commands
//!
//! `Commands::spawn` is deferred, so a `Query` in the same system that spawns
//! sees nothing. Every system here that spawns therefore has **no** query, and
//! camera framing runs in `PostStartup` against a resource the caller inserted.
//! Doing both in one system silently leaves the camera at its default position —
//! which is the second time that mistake presented as a framing bug.

use std::sync::Arc;

use bevy::app::Plugin;
use bevy::math::Quat;
use bevy::pbr::StandardMaterial;
use bevy::prelude::*;

use spore_assets::AssetError;
use spore_core::ResourceKey;

use crate::convert::{self, NormalMode};
use crate::domain::materials::{MaterialRoute, MeshMaterialPlan, ModelMaterials};
use crate::domain::model::{DecodeStatus, ModelDefinition, ModelInstance, Orientation};
use crate::domain::paint::{self, PaintBinding};
use crate::domain::scene::{Layout, ObjectExtent, Scene};
use crate::domain::world::ResourceWorld;
use crate::domain::Bounds;
use crate::scene::{MainCamera, OnStage};
use crate::texture::{self, NormalMapOrientation};

/// The normal policy this build stages geometry with.
pub const NORMAL_MODE: NormalMode = NormalMode::Computed;

/// The world floor. Every object is lifted so its own lowest point sits here.
pub const FLOOR_Y: f32 = 0.0;

/// The vertical field of view used when framing a scene.
pub const DEFAULT_VERTICAL_FOV_DEGREES: f32 = 60.0;

// ---------------------------------------------------------------------------
// Resources and components
// ---------------------------------------------------------------------------

/// A loaded scene: the domain description plus every record it needs.
///
/// Inserted before the app is built; see [`crate::build_scene_app`].
#[derive(Resource, Debug, Default)]
pub struct LoadedScene {
    /// What is on stage, with placements resolved.
    pub scene: Scene,
    /// One definition per distinct record, in load order.
    ///
    /// Shared behind an [`Arc`], so a record placed twice is decoded once and
    /// dropped once.
    pub models: Vec<Arc<ModelDefinition>>,
}

impl LoadedScene {
    /// The definition for a record, if it was loaded.
    pub fn model(&self, key: &ResourceKey) -> Option<&Arc<ModelDefinition>> {
        self.models.iter().find(|model| model.key == *key)
    }

    /// Every model instance the scene places, paired with its definition.
    pub fn placements(&self) -> Vec<(&Arc<ModelDefinition>, &ModelInstance)> {
        self.scene
            .instances()
            .into_iter()
            .filter_map(|instance| self.model(&instance.key).map(|model| (model, instance)))
            .collect()
    }

    /// The union of everything placed, transformed by its placement.
    pub fn bounds(&self) -> Bounds {
        let mut bounds: Bounds = None;
        for (model, instance) in self.placements() {
            let Some((lo, hi)) = model.bounds() else {
                continue;
            };
            let [x, y, z] = instance.placement.translation;
            let scale = instance.placement.scale;
            bounds = crate::domain::grow(
                bounds,
                Some((
                    [lo[0] * scale + x, lo[1] * scale + y, lo[2] * scale + z],
                    [hi[0] * scale + x, hi[1] * scale + y, hi[2] * scale + z],
                )),
            );
        }
        bounds
    }

    /// Total triangles across every **definition**, not every placement.
    ///
    /// Placement-independent on purpose: quoting the multiplied count would make
    /// the number depend on the layout, and a triangle count that changes when
    /// you move something is not a property of the data.
    pub fn triangles(&self) -> usize {
        self.models.iter().map(|model| model.triangles()).sum()
    }

    /// How many leaf mesh entities the scene will spawn.
    pub fn part_count(&self) -> usize {
        self.placements()
            .iter()
            .map(|(model, _)| model.parts.len())
            .sum()
    }

    /// How many leaf entities will carry a bound normal map.
    pub fn parts_with_normal_maps(&self) -> usize {
        self.placements()
            .iter()
            .map(|(model, _)| {
                model.materials.as_ref().map_or(0, |materials| {
                    materials
                        .plans()
                        .iter()
                        .filter(|plan| plan.normal_map.is_some())
                        .count()
                })
            })
            .sum()
    }
}

/// The packages the scene was loaded from, kept for a future streaming
/// subsystem.
///
/// Deliberately unread by any system here: the renderer needs vertex buffers and
/// texels, both copied out of the mapping by the decoders, so nothing downstream
/// of the definitions reads the store. Holding the world as a resource makes the
/// ownership explicit without letting a borrow keep 995 MB of mapping alive by
/// accident.
#[derive(Resource, Debug, Clone)]
pub struct WorldResources {
    /// The world, shared.
    pub world: Arc<ResourceWorld>,
}

impl WorldResources {
    /// Bytes of package image the mapping holds.
    pub fn mapped_bytes(&self) -> usize {
        self.world.mapped_bytes()
    }

    /// Which packages were searched, in resolution order.
    pub fn package_names(&self) -> &[String] {
        self.world.package_names()
    }
}

/// The scene's normal-map orientation policy.
///
/// A resource because it is a decision about **the data**, not about the
/// renderer: two scenes in one process could disagree about it, and the answer
/// must be readable at runtime rather than baked into a constant.
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct NormalMapPolicy(pub NormalMapOrientation);

impl NormalMapPolicy {
    /// The orientation to stage with.
    pub const fn orientation(self) -> NormalMapOrientation {
        self.0
    }
}

/// What the camera should frame, and with what field of view.
///
/// A resource rather than a closure added by the caller: it makes the framing
/// step a named system a test can drive, instead of a per-app closure no test
/// can reach.
///
/// **Not** `Reflect`, deliberately, and reading it over BRP therefore returns `null`
/// whether or not it is in the world. That was misread during this work as "the
/// resource was never inserted" and nearly sent me to fix a framing system that was
/// fine. A `null` from BRP means "not registered for reflection", never "absent from
/// the world" -- the same trap as Bevy's own `Transform`, which is invisible to BRP
/// because `bevy_transform` contains zero `register_type` calls in 0.19.1. Reflecting
/// this type would make it readable but is not required by anything, so it stays off.
#[derive(Resource, Debug, Clone, Copy)]
pub struct CameraFraming {
    /// The box to frame.
    pub bounds: ([f32; 3], [f32; 3]),
    /// Vertical field of view, in degrees.
    pub vertical_fov_degrees: f32,
}

/// Marks the parent entity of one scene object.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct SceneObjectMarker {
    /// The object's index in [`Scene::objects`].
    pub index: usize,
    /// `"model"` or `"creature"`.
    pub kind: &'static str,
}

/// Marks a leaf entity carrying one mesh of one model.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModelPartMarker {
    /// The record the geometry came from.
    pub key: ResourceKey,
    /// The mesh's index in the record's mesh table.
    pub mesh_index: u32,
    /// The material id the record associated with the mesh. A bare word.
    pub material_id: u32,
}

/// Every uploaded mask channel of one mesh, carried on the mesh that owns them.
///
/// **The purpose of none of them is established.**
/// `spore_material::claims::MASK_LAYER_PURPOSE` is an `UNKNOWN` non-finding, so
/// this names each channel by the record it came from and the byte lane it was
/// published from, and nothing else. It exists so the channels are addressable
/// and demonstrably uploaded rather than guessed at or discarded.
///
/// **Empty for every record in the current corpus**: with `DXT5` read as BC3,
/// `spore-material` publishes `UnidentifiedLane` on every lane of every
/// non-normal raster record, so there is no mask channel to upload. The
/// component stays because "no channel was published" and "the engine has
/// nowhere to put one" must not look the same, and because the next role
/// derivation that publishes one takes this path unchanged.
///
/// A `Vec` rather than one component per channel, and that is a real constraint
/// rather than a style choice: `Commands::insert` **replaces** a component of the
/// same type, so a per-channel component would leave exactly one of a mesh's
/// channels on the entity — the last one inserted — and the others would vanish
/// with no error.
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub struct MaskChannels {
    /// The channels, in slot then channel order.
    pub planes: Vec<MaskChannelBinding>,
}

impl MaskChannels {
    /// How many channels this mesh carries.
    pub fn len(&self) -> usize {
        self.planes.len()
    }

    /// Whether the mesh carries none.
    pub fn is_empty(&self) -> bool {
        self.planes.is_empty()
    }
}

/// One uploaded mask channel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MaskChannelBinding {
    /// The record the channel came from.
    pub key: ResourceKey,
    /// Which of that record's layers. `0` for every record measured.
    pub layer_index: usize,
    /// The byte lane, as `spore-material` published it.
    pub lane: spore_material::Lane,
    /// The uploaded single-channel image.
    pub handle: Handle<Image>,
}

/// The colour one mesh was given, carried on the entity.
///
/// Kept beside [`MaskChannels`] for the same reason and with the same
/// discipline: a decision that is not addressable at runtime is a decision a
/// later reader cannot check. The [`crate::domain::paint::MeshPaint`] it holds
/// carries its own evidence grade and its `paintregion` disposition, so
/// "why is this mesh this colour" is answerable from the entity alone.
#[derive(Component, Debug, Clone, PartialEq)]
pub struct MeshPaintMarker {
    /// The domain decision, verbatim and undecorated.
    pub paint: crate::domain::paint::MeshPaint,
}

/// What happened when the scene was put on stage.
#[derive(Resource, Debug, Clone, PartialEq)]
pub struct SceneOutcome {
    /// Parent entities spawned.
    pub objects: usize,
    /// Leaf mesh entities spawned.
    pub parts: usize,
    /// Triangles across every definition.
    pub triangles: usize,
    /// Leaf entities carrying a bound normal map.
    pub parts_with_normal_maps: usize,
    /// Leaf entities carrying a base colour from a `sporemodel` record.
    pub parts_painted: usize,
    /// Mask planes uploaded.
    pub mask_channels: usize,
    /// The union of what was placed, for camera framing.
    pub bounds: Bounds,
}

/// What the loader decided before the window opened.
#[derive(Debug)]
pub struct PrepareOutcome {
    /// What to stage.
    pub loaded: LoadedScene,
    /// One report per record, in load order.
    pub reports: Vec<ModelReport>,
    /// The world, for insertion as a resource.
    pub world: Arc<ResourceWorld>,
}

impl PrepareOutcome {
    /// Every report line, in a fixed order: one `STAGED v1`, one `MATERIAL v1`
    /// and one `PAINT v1` per record, then one `SCENE v1` summary.
    pub fn report_lines(&self) -> Vec<String> {
        let records = self.reports.len();
        let mut lines = Vec::with_capacity(records * 3 + 1);
        for (index, report) in self.reports.iter().enumerate() {
            lines.push(report.staged_line());
            lines.push(report.material_line(index, records));
            lines.push(report.paint_line(index, records));
        }
        lines.push(self.scene_line());
        lines
    }

    /// The `OPENSPORE-SCENE v1` summary.
    pub fn scene_line(&self) -> String {
        let parts: usize = self.loaded.models.iter().map(|m| m.parts.len()).sum();
        let normal_maps: usize = self.reports.iter().map(ModelReport::normal_maps).sum();
        let masks: usize = self.reports.iter().map(ModelReport::mask_channels).sum();
        let painted = self
            .reports
            .iter()
            .filter(|report| report.paint.is_bound())
            .count();
        format!(
            "OPENSPORE-SCENE v1 objects={} models={} parts={} triangles={} \
             normal_maps_bound={normal_maps} mask_channels_bound={masks} \
             models_painted={painted} packages={} mapped_bytes={}",
            self.loaded.scene.len(),
            self.loaded.models.len(),
            parts,
            self.loaded.triangles(),
            self.world.package_names().join(","),
            self.world.mapped_bytes(),
        )
    }
}

/// One record's decode outcome, for the report lines.
#[derive(Debug, Clone, PartialEq)]
pub struct ModelReport {
    /// Which record.
    pub key: ResourceKey,
    /// Which package answered.
    pub package_name: String,
    /// The container it decoded as.
    pub format: String,
    /// Meshes in the record.
    pub meshes: usize,
    /// Triangles, after fan expansion.
    pub triangles: usize,
    /// The record's bounds.
    pub bounds: Bounds,
    /// How far the decode got.
    pub decode: DecodeStatus,
    /// Texture references resolved, out of those attempted.
    pub textures_resolved: usize,
    /// Texture references attempted.
    pub textures_attempted: usize,
    /// Whether texture resolution was requested at all.
    pub textures_requested: bool,
    /// The role table, when textures were resolved.
    pub materials: Option<ModelMaterials>,
    /// The per-mesh plans, when textures were resolved.
    pub plans: Vec<MeshMaterialPlan>,
    /// Which route produced the role table.
    pub route: Option<MaterialRoute>,
    /// Where the colour came from, or why none was found.
    pub paint: PaintBinding,
}

impl ModelReport {
    /// Plans carrying a bound normal map.
    pub fn normal_maps(&self) -> usize {
        self.plans
            .iter()
            .filter(|plan| plan.normal_map.is_some())
            .count()
    }

    /// Mask planes the plans call for.
    pub fn mask_channels(&self) -> usize {
        self.plans.iter().map(|plan| plan.masks.len()).sum()
    }

    /// The `OPENSPORE-PAINT v1` line for this record.
    pub fn paint_line(&self, index: usize, records: usize) -> String {
        self.paint.describe(&self.key, index, records)
    }

    /// The `OPENSPORE-STAGED v1` line for this record.
    ///
    /// **The field set and order are unchanged** from the line this engine has
    /// always printed, so an existing consumer keeps working. `textures=n/m`
    /// distinguishes "we looked and there were none" (`0/0` with
    /// `textures_requested == false`) from "all m references failed" (`0/m`).
    pub fn staged_line(&self) -> String {
        let (lo, hi) = self.bounds.unwrap_or(([0.0; 3], [0.0; 3]));
        format!(
            "OPENSPORE-STAGED v1 key={} package={} format={} meshes={} triangles={} \
             bounds_min={:.6},{:.6},{:.6} bounds_max={:.6},{:.6},{:.6} normals=derived \
             textures={}/{} decode={}",
            self.key,
            self.package_name,
            self.format,
            self.meshes,
            self.triangles,
            lo[0],
            lo[1],
            lo[2],
            hi[0],
            hi[1],
            hi[2],
            self.textures_resolved,
            self.textures_attempted,
            self.decode.describe(),
        )
    }

    /// The `OPENSPORE-MATERIAL v1` line for this record.
    ///
    /// New, and additive: it says what the roles were, which surface fed which
    /// channel, and — the field a reader should not skip — that the per-mesh cut
    /// is an **inference**.
    pub fn material_line(&self, index: usize, records: usize) -> String {
        let Some(materials) = &self.materials else {
            return format!(
                "OPENSPORE-MATERIAL v1 record={index}/{records} key={} roles=unavailable \
                 reason=textures_not_requested",
                self.key
            );
        };
        let roles = materials
            .role_census()
            .iter()
            .map(|(role, count)| format!("{}:{count}", role.as_str()))
            .collect::<Vec<_>>()
            .join(",");
        let agreed = materials
            .ordinal_agreement
            .map_or(0, |agreement| agreement.agreed);
        let compared = materials
            .ordinal_agreement
            .map_or(0, |agreement| agreement.compared);
        let plans = self
            .plans
            .iter()
            .map(MeshMaterialPlan::describe)
            .collect::<Vec<_>>()
            .join(" | ");
        format!(
            "OPENSPORE-MATERIAL v1 record={index}/{records} key={} meshes={} scope={} \
             refs_per_mesh={} inferred={} route={} shape={} ordinals={agreed}/{compared} \
             roles={} normal_maps_bound={} mask_channels_bound={} plans=[{plans}]",
            self.key,
            self.meshes,
            materials.scope.as_str(),
            materials
                .scope
                .refs_per_mesh()
                .map_or_else(|| "unavailable".to_owned(), |per_mesh| per_mesh.to_string()),
            materials.scope.is_inferred(),
            self.route.map_or("unavailable", MaterialRoute::as_str),
            materials
                .observed_shape
                .as_ref()
                .map_or("unavailable", spore_material::TexSetShape::as_str),
            if roles.is_empty() { "none" } else { &roles },
            self.normal_maps(),
            self.mask_channels(),
        )
    }
}

// ---------------------------------------------------------------------------
// Loading, before Bevy
// ---------------------------------------------------------------------------

/// Loads and decodes a scene from a world. No Bevy, no GPU, no window.
///
/// `textured` gates texture resolution, because resolving one reference costs a
/// record read and a decode, and must stay opt-in.
///
/// `sporemodel` is the `--sporemodel` override. When it is `None` each record's
/// colour is resolved from its own asset family; see
/// [`crate::domain::paint`]. Colour resolution is **not** gated on `textured`:
/// the two read different records in different packages, and a model can be
/// untextured and still have a colour.
pub fn prepare_scene(
    world: Arc<ResourceWorld>,
    mut scene: Scene,
    textured: bool,
    assumed_texture_type: u32,
    layout: Layout,
    gap: f32,
    sporemodel: Option<ResourceKey>,
) -> Result<(PrepareOutcome, Vec<Option<ObjectExtent>>), AssetError> {
    let mut models: Vec<Arc<ModelDefinition>> = Vec::new();
    for key in scene.records() {
        let loaded = world.load_model(&key)?;
        let materials = if textured {
            Some(ModelMaterials::resolve(
                world.store(),
                &key,
                &loaded,
                assumed_texture_type,
            ))
        } else {
            None
        };
        let paint = paint::resolve(world.store(), &key, sporemodel);
        models.push(Arc::new(ModelDefinition::from_loaded(
            &loaded, materials, paint,
        )));
    }

    let extents = object_extents(&scene, &models);
    scene.layout(&extents, layout, gap, FLOOR_Y);

    let reports = models.iter().map(|model| report_for(model)).collect();
    Ok((
        PrepareOutcome {
            loaded: LoadedScene { scene, models },
            reports,
            world,
        },
        extents,
    ))
}

/// The extent of each object in its own frame, before placement.
fn object_extents(scene: &Scene, models: &[Arc<ModelDefinition>]) -> Vec<Option<ObjectExtent>> {
    scene
        .objects
        .iter()
        .map(|object| {
            let mut merged: Option<ObjectExtent> = None;
            for instance in object.instances() {
                let Some(model) = models.iter().find(|model| model.key == instance.key) else {
                    continue;
                };
                let Some((lo, hi)) = model.bounds() else {
                    continue;
                };
                merged = Some(match merged {
                    None => ObjectExtent {
                        bounds_min: lo,
                        bounds_max: hi,
                    },
                    Some(current) => ObjectExtent {
                        bounds_min: [
                            current.bounds_min[0].min(lo[0]),
                            current.bounds_min[1].min(lo[1]),
                            current.bounds_min[2].min(lo[2]),
                        ],
                        bounds_max: [
                            current.bounds_max[0].max(hi[0]),
                            current.bounds_max[1].max(hi[1]),
                            current.bounds_max[2].max(hi[2]),
                        ],
                    },
                });
            }
            merged
        })
        .collect()
}

fn report_for(model: &ModelDefinition) -> ModelReport {
    let plans = model
        .materials
        .as_ref()
        .map_or_else(Vec::new, ModelMaterials::plans);
    let materials = model.materials.clone();
    ModelReport {
        key: model.key,
        package_name: model.package_name.clone(),
        format: model.format.as_str().to_owned(),
        meshes: model.parts.len(),
        triangles: model.triangles(),
        bounds: model.bounds(),
        decode: model.decode.clone(),
        textures_resolved: materials.as_ref().map_or(0, ModelMaterials::resolved_count),
        textures_attempted: materials.as_ref().map_or(0, ModelMaterials::attempted),
        // "Asked and got nothing" is not "did not ask": the report line has to be
        // able to tell `0/0` from `0/2`.
        textures_requested: model.materials.is_some(),
        materials,
        plans,
        route: model.materials.as_ref().map(|table| table.route),
        paint: model.paint.clone(),
    }
}

// ---------------------------------------------------------------------------
// Spawning
// ---------------------------------------------------------------------------

/// Spawns entities for an already-loaded scene.
///
/// Takes the decode result rather than looking one up, because the lookup
/// already happened outside the render loop; re-doing it here would make the
/// window's contents depend on when it was opened.
pub fn spawn_scene(
    loaded: &LoadedScene,
    orientation: NormalMapOrientation,
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
) -> SceneOutcome {
    let mut outcome = SceneOutcome {
        objects: loaded.scene.len(),
        parts: 0,
        triangles: loaded.triangles(),
        parts_with_normal_maps: 0,
        parts_painted: 0,
        mask_channels: 0,
        bounds: loaded.bounds(),
    };

    for (index, object) in loaded.scene.objects.iter().enumerate() {
        // One parent per object, and it owns the object's placement. A
        // creature's parts therefore share one frame, which is what "a creature
        // built from several records" has to mean for the transform to be
        // meaningful; the leaves stay in identity space.
        let parent = commands
            .spawn((
                object
                    .instances()
                    .first()
                    .map_or_else(Transform::default, |instance| {
                        Transform::from_translation(Vec3::from(instance.placement.translation))
                            .with_rotation(orientation_to_quat(instance.placement.orientation))
                            .with_scale(Vec3::splat(instance.placement.scale))
                    }),
                SceneObjectMarker {
                    index,
                    kind: object.kind(),
                },
            ))
            .id();
        for instance in object.instances() {
            let Some(model) = loaded.model(&instance.key) else {
                continue;
            };
            // Once per record, not once per mesh: `plans()` cuts the flat
            // reference list per call and doing that 47 000 times would be a
            // different bug than the one this engine came with.
            let plans = model
                .materials
                .as_ref()
                .map_or_else(Vec::new, ModelMaterials::plans);
            let textures = model
                .materials
                .as_ref()
                .map(|table| table.textures.as_slice())
                .unwrap_or(&[]);
            for part in &model.parts {
                let buffers = part.buffers();
                let bevy_mesh = convert::to_bevy_mesh(&buffers, NORMAL_MODE);
                // The colour comes from the record, never from a raster: no
                // raster in this corpus decodes as an albedo, and the shader
                // takes both of its colours as constants.
                let mesh_paint = model.mesh_paint(part.mesh_index);
                if mesh_paint.base_color.is_some() {
                    outcome.parts_painted += 1;
                }
                let bound = plans.get(part.mesh_index as usize).map(|plan| {
                    texture::bind_material(
                        plan,
                        textures,
                        orientation,
                        images,
                        mesh_paint.base_color,
                    )
                });
                let material = match &bound {
                    Some(bound) => {
                        if bound.surfaces.normal_map.is_some() {
                            outcome.parts_with_normal_maps += 1;
                        }
                        outcome.mask_channels += bound.mask_count();
                        materials.add(bound.material.clone())
                    }
                    None => materials.add(texture::standard_material(
                        &texture::SurfaceSet::default(),
                        None,
                        mesh_paint.base_color,
                    )),
                };
                // The mask channels ride in the spawn bundle rather than being
                // inserted afterwards, so the entity is complete the moment it
                // exists. Inserting afterwards would also be *one component per
                // channel*, which silently keeps only the last one.
                let planes: Vec<MaskChannelBinding> = bound
                    .as_ref()
                    .map(|bound| {
                        bound
                            .surfaces
                            .masks
                            .iter()
                            .zip(bound.mask_handles.iter())
                            .map(|(mask, handle)| MaskChannelBinding {
                                key: mask.key,
                                layer_index: match mask.kind {
                                    crate::texture::SurfaceKind::MaskChannel {
                                        layer_index,
                                        ..
                                    } => layer_index,
                                    crate::texture::SurfaceKind::TangentSpaceNormal => {
                                        unreachable!("a normal map is never a mask channel")
                                    }
                                },
                                lane: match mask.kind {
                                    crate::texture::SurfaceKind::MaskChannel { lane, .. } => lane,
                                    crate::texture::SurfaceKind::TangentSpaceNormal => {
                                        unreachable!("a normal map is never a mask channel")
                                    }
                                },
                                handle: handle.clone(),
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                let channels = (!planes.is_empty()).then_some(MaskChannels { planes });
                let child = commands
                    .spawn((
                        Mesh3d(meshes.add(bevy_mesh)),
                        MeshMaterial3d(material),
                        // Identity: the placement lives on the parent, so a
                        // creature's parts move together.
                        Transform::default(),
                        OnStage,
                        ModelPartMarker {
                            key: model.key,
                            mesh_index: part.mesh_index,
                            material_id: part.material_id,
                        },
                        MeshPaintMarker { paint: mesh_paint },
                    ))
                    .id();
                if let Some(channels) = channels {
                    commands.entity(child).insert(channels);
                }
                // `ChildOf` on the child alone is NOT enough: Bevy's hierarchy
                // invariant is that the parent carries `Children` too, and its
                // B0004 warning says plainly that a one-sided relationship
                // "will cause inconsistent behaviors". `add_child` maintains
                // both sides, so it is what the engine uses.
                commands.entity(parent).add_child(child);
                outcome.parts += 1;
            }
        }
    }
    outcome
}

// ---------------------------------------------------------------------------
// The plugin
// ---------------------------------------------------------------------------

/// Turns a [`LoadedScene`] into entities.
///
/// The boundary between "a record has been decoded" and "there is something on
/// screen". A second content source — a procedural creature, a mod-provided
/// asset, a test fixture — becomes a second implementation of "produce a
/// `LoadedScene`", with no change here.
#[derive(Debug, Clone, Copy, Default)]
pub struct SporeAssetPlugin;

impl Plugin for SporeAssetPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_loaded_scene);
        app.add_systems(PostStartup, frame_camera_on_the_scene);
    }
}

fn spawn_loaded_scene(
    mut commands: Commands,
    loaded: Res<LoadedScene>,
    policy: Res<NormalMapPolicy>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    // NOTE: there is deliberately NO camera query here. `Commands::spawn` is
    // deferred, so a `Query` in the same system that spawns the camera sees zero
    // cameras -- which would silently leave the camera at its default position.
    // The framing runs in `PostStartup` instead.
    let outcome = spawn_scene(
        &loaded,
        policy.orientation(),
        &mut commands,
        &mut meshes,
        &mut materials,
        &mut images,
    );
    info!(
        "staged {objects} object(s), {parts} mesh entity(ies), {triangles} triangle(s), \
         {normals} with a bound normal map, {painted} with a record colour, \
         {masks} mask plane(s)",
        objects = outcome.objects,
        parts = outcome.parts,
        triangles = outcome.triangles,
        normals = outcome.parts_with_normal_maps,
        painted = outcome.parts_painted,
        masks = outcome.mask_channels,
    );
    for report in loaded
        .models
        .iter()
        .filter_map(|model| model.materials.as_ref())
    {
        for plan in report.plans() {
            info!("{}", plan.describe());
            if let Some(normal) = &plan.normal_map {
                info!("  normal map {}", normal.describe());
            }
        }
    }
    // The colour decision is logged per model, with its grade, because "the
    // creature is green" is only meaningful next to "green came from this
    // record, and this record is the model's colour by inference".
    for model in loaded.models.iter() {
        for part in &model.parts {
            info!("{}", model.mesh_paint(part.mesh_index).describe());
        }
    }
    commands.insert_resource(outcome);
}

/// Frames the camera on the loaded scene's extent.
///
/// Runs in `PostStartup` because the camera is spawned in `Startup` by
/// [`crate::scene::ScenePlugin`] and `Commands::spawn` is deferred.
fn frame_camera_on_the_scene(
    mut cameras: Query<&mut Transform, With<MainCamera>>,
    framing: Option<Res<CameraFraming>>,
) {
    let Some(framing) = framing else {
        return;
    };
    match cameras.single_mut() {
        Ok(mut transform) => {
            frame_camera(&mut transform, framing.bounds, framing.vertical_fov_degrees)
        }
        Err(error) => warn!("could not frame the camera: {error}"),
    }
}

/// Frames the camera on a bounding box.
///
/// Deliberately simple: fit the box's bounding sphere into the vertical field of
/// view with a margin, from a fixed oblique angle. Spore's own camera behaviour
/// is a separate and much later question, and pretending to know it here would
/// be an invention.
pub fn frame_camera(
    camera: &mut Transform,
    bounds: ([f32; 3], [f32; 3]),
    vertical_fov_degrees: f32,
) {
    let center = Vec3::new(
        (bounds.0[0] + bounds.1[0]) * 0.5,
        (bounds.0[1] + bounds.1[1]) * 0.5,
        (bounds.0[2] + bounds.1[2]) * 0.5,
    );
    let extent = Vec3::new(
        bounds.1[0] - bounds.0[0],
        bounds.1[1] - bounds.0[1],
        bounds.1[2] - bounds.0[2],
    );
    let radius = 0.5 * extent.length().max(1e-3);
    let fov = vertical_fov_degrees.to_radians();
    // distance = radius / sin(fov / 2), with a 1.25 margin so the model never
    // touches the edge of the frame.
    let distance = radius / (fov * 0.5).sin().max(1e-3) * 1.25;
    let direction = Vec3::new(0.0, 0.42, 0.91).normalize();
    *camera =
        Transform::from_translation(center + direction * distance).looking_at(center, Vec3::Y);
}

/// Turns an [`Orientation`] into the quaternion a `Transform` carries.
///
/// # The transpose, once, in one place
///
/// `Quat::from_mat3` takes a **column-major** basis in glam's convention, while
/// the creator save format spells `<orientation>` as three **rows**. So the
/// transpose happens here, explicitly, and this is the only function in the crate
/// that does it.
///
/// It is a function rather than an inline match because two callers now need it —
/// the record path above and the creator projection in [`crate::creator`] — and a
/// second copy of a transpose is a second answer to "which way does a rotation
/// face". Getting it backwards mirrors a part instead of rotating it, which
/// renders plausibly and is exactly the kind of defect a screenshot cannot catch.
///
/// The `YRotation` branch stays because every hand-authored placement still uses
/// it, and because it is a *stronger* claim than the matrix: it says the author
/// wrote a Y rotation. Collapsing a `General` to one would throw away exactly the
/// information a creator block carries.
pub fn orientation_to_quat(orientation: Orientation) -> Quat {
    match orientation {
        Orientation::YRotation(degrees) => Quat::from_rotation_y(degrees.to_radians()),
        Orientation::General(m) => {
            let [r00, r01, r02, r10, r11, r12, r20, r21, r22] = m.to_array();
            Quat::from_mat3(&bevy::math::Mat3::from_cols_array(&[
                r00, r10, r20, r01, r11, r21, r02, r12, r22,
            ]))
        }
    }
}

/// Turns a loader failure into something a person can act on.
pub fn describe(error: &AssetError) -> String {
    match error {
        AssetError::NotFound {
            key,
            searched,
            candidates,
        } => {
            let mut message =
                format!("record {key} is not in any of the {searched} package(s) searched");
            if !candidates.is_empty() {
                let list: Vec<String> =
                    candidates.iter().take(5).map(ToString::to_string).collect();
                message.push_str(&format!(
                    "; nearby identities of that type: {}",
                    list.join(", ")
                ));
            }
            message
        }
        AssetError::EmptyStore => {
            "no Spore package was loaded. Pass --package <path> pointing at a .package file."
                .to_owned()
        }
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::model::{DecodeStatus, ModelPart};
    use spore_assets::ModelFormat;
    use spore_gmdl::Mesh;

    fn triangle_mesh(min_y: f32) -> Mesh {
        Mesh {
            positions: vec![[0.0, min_y, 0.0], [1.0, min_y, 0.0], [0.0, 1.0, 0.0]],
            normals: vec![[0.0, 0.0, 1.0]; 3],
            uvs: vec![[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]],
            indices: vec![0, 1, 2],
            topology: spore_gmdl::Topology::TriangleList,
            bounds_min: [0.0, min_y, 0.0],
            bounds_max: [1.0, 1.0, 0.0],
            radius: 1.0,
        }
    }

    fn definition(key: ResourceKey, meshes: Vec<Mesh>) -> Arc<ModelDefinition> {
        Arc::new(ModelDefinition {
            key,
            package_name: "mini".into(),
            format: ModelFormat::Gmdl,
            parts: meshes
                .into_iter()
                .enumerate()
                .map(|(index, mesh)| ModelPart {
                    mesh_index: index as u32,
                    mesh,
                    material_id: 0x40DE_AD00,
                })
                .collect(),
            materials: None,
            paint: PaintBinding::Unavailable {
                reason: crate::domain::paint::PaintUnavailableReason::NoSporemodelRecords,
                fact: spore_core::evidence::Fact::unavailable("unit test"),
            },
            decode: DecodeStatus::Complete,
        })
    }

    fn loaded_scene(
        objects: Vec<crate::domain::scene::SceneObject>,
        models: Vec<Arc<ModelDefinition>>,
    ) -> LoadedScene {
        LoadedScene {
            scene: Scene { objects },
            models,
        }
    }

    #[test]
    fn an_empty_scene_bounds_to_nothing_rather_than_to_the_origin() {
        let scene = loaded_scene(Vec::new(), Vec::new());
        assert_eq!(scene.bounds(), None);
        assert_eq!(scene.triangles(), 0);
        assert_eq!(scene.part_count(), 0);
    }

    #[test]
    fn bounds_include_each_placement_and_union_across_objects() {
        let key = ResourceKey::new(1, 2, 3);
        let model = definition(key, vec![triangle_mesh(-2.0)]);
        let mut first = ModelInstance::new(key, Default::default());
        first.placement.translation = [0.0, 2.0, 0.0];
        let mut second = ModelInstance::new(key, Default::default());
        second.placement.translation = [5.0, 0.0, 0.0];
        let scene = loaded_scene(
            vec![
                crate::domain::scene::SceneObject::Model(first),
                crate::domain::scene::SceneObject::Model(second),
            ],
            vec![model],
        );
        let (lo, hi) = scene.bounds().unwrap();
        // The first instance was lifted by 2 so its lowest point sits at 0; the
        // second was placed at y = 0 and so still reaches -2. The union has to
        // show both, which is what makes the number checkable.
        assert_eq!(lo, [0.0, -2.0, 0.0]);
        assert_eq!(hi, [6.0, 3.0, 0.0]);
        assert_eq!(scene.part_count(), 2);
        assert_eq!(scene.triangles(), 1, "counted once per definition");
    }

    #[test]
    fn a_definition_with_no_materials_reports_no_normal_maps() {
        let key = ResourceKey::new(1, 2, 3);
        let model = definition(key, vec![triangle_mesh(0.0)]);
        let scene = loaded_scene(
            vec![crate::domain::scene::SceneObject::Model(
                ModelInstance::new(key, Default::default()),
            )],
            vec![model],
        );
        assert_eq!(scene.parts_with_normal_maps(), 0);
        assert!(scene.model(&key).is_some());
        assert!(scene.model(&ResourceKey::new(9, 9, 9)).is_none());
    }

    #[test]
    fn the_staged_line_keeps_the_v1_field_set() {
        let key = ResourceKey::new(0x00E6_BCE5, 0x4062_7100, 0x067C_79D2);
        let model = definition(key, vec![triangle_mesh(0.0)]);
        let report = report_for(&model);
        let line = report.staged_line();
        assert!(line.starts_with("OPENSPORE-STAGED v1 "), "{line}");
        for field in [
            "key=",
            "package=",
            "format=",
            "meshes=",
            "triangles=",
            "bounds_min=",
            "bounds_max=",
            "normals=derived",
            "textures=",
            "decode=",
        ] {
            assert!(line.contains(field), "the v1 field set changed: {line}");
        }
        assert!(line.contains("textures=0/0"), "{line}");
        assert!(!report.textures_requested, "0/0 must mean 'not asked' here");
    }

    #[test]
    fn the_material_line_says_not_requested_rather_than_none_found() {
        let key = ResourceKey::new(1, 2, 3);
        let report = report_for(&definition(key, vec![triangle_mesh(0.0)]));
        let line = report.material_line(0, 1);
        assert!(line.starts_with("OPENSPORE-MATERIAL v1 "), "{line}");
        assert!(line.contains("reason=textures_not_requested"), "{line}");
        assert!(!line.contains("roles=unidentified"), "{line}");
    }

    #[test]
    fn the_scene_line_reports_counts_and_the_mapping_cost() {
        let key = ResourceKey::new(1, 2, 3);
        let scene = loaded_scene(
            vec![
                crate::domain::scene::SceneObject::Model(ModelInstance::new(
                    key,
                    Default::default(),
                )),
                crate::domain::scene::SceneObject::Creature(
                    crate::domain::creature::Creature::single(ModelInstance::new(
                        key,
                        Default::default(),
                    )),
                ),
            ],
            vec![definition(key, vec![triangle_mesh(0.0)])],
        );
        let outcome = PrepareOutcome {
            loaded: scene,
            reports: Vec::new(),
            world: Arc::new(ResourceWorld::open(&Vec::<String>::new()).unwrap()),
        };
        let line = outcome.scene_line();
        assert!(line.starts_with("OPENSPORE-SCENE v1 "), "{line}");
        assert!(line.contains("objects=2"), "{line}");
        assert!(line.contains("models=1"), "{line}");
        assert!(line.contains("parts=1"), "{line}");
        assert!(line.contains("mapped_bytes=0"), "{line}");
        let lines = outcome.report_lines();
        assert_eq!(lines.len(), 1, "no records, so only the summary");
        assert!(lines[0].starts_with("OPENSPORE-SCENE v1 "));
    }

    #[test]
    fn framing_a_scene_uses_the_inserted_resource() {
        let mut transform = Transform::default();
        frame_camera(
            &mut transform,
            ([-10.0, -10.0, -10.0], [10.0, 10.0, 10.0]),
            60.0,
        );
        assert!(transform.translation.length() > 10.0);
        assert!(
            transform.forward().dot(-transform.translation) > 0.0,
            "camera is not looking at the centre"
        );
    }

    #[test]
    fn framing_survives_a_degenerate_box() {
        let mut transform = Transform::default();
        frame_camera(&mut transform, ([0.0; 3], [0.0; 3]), 60.0);
        assert!(transform.translation.is_finite());
    }

    #[test]
    fn the_row_major_transpose_is_the_only_place_a_rotation_is_built() {
        // A right-handed 90-degree rotation about +Z, written ROW-major as the
        // creator format spells it: R_z(90) applied to +X is +Y, so row 0 is
        // (0,-1,0). A matrix whose transpose has the same property is a rotation
        // by -90, and building the quaternion from it would silently un-rotate
        // every part in the corpus.
        let row_major = spore_core::Matrix3 {
            row0: [0.0, -1.0, 0.0],
            row1: [1.0, 0.0, 0.0],
            row2: [0.0, 0.0, 1.0],
        };
        let rotation = orientation_to_quat(Orientation::General(row_major));
        let axis = Vec3::X;
        let expected = axis.rotate_z(core::f32::consts::FRAC_PI_2);
        assert!(
            (rotation * axis - expected).length() < 1e-5,
            "+X must map to {expected:?}, not {:?}; a rotation built without the transpose \
             mirrors the part rather than rotating it",
            rotation * axis
        );
        // And the basis is orthonormal, so `from_mat3` had a real rotation to read.
        assert!(rotation.is_normalized());
    }

    #[test]
    fn a_y_rotation_orientation_is_not_narrowed_through_the_matrix() {
        // The `YRotation` arm is a stronger claim, not a fallback, and it must stay
        // reachable: `as_y_rotation` on the *matrix* would accept this, and the
        // 0-degree case is exactly where a `-sin(0.0) = -0.0` bit comparison goes
        // wrong. See `spore_core::Orientation::is_y_rotation`.
        let zero = orientation_to_quat(Orientation::YRotation(0.0));
        assert_eq!(zero, Quat::IDENTITY);
        let quarter = orientation_to_quat(Orientation::YRotation(90.0));
        assert!(
            (quarter * Vec3::Z - Vec3::X).length() < 1e-5,
            "right-handed +90 about Y takes +Z to +X, got {:?}",
            quarter * Vec3::Z
        );
    }

    #[test]
    fn a_missing_package_is_reported_with_the_fix() {
        assert_eq!(
            describe(&AssetError::EmptyStore),
            "no Spore package was loaded. Pass --package <path> pointing at a .package file."
        );
    }

    #[test]
    fn not_found_names_the_record_and_offers_alternatives() {
        let error = AssetError::NotFound {
            key: ResourceKey::new(0x00E6_BCE5, 1, 2),
            searched: 3,
            candidates: vec![ResourceKey::new(0x00E6_BCE5, 3, 4)],
        };
        let message = describe(&error);
        assert!(
            message.contains("0x00e6bce5:0x00000001:0x00000002"),
            "{message}"
        );
        assert!(message.contains("3 package"), "{message}");
        let empty = AssetError::NotFound {
            key: ResourceKey::new(1, 2, 3),
            searched: 1,
            candidates: Vec::new(),
        };
        assert!(!describe(&empty).contains("nearby"));
    }
}
