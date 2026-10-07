//! The creator projection: a real `<sporemodel>` document into a Bevy scene.
//!
//! # What this module is for
//!
//! [`spore-morphology`] answers "what parts does this document place, and where".
//! This module answers the last question in the chain: **what does that look like
//! on screen, and what does an agent see while it happens.** One entity per
//! resolved block, parented the way the document nests, positioned by the
//! projection's composed world transform, and a resource that reports the whole
//! thing honestly -- including the blocks that produced nothing.
//!
//! ```text
//!   .package --> <sporemodel> record --> spore_creator_core::CreatorDocument
//!                                            |
//!   .package --> rigblock/prop/rw4/gmsh --> spore_morphology::compose
//!                                            |
//!                                   spore_morphology::project
//!                                            |  one entry per block, always
//!                                            v
//!                                   this module --> Bevy entities + state
//! ```
//!
//! # The one promise
//!
//! **An unresolved block is never invisible.** `spore-morphology` guarantees one
//! projected entry per authored block; this module guarantees the *count* and the
//! *reasons* reach the caller -- in [`CreatorRuntimeState`], in every report line,
//! and in the spawn outcome. There is no path from a document to a rendered scene
//! that drops the failures on the floor, and [`CreatorOutcome::spawned`] is a
//! separate number from [`CreatorOutcome::resolved`] precisely so "12 of 13 blocks
//! are on screen" cannot be read as "13 blocks are on screen".
//!
//! # Parentage, and why the *local* transform is what reaches `Transform`
//!
//! Each entity carries its block's **local** transform and is a child of the
//! entity for its authored parent. Bevy's hierarchy composes
//! `child_world = parent_world * child_local`, which is exactly the projection's
//! `parent * local` rule -- so the world transform an entity ends up with is the
//! projection's `world_position` / `world_orientation`.
//!
//! Writing `world_position` onto the child *and* parenting it would apply the
//! parent chain twice, and the outermost part is the only one that would look
//! wrong. `tests/creator_projection.rs` asserts the composed `GlobalTransform`
//! against the projection's world values rather than trusting the argument above,
//! because this is the single place where a plausible-looking mistake would
//! render.
//!
//! # One owner per entity kind
//!
//! [`crate::scene::ScenePlugin`] owns the camera, the lights and the ground, and
//! it is the only thing in the workspace that spawns a camera. This module spawns
//! no camera: it *frames* the one that exists, in `PostStartup`, from the
//! projection's own bounds. Two owners is how a framing bug once wore a framing
//! bug's clothes (see `AGENTS.md`).
//!
//! # Bevy-free arithmetic, Bevy-bound spawning
//!
//! [`load_creator`] is a pure function over packages and a document identity: no
//! window, no GPU, no ECS. Only [`spawn_projection`] and the three systems below
//! touch Bevy, which is what keeps `--info` a CI gate and what lets the whole
//! projection be asserted in `cargo test`.

use std::cell::Cell;
use std::path::{Path, PathBuf};

use bevy::pbr::StandardMaterial;
use bevy::prelude::*;
use bevy::reflect::Reflect;

use spore_assets::{AssetError, ContentStore};
use spore_core::ResourceKey;
use spore_creator_core::persist;
use spore_creator_core::{parse_sporemodel, CreatorDocument, Family};
use spore_morphology::{project, Projection, RecordSource};

use crate::convert::{self, MeshBuffers, Topology};
use crate::domain::world::ResourceWorld;
use crate::loader::{
    orientation_to_quat, CameraFraming, DEFAULT_VERTICAL_FOV_DEGREES, NORMAL_MODE,
};
use crate::scene::{MainCamera, OnStage, ASSET_TINT};
use crate::texture;

/// The tint a staged part wears when it is the selected block.
///
/// OpenSpore's own choice, in the same family as [`crate::scene::ASSET_TINT`]: no
/// raster record in this corpus decodes as a base colour, so any hue would be an
/// invention and the neutral grey says "this is a debug tint, not a material
/// fact". This one is *deliberately* a hue, because it marks a selection, which
/// is a fact about the editor rather than about the data.
pub const SELECTION_TINT: Color = Color::srgb(0.94, 0.63, 0.22);

/// The name every creator root entity carries.
///
/// Entities are named because BRP's `world.find_entities_by_name` is the only way
/// an agent can address one without a component filter, and OpenSpore set no
/// names at all until now (recorded in `AGENTS.md` and `docs/BRP.md`). A `Name`
/// is **not** reflected in Bevy 0.19 -- `bevy_transform` registers no types -- so
/// this is for addressing, not for reading; [`CreatorRuntimeState`] carries the
/// values.
pub const ROOT_NAME: &str = "creator.root";

/// The name the entity for block `index` carries.
pub fn block_name(index: usize) -> String {
    format!("creator.block.{index}")
}

/// Where a creator document came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DocumentSource {
    /// A `<sporemodel>` record in one of the open packages.
    ///
    /// The family is carried because several measurements differ per family
    /// (paint presence, `<handles>`, `<limbtype>`) and a caller that does not
    /// know it cannot interpret those differences.
    Record {
        /// The record's identity.
        key: ResourceKey,
        /// Which of the six authoring families it belongs to.
        family: Family,
    },
    /// A `spore-creator-doc/1` file, as written by `--creator-save`.
    ///
    /// **The family is `None` here, and that is a real absence, not a default.**
    /// The interchange format carries no type word, so a document read back from
    /// disk cannot say whether it was a `crt`, a `cll` or a `flr`. Nothing is
    /// guessed: the runtime state reports `"unknown"`.
    SavedFile(PathBuf),
}

impl DocumentSource {
    /// A one-word label for a report line.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Record { .. } => "record",
            Self::SavedFile(_) => "saved-file",
        }
    }

    /// The record identity, when there is one.
    pub fn key(&self) -> Option<ResourceKey> {
        match self {
            Self::Record { key, .. } => Some(*key),
            Self::SavedFile(_) => None,
        }
    }

    /// The authoring family, or `None` for a document read from a file.
    pub fn family(&self) -> Option<Family> {
        match self {
            Self::Record { family, .. } => Some(*family),
            Self::SavedFile(_) => None,
        }
    }

    /// Builds a record source, refusing a type id this build does not know.
    ///
    /// The family is derived from the record's **own** type word rather than
    /// trusted from the caller, because a document's grammar is family-specific
    /// and applying the wrong one produces a document that parses into the wrong
    /// shape.
    pub fn for_record(key: ResourceKey) -> Result<Self, CreatorLoadError> {
        match Family::from_type_id(key.type_id) {
            Some(family) => Ok(Self::Record { key, family }),
            None => Err(CreatorLoadError::UnknownFamily {
                type_id: key.type_id,
                key,
            }),
        }
    }
}

/// A document, resolved and projected: everything needed to draw it.
#[derive(Debug, Clone)]
pub struct CreatorStage {
    /// The authoring source of truth, unmutated.
    pub document: CreatorDocument,
    /// One entry per block, resolved or explicitly not.
    pub projection: Projection,
    /// Where the document came from.
    pub source: DocumentSource,
    /// How many packages were opened for it.
    pub packages: usize,
    /// Record lookups the resolution made.
    pub lookups: usize,
    /// Lookups that found nothing.
    pub misses: usize,
    /// Lookups that found an index entry but could not extract bytes from it.
    ///
    /// Counted apart from [`Self::misses`] so a block reported as "no such
    /// record" is never silently the result of an extraction failure.
    pub read_errors: usize,
}

impl CreatorStage {
    /// How many authored blocks there are.
    pub fn block_count(&self) -> usize {
        self.projection.blocks().len()
    }

    /// How many resolved blocks produced no drawable geometry.
    ///
    /// A resolved block whose `gmsh` record would not decode is **resolved but
    /// not visible**, and collapsing the two is exactly the lie
    /// [`Self::visible_block_count`] exists to prevent.
    pub fn without_geometry_count(&self) -> usize {
        self.projection
            .resolved()
            .filter(|block| {
                block
                    .part
                    .as_ref()
                    .is_some_and(|part| part.geometry.is_none())
            })
            .count()
    }

    /// How many blocks will produce a drawable mesh.
    pub fn visible_block_count(&self) -> usize {
        self.projection
            .resolved()
            .filter(|block| {
                block
                    .part
                    .as_ref()
                    .is_some_and(|part| part.geometry.is_some())
            })
            .count()
    }

    /// The `OPENSPORE-CREATOR v1` summary line, then one line per block that did
    /// not make it onto the screen.
    ///
    /// The summary carries both halves of the split, so a caller that reads only
    /// the first line still learns that a creature is incomplete.
    pub fn report_lines(&self) -> Vec<String> {
        let counts = self.projection.counts();
        let family = match self.source.family() {
            Some(family) => format!(
                "{} (group low bits 0x{:04x})",
                family.name(),
                family.family_bits()
            ),
            None => "unknown".to_owned(),
        };
        let mut lines = vec![format!(
            "OPENSPORE-CREATOR v1 source={} document={} family={family} \
             formatversion={} blocks={} resolved={} unresolved={} \
             without_geometry={} visible={} triangles={} vertices={} bones={} depth={} \
             packages={} lookups={} misses={} read_errors={}",
            self.source.as_str(),
            self.document_identity(),
            self.projection
                .document_format_version
                .map_or_else(|| "none".to_owned(), |version| version.to_string()),
            self.block_count(),
            counts.resolved,
            counts.unresolved,
            self.without_geometry_count(),
            self.visible_block_count(),
            self.projection.total_triangles(),
            self.projection.total_vertices(),
            self.projection.total_bones(),
            self.projection.max_depth(),
            self.packages,
            self.lookups,
            self.misses,
            self.read_errors,
        )];
        for block in self.projection.blocks() {
            let parent = block
                .parent
                .map_or_else(|| "none".to_owned(), |parent| parent.to_string());
            match (&block.part, &block.unresolved) {
                (Some(part), _) if part.geometry.is_none() => lines.push(format!(
                    "OPENSPORE-CREATOR-BLOCK v1 block={} parent={parent} depth={} resolved=true \
                     without_geometry=true container={}",
                    block.block, block.ancestor_depth, part.container_key,
                )),
                (None, Some(unresolved)) => lines.push(format!(
                    "OPENSPORE-CREATOR-BLOCK v1 block={} parent={parent} depth={} resolved=false \
                     stage={} reason={}",
                    block.block,
                    block.ancestor_depth,
                    unresolved.stage.as_str(),
                    unresolved.reason.as_str(),
                )),
                _ => {}
            }
        }
        lines
    }

    /// The identity this document is known by, for a log or a report.
    pub fn document_identity(&self) -> String {
        match &self.source {
            DocumentSource::Record { key, .. } => format!("{key}"),
            DocumentSource::SavedFile(path) => path.display().to_string(),
        }
    }

    /// The runtime state an agent reads.
    ///
    /// `visible_block_count` starts at **zero** here and is filled in by the spawn
    /// system from [`CreatorOutcome::spawned`]: how much is on screen is a
    /// measured runtime fact, not something the projection can assert.
    pub fn runtime_state(&self, selected: Option<usize>) -> CreatorRuntimeState {
        let counts = self.projection.counts();
        CreatorRuntimeState {
            document: self.document_identity(),
            document_source: self.source.as_str().to_owned(),
            document_family: self
                .source
                .family()
                .map_or("unknown", Family::name)
                .to_owned(),
            document_format_version: self.projection.document_format_version,
            block_count: self.block_count() as u32,
            resolved_count: counts.resolved as u32,
            unresolved_count: counts.unresolved as u32,
            visible_block_count: 0,
            selected_block: selected.map(|block| block as u32),
            total_vertices: self.projection.total_vertices() as u32,
            total_triangles: self.projection.total_triangles() as u32,
            total_bones: self.projection.total_bones() as u32,
            max_depth: self.projection.max_depth() as u32,
            packages: self.packages as u32,
            scene_stable: false,
            frame: 0,
            camera_framed: false,
            framed_extent: None,
            camera_position: None,
            camera_distance_to_centre: None,
            viewport_available: false,
            viewport_physical_position: Some([0, 0]),
            viewport_physical_size: Some([0, 0]),
            viewport_scale_factor: 1.0,
            viewport_camera_position: None,
            viewport_camera_distance_to_centre: None,
            backdrop_loaded: false,
            backdrop_size: None,
            backdrop_position: None,
            bake_stage: String::new(),
            bake_tiles: 0,
            bake_reason: None,
            bake_pages: 1,
            skin: crate::editor::skin::SkinPalette::from_document(&self.document)
                .map(|palette| palette.describe())
                .unwrap_or_else(|| "no <skincolor*> in this document".to_owned()),
            blocks: self
                .projection
                .blocks()
                .iter()
                .map(|block| CreatorBlockState {
                    block: block.block as u32,
                    parent: block.parent.map(|parent| parent as u32),
                    ancestor_depth: block.ancestor_depth as u32,
                    block_id: block.block_id.map(|id| [id.group_id, id.instance_id]),
                    resolved: block.part.is_some(),
                    visible: block
                        .part
                        .as_ref()
                        .is_some_and(|part| part.geometry.is_some()),
                    container: block
                        .part
                        .as_ref()
                        .map(|part| key_array(part.container_key)),
                    geometry: block
                        .part
                        .as_ref()
                        .and_then(|part| part.geometry_key)
                        .map(key_array),
                    vertices: block
                        .part
                        .as_ref()
                        .and_then(|part| part.vertices)
                        .map(|count| count as u32),
                    triangles: block
                        .part
                        .as_ref()
                        .and_then(|part| part.triangles)
                        .map(|count| count as u32),
                    bones: block
                        .part
                        .as_ref()
                        .and_then(|part| part.bones)
                        .map(|count| count as u32),
                    named_bones: block
                        .part
                        .as_ref()
                        .map_or(0, |part| part.named_bones as u32),
                    weakest_grade: block
                        .part
                        .as_ref()
                        .map_or("none", |part| grade_name(part.weakest_grade))
                        .to_owned(),
                    stop_stage: block
                        .unresolved
                        .as_ref()
                        .map(|unresolved| unresolved.stage.as_str().to_owned()),
                    stop_reason: block
                        .unresolved
                        .as_ref()
                        .map(|unresolved| unresolved.reason.as_str().to_owned()),
                })
                .collect(),
        }
    }

    /// The camera framing this projection implies, or `None` when nothing resolved.
    ///
    /// `None` is the honest answer for a creature that failed to compose: framing
    /// a camera on nothing puts it in the origin, which reads as a composed
    /// creature of zero size rather than as a failure.
    pub fn camera_framing(&self) -> Option<CameraFraming> {
        self.projection.bounds().map(|bounds| CameraFraming {
            bounds: (bounds.min, bounds.max),
            vertical_fov_degrees: DEFAULT_VERTICAL_FOV_DEGREES,
        })
    }
}

/// The whole projection, as a resource: the spawn system's input.
///
/// Kept alive after spawning rather than dropped, and the cost is stated: a
/// whole-document projection holds the decoded geometry, so it duplicates what
/// `Assets<Mesh>` now holds. It is bounded -- 110 037 vertices across the entire
/// 68-document corpus -- and it is what makes the runtime state's numbers
/// re-derivable rather than a snapshot someone took once.
#[derive(Resource, Debug, Clone)]
pub struct StagedCreator {
    /// The projection.
    pub stage: CreatorStage,
    /// The block the caller selected, if any.
    pub selected: Option<usize>,
    /// A world-space uniform scale for the whole creature.
    pub scale: f32,
}

impl CreatorRuntimeState {
    /// Whether [`spore_morphology::Projection::bounds`] applies a part's
    /// orientation and scale to its geometry.
    ///
    /// **It does**, as of the commit that added
    /// [`ProjectedBlock::world_bounds`](spore_morphology::ProjectedBlock::world_bounds).
    ///
    /// This was `false`, and the reason it was `false` is the useful part: `bounds`
    /// added `world_position` to each of a part's *untransformed* geometry
    /// positions. That is correct for the corpus it was measured on, where every
    /// part is small, near-axis and unscaled, and wrong everywhere else. A rotated
    /// creature was framed against a box smaller than itself and a scaled one
    /// against a box the wrong size entirely.
    ///
    /// It stayed `false` for so long because nothing consumed a correct answer:
    /// the spawn path uses `local_scale` and lets Bevy's parent chain apply the
    /// rest, which is right, and the camera framing it fed was unremarkable on the
    /// near-axis corpus. The first real consumer was **picking** -- and a picker
    /// built on an offset-only box selects the wrong part, which is a worse failure
    /// than a slightly loose frame.
    ///
    /// Two things had to be true before this could flip, and the second is the
    /// reason it is a `const` at all:
    ///
    /// * `ProjectedBlock` gained a `world_scale`. It did not have one -- the value
    ///   was computed in `descend`, carried down to every child, and then dropped
    ///   at the struct literal, with its doc comment migrated onto the neighbouring
    ///   `parent` field. A bounds implementation cannot honour a field that is not
    ///   stored, so this was the actual blocker.
    /// * The transform is applied by mapping the box's eight corners, which is
    ///   **exact** for an affine map rather than approximate: an affine map sends a
    ///   convex hull to the hull of the mapped points.
    ///
    /// Keeping it a `const` rather than restoring a per-frame `bool` is deliberate.
    /// The previous field was assigned once and read nowhere, so it said `false`
    /// forever and taught readers something untrue. A `const` cannot go stale, and
    /// changing it makes the compiler name every reader that assumed the old answer.
    pub const BOUNDS_APPLY_ORIENTATION: bool = true;
}

/// Everything an agent can ask about a staged creator scene.
///
/// Reflected so BRP can read it. It holds OpenSpore-owned plain data only --
/// never a `Transform`, a `Visibility` or a `Handle` -- because in Bevy 0.19
/// `bevy_transform`, `bevy_camera` and `bevy_mesh` contain **zero**
/// `register_type` calls, so a query for one of those fails with
/// `isn't registered or used in the world` and, under `strict: false`, fails
/// *silently as zero rows*. See `AGENTS.md` and `docs/BRP.md`.
#[derive(Resource, Reflect, Debug, Clone, Default)]
#[reflect(Resource)]
pub struct CreatorRuntimeState {
    /// The document's identity, or the file it was read from.
    pub document: String,
    /// `"record"` or `"saved-file"`.
    pub document_source: String,
    /// The authoring family, or `"unknown"` when the file carried no type word.
    pub document_family: String,
    /// `<formatversion>`, or `None` when the document carried none.
    pub document_format_version: Option<u32>,
    /// How many blocks the document places.
    pub block_count: u32,
    /// How many resolved to a part.
    pub resolved_count: u32,
    /// How many did not, each with a stage and a reason in [`Self::blocks`].
    pub unresolved_count: u32,
    /// How many blocks are actually drawable, measured after the spawn.
    pub visible_block_count: u32,
    /// The block the caller selected, or `None`.
    pub selected_block: Option<u32>,
    /// Vertices across every resolved block that decoded.
    pub total_vertices: u32,
    /// Triangles across every resolved block that decoded.
    pub total_triangles: u32,
    /// Bones across every resolved block that has a body graph.
    pub total_bones: u32,
    /// The deepest authored chain.
    pub max_depth: u32,
    /// How many packages were opened.
    pub packages: u32,
    /// Whether the scene has finished staging and can be trusted.
    ///
    /// The readiness signal: automation polls this instead of sleeping. It is
    /// `false` until the spawn system has run, and `Commands::spawn` is deferred,
    /// so it is also `false` on any frame where the entities do not exist yet.
    pub scene_stable: bool,
    /// Frames since the app started updating. `0` before the first `Update`.
    pub frame: u64,
    /// Whether a [`CameraFraming`] resource existed, i.e. whether the projection
    /// produced bounds at all.
    ///
    /// Read from the world rather than assumed from the projection, so it reports
    /// what the camera system actually received. `false` here with
    /// [`Self::resolved_count`] greater than zero is the one combination that means
    /// "the creature composed but was never framed".
    pub camera_framed: bool,
    /// The extent the framing resource asked to fit, as `(x, y, z)`.
    ///
    /// `None` when there was no framing resource. This is the box the creature's
    /// geometry was measured into, which is not necessarily the box that is drawn
    /// -- see [`CreatorRuntimeState::BOUNDS_APPLY_ORIENTATION`].
    pub framed_extent: Option<[f32; 3]>,
    /// The camera's measured world position, read back out of the scene.
    ///
    /// Present so a framing question can be answered without trusting a
    /// screenshot. See the note on [`Self::camera_framed`]: Bevy 0.19 registers no
    /// reflection for `Transform`, `GlobalTransform`, `Visibility` or `Name`, so a
    /// BRP query for the camera's own transform returns nothing and a `null` there
    /// means *not reflected*, never *not present*. Reading the transform here and
    /// republishing it in an OpenSpore-owned reflected type is what makes the
    /// difference between those two answerable. Cost and intent are stated in
    /// `tick_creator_runtime`.
    pub camera_position: Option<[f32; 3]>,
    /// How far the camera sits from the centre of the framed box.
    ///
    /// The single number that says whether framing happened: `ScenePlugin` parks
    /// the camera at `(0, 6, 34)` looking at the origin, so an unframed creature
    /// reads as a very large distance and a framed one as a small one, derived
    /// from the creature's own measured extent rather than from any constant.
    pub camera_distance_to_centre: Option<f32>,
    /// The document's own skin, as a `1 / 3` description, or why there is none.
    ///
    /// A **string**, not a colour, because the interesting half is not the base
    /// colour: it is that the pattern ids are reported and explicitly marked
    /// `NOT_APPLIED`, since a 32-bit id with no registry cannot be turned into
    /// stripes honestly.
    pub skin: String,
    /// One row per authored block, in document order.
    ///
    /// **Including the unresolved ones.** An agent can therefore enumerate a
    /// failure it did not see on screen, which is the entire point: the entity
    /// does not exist, but the reason does.
    pub blocks: Vec<CreatorBlockState>,
    /// Whether a laid-out UI node has been measured for the 3D viewport yet.
    ///
    /// `false` during the first frames, which is normal and is not an error. It is
    /// emphatically not "the viewport is the whole window": that substitution is
    /// the defect this exists to make visible, and a reader that assumed it would
    /// conclude the creature is centred in a region it is not.
    pub viewport_available: bool,
    /// The 3D viewport rectangle's physical top-left, as the camera was given it.
    pub viewport_physical_position: Option<[u32; 2]>,
    /// The 3D viewport rectangle's physical size, as the camera was given it.
    pub viewport_physical_size: Option<[u32; 2]>,
    /// The logical-to-physical factor that rectangle was measured under.
    pub viewport_scale_factor: f32,
    /// The creature camera's world position, read back out of the scene.
    ///
    /// Separate from [`Self::camera_position`] because in the editor the two
    /// describe different cameras: `camera_position` is the full-window UI camera,
    /// which is deliberately never moved, and this is the one the creature is
    /// rendered through and framed on. Overwriting the shared field would have
    /// quietly redefined it for the viewer too.
    pub viewport_camera_position: Option<[f32; 3]>,
    /// How far the creature camera sits from the centre of the framed box.
    ///
    /// The same quantity as [`Self::camera_distance_to_centre`], over the same
    /// centre, so the two are comparable numbers rather than two definitions of
    /// "framed".
    pub viewport_camera_distance_to_centre: Option<f32>,
    /// Whether the painted backdrop's image was read and installed.
    pub backdrop_loaded: bool,
    /// The backdrop node's laid-out size in physical px, or `None` before layout.
    ///
    /// Published because "the image loaded" and "the image is drawn" are separate
    /// questions, and a zero-sized node with a perfectly valid image handle is the
    /// state that looks exactly like a missing asset in a screenshot.
    pub backdrop_size: Option<[f32; 2]>,
    /// The backdrop node's laid-out top-left in physical px.
    pub backdrop_position: Option<[f32; 2]>,
    /// Where the part-thumbnail bake got to, as a debug-formatted stage name.
    pub bake_stage: String,
    /// How many atlas tiles actually carry an image.
    pub bake_tiles: u32,
    /// Why the bake failed, or `None` while it is fine.
    pub bake_reason: Option<String>,
    /// How many pages of parts the catalogue holds.
    pub bake_pages: u32,
}

/// One authored block's row in [`CreatorRuntimeState::blocks`].
#[derive(Reflect, Debug, Clone, PartialEq, Eq)]
pub struct CreatorBlockState {
    /// The block's index in its document.
    pub block: u32,
    /// The authored parent, or `None` for a root.
    pub parent: Option<u32>,
    /// How many ancestors contributed to the world transform.
    pub ancestor_depth: u32,
    /// `<blockid>` as `(group, instance)`, or `None` when the document wrote none.
    pub block_id: Option<[u32; 2]>,
    /// Whether the block resolved to a part.
    pub resolved: bool,
    /// Whether that part's geometry decoded, and so whether an entity exists.
    pub visible: bool,
    /// The `rw4` container key as `(type, group, instance)`.
    pub container: Option<[u32; 3]>,
    /// The `gmsh` geometry key as `(type, group, instance)`.
    pub geometry: Option<[u32; 3]>,
    /// Vertices, or `None` when the record would not decode.
    pub vertices: Option<u32>,
    /// Triangles, or `None` when the record would not decode.
    pub triangles: Option<u32>,
    /// Bones, or `None` when the container holds no skeleton.
    ///
    /// `None` is a real corpus state -- 256 of 1824 containers hold no skeleton
    /// -- and is not the same as zero bones.
    pub bones: Option<u32>,
    /// How many bones recovered a name.
    pub named_bones: u32,
    /// The weakest grade on the chain that produced this part.
    pub weakest_grade: String,
    /// Where an unresolved block stopped.
    pub stop_stage: Option<String>,
    /// Why it stopped.
    pub stop_reason: Option<String>,
}

/// Marks an entity as one projected block, and names its parent in block space.
///
/// A separate component from [`OnStage`] so a query can ask "which blocks are on
/// stage" without matching the camera, the ground and the lights too. Reflected,
/// so a `world.query` with component data returns rows rather than nothing.
#[derive(Component, Reflect, Debug, Clone, Copy, PartialEq, Eq)]
pub struct CreatorBlockMarker {
    /// The block's index in its document.
    pub block: u32,
    /// The authored parent block, or `None` for a root.
    pub parent: Option<u32>,
    /// How many ancestors contributed to the world transform.
    pub ancestor_depth: u32,
    /// Whether this block is the selected one.
    pub selected: bool,
}

/// Marks the one entity that owns the whole creature's transform.
#[derive(Component, Reflect, Debug, Clone, Copy, Default)]
pub struct CreatorRootMarker;

/// What one spawn produced.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub struct CreatorOutcome {
    /// Blocks in the document.
    pub blocks: usize,
    /// Blocks that resolved to a part.
    pub resolved: usize,
    /// Blocks that did not.
    pub unresolved: usize,
    /// Resolved blocks whose `gmsh` would not decode, so no entity was spawned.
    pub without_geometry: usize,
    /// Entities actually spawned. The number an agent should trust for "what is on
    /// screen", and why it is a separate field from [`Self::resolved`].
    pub spawned: usize,
    /// Vertices in the spawned meshes.
    pub vertices: usize,
    /// Triangles in the spawned meshes.
    pub triangles: usize,
    /// Bones across the spawned parts.
    pub bones: usize,
}

impl CreatorOutcome {
    /// A one-line account, for the startup log.
    pub fn describe(&self) -> String {
        format!(
            "{} block(s): {} resolved, {} unresolved, {} without geometry, {} entity(ies), \
             {} triangle(s), {} vertex(ies), {} bone(s)",
            self.blocks,
            self.resolved,
            self.unresolved,
            self.without_geometry,
            self.spawned,
            self.triangles,
            self.vertices,
            self.bones,
        )
    }
}

/// The result of decoding a creator request, before any rendering.
#[derive(Debug, Clone)]
pub enum CreatorDecoded {
    /// It decoded, and can be staged.
    Ready(Box<CreatorStage>),
    /// It did not, with a reason a person can act on.
    Failed(String),
}

impl CreatorDecoded {
    /// The report lines to print, or the failure line.
    pub fn report_lines(&self) -> Vec<String> {
        match self {
            Self::Ready(stage) => stage.report_lines(),
            Self::Failed(reason) => vec![format!("OPENSPORE-CREATOR v1 failed: {reason}")],
        }
    }
}

/// Why a creator request could not be staged at all.
#[derive(Debug)]
pub enum CreatorLoadError {
    /// The packages would not open.
    Packages(String),
    /// No package was named, so nothing could be resolved.
    NoPackages,
    /// `--creator-replace` named a block the document does not have.
    ///
    /// Its own variant because the two numbers are the whole diagnostic: the index
    /// asked for and how many blocks exist. Clamping to the last block would edit a
    /// block nobody named.
    NoSuchBlock {
        /// The block index the caller asked for.
        index: usize,
        /// How many blocks the document has.
        len: usize,
    },
    /// The record is not in any package searched.
    RecordNotFound {
        /// The identity asked for.
        key: ResourceKey,
        /// How many packages were searched.
        searched: usize,
    },
    /// The record's bytes are not UTF-8, so it is not the XML document the
    /// format says it is.
    NotText {
        /// The identity asked for.
        key: ResourceKey,
    },
    /// The record's type id is not one of the six authoring families, so the
    /// grammar to apply is unknown.
    UnknownFamily {
        /// The type id that was not recognised.
        type_id: u32,
        /// The record's identity.
        key: ResourceKey,
    },
    /// The document did not parse.
    Document(String),
    /// The document has no blocks to resolve.
    NoBlocks,
    /// The document and its composition could not be projected together.
    Project(String),
    /// A `--creator-save` target could not be written or read.
    Save {
        /// The path.
        path: PathBuf,
        /// Why.
        reason: String,
    },
}

impl core::fmt::Display for CreatorLoadError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Packages(reason) => write!(f, "{reason}"),
            Self::NoSuchBlock { index, len } => write!(
                f,
                "--creator-replace named block {index}, but the document has {len} blocks \
                 (indices 0..{len}). Nothing was edited: clamping to the last block would \
                 change a block you did not name"
            ),
            Self::NoPackages => write!(
                f,
                "no Spore package was named, so there is nothing to resolve the creator \
                 document against. Pass --package <PATH> pointing at the .package files that \
                 hold the document and its parts, e.g. SPORE/Data/Spore_Game.package and \
                 SPORE/Data/Spore_Graphics.package."
            ),
            Self::RecordNotFound { key, searched } => write!(
                f,
                "creator record {key} is not in any of the {searched} package(s) searched; \
                 creator documents live in Spore_Game.package"
            ),
            Self::NotText { key } => write!(
                f,
                "creator record {key} is not UTF-8, so it is not the XML document the \
                 format says it is"
            ),
            Self::UnknownFamily { type_id, key } => write!(
                f,
                "creator record {key} has type id 0x{type_id:08x}, which is not one of the six \
                 authoring families this build knows; a document's grammar is family-specific"
            ),
            Self::Document(reason) => {
                write!(f, "the creator document did not parse: {reason}")
            }
            Self::NoBlocks => {
                write!(
                    f,
                    "the creator document places no blocks, so there is nothing to project"
                )
            }
            Self::Project(reason) => {
                write!(f, "the document and its composition disagree: {reason}")
            }
            Self::Save { path, reason } => write!(f, "could not write {path:?}: {reason}"),
        }
    }
}

impl std::error::Error for CreatorLoadError {}

/// Loads, resolves and projects a creator document. No window, no GPU, no ECS.
///
/// This is the headless half: `--info` prints [`CreatorDecoded::report_lines`] and
/// exits, and a test can call it with no game install at all because the record
/// source is the same [`ContentStore`] the rest of the engine already opens.
pub fn load_creator(
    source: &DocumentSource,
    packages: &[PathBuf],
) -> Result<CreatorStage, CreatorLoadError> {
    load_creator_replacing(source, packages, None)
}

/// [`load_creator`], with one block's part swapped before anything is resolved.
///
/// # Why the replacement happens HERE and not after projection
///
/// The point of an edit is that the new part is resolved by the **same chain** an
/// authored part would be: `blockref -> rigblock -> prop -> part pair`. Applying it
/// after projection would leave every number in the runtime state describing the part
/// that was replaced, and a replacement that does not resolve would look like a
/// missing entity rather than like an unresolved block with a stop stage.
///
/// It is here rather than on [`CreatorStage`] because a stage deliberately does not
/// keep the record source: a staged creature holds its decoded geometry and the
/// document, not the packages it came from, so it cannot re-resolve anything. An
/// `Arc<Source>` would be a live handle on a memory map for the lifetime of every
/// stage, which is the wrong price for a second call path.
///
/// An index past the end is refused. Clamping to the last block would edit a block
/// the caller did not name, and that looks exactly like success.
pub fn load_creator_replacing(
    source: &DocumentSource,
    packages: &[PathBuf],
    replace: Option<(usize, spore_creator_core::document::BlockId)>,
) -> Result<CreatorStage, CreatorLoadError> {
    if packages.is_empty() {
        return Err(CreatorLoadError::NoPackages);
    }
    let world = ResourceWorld::open(packages)
        .map_err(|error| CreatorLoadError::Packages(crate::loader::describe(&error)))?;
    let mut document = read_document(source, world.store())?;
    if let Some((index, block_id)) = replace {
        let len = document.blocks.len();
        if index >= len {
            return Err(CreatorLoadError::NoSuchBlock { index, len });
        }
        document
            .replace_block(index, block_id)
            .map_err(|error| CreatorLoadError::Document(error.to_string()))?;
    }
    let records = StoreSource::new(world.store());
    let stage = project_document(document, source, &records, world.package_names().len())?;
    Ok(CreatorStage {
        read_errors: records.read_errors.get(),
        ..stage
    })
}

/// Projects a document against real packages, for either document source.
///
/// [`load_creator`] is the record path: it locates the packages, reads the document
/// and projects it. There was no equivalent for the **saved-file** path, because
/// [`read_document`] and the projection were private, so a round trip -- write a
/// document, read it back, project it against the same packages -- could only be
/// done through the engine's own binary and a screenshot. That is the whole
/// replace/save/reload loop, so it needed a public seam.
///
/// Takes the document rather than reading it, so the caller decides which of the two
/// documents it is projecting: the one it just edited, or the one it read back from
/// disk. Comparing those two projections IS the round-trip assertion.
pub fn project_against_packages(
    document: CreatorDocument,
    source: &DocumentSource,
    packages: &[PathBuf],
) -> Result<CreatorStage, CreatorLoadError> {
    let world = ResourceWorld::open(packages)
        .map_err(|error| CreatorLoadError::Packages(crate::loader::describe(&error)))?;
    project_document(
        document,
        source,
        &StoreSource::new(world.store()),
        world.package_names().len(),
    )
}

/// Reads the document itself: a `<sporemodel>` record out of a package, or a
/// `spore-creator-doc/1` file off disk.
///
/// Public because [`CreatorSession`](crate::editor::session::CreatorSession) needs
/// exactly this and nothing more: it holds a live [`ContentStore`] so it can
/// re-resolve an edited document, and reusing the engine's own read path is what
/// makes "the editor opened the same document the loader would have" a fact
/// rather than a resemblance.
///
/// The [`DocumentSource::SavedFile`] arm never touches the store, so
/// [`read_saved_document`] serves a caller that has a file and no packages
/// without having to hand it an empty [`ContentStore`] to satisfy the signature.
pub fn read_document(
    source: &DocumentSource,
    store: &ContentStore,
) -> Result<CreatorDocument, CreatorLoadError> {
    match source {
        DocumentSource::Record { key, family } => {
            let bytes = match store.read(key) {
                Ok(bytes) => bytes,
                Err(AssetError::NotFound { .. }) => {
                    return Err(CreatorLoadError::RecordNotFound {
                        key: *key,
                        searched: store.len(),
                    });
                }
                Err(error) => return Err(CreatorLoadError::Packages(error.to_string())),
            };
            let text =
                std::str::from_utf8(&bytes).map_err(|_| CreatorLoadError::NotText { key: *key })?;
            parse_sporemodel(*family, text)
                .map_err(|error| CreatorLoadError::Document(error.to_string()))
        }
        DocumentSource::SavedFile(path) => {
            let text = std::fs::read_to_string(path).map_err(|error| CreatorLoadError::Save {
                path: path.clone(),
                reason: error.to_string(),
            })?;
            persist::load(&text).map_err(|error| CreatorLoadError::Save {
                path: path.clone(),
                reason: error.to_string(),
            })
        }
    }
}

/// Reads a saved document with no [`ContentStore`] in hand at all.
///
/// Separate from [`read_document`] because the two arms of that function need
/// different things: a `Record` source needs the packages, a `SavedFile` source
/// needs a filesystem and nothing else. Giving this one the general signature
/// would force a caller that only has a file to construct an empty store, and
/// "this session has no packages" is a different claim from "this session needs
/// no packages to read this".
pub fn read_saved_document(path: &Path) -> Result<CreatorDocument, CreatorLoadError> {
    read_document(
        &DocumentSource::SavedFile(path.to_path_buf()),
        &ContentStore::new(),
    )
}

/// Resolves a document against a record source and projects it.
///
/// Split from [`load_creator`] so a test can drive the whole chain from synthetic
/// records, with no package on disk.
pub fn project_document(
    document: CreatorDocument,
    source: &DocumentSource,
    records: &dyn RecordSource,
    packages: usize,
) -> Result<CreatorStage, CreatorLoadError> {
    if document.blocks.is_empty() {
        return Err(CreatorLoadError::NoBlocks);
    }
    let counting = CountingSource::new(records);
    let composition = spore_morphology::compose(&document, &counting)
        .map_err(|error| CreatorLoadError::Document(error.to_string()))?;
    let projection = project(&document, &composition)
        .map_err(|error| CreatorLoadError::Project(error.to_string()))?;
    let (lookups, misses) = counting.counters();
    Ok(CreatorStage {
        document,
        projection,
        source: source.clone(),
        packages,
        lookups,
        misses,
        read_errors: 0,
    })
}

/// A [`RecordSource`] over any other one, counting what it did.
///
/// The counters are the point: a block that failed with "no such record" reads
/// very differently when the source also had extraction failures, and a resolver
/// that cannot tell those apart looks like a resolver bug.
struct CountingSource<'a> {
    inner: &'a dyn RecordSource,
    lookups: Cell<usize>,
    misses: Cell<usize>,
}

impl<'a> CountingSource<'a> {
    fn new(inner: &'a dyn RecordSource) -> Self {
        Self {
            inner,
            lookups: Cell::new(0),
            misses: Cell::new(0),
        }
    }

    fn counters(&self) -> (usize, usize) {
        (self.lookups.get(), self.misses.get())
    }
}

impl RecordSource for CountingSource<'_> {
    fn record(&self, key: &ResourceKey) -> Option<Vec<u8>> {
        self.lookups.set(self.lookups.get() + 1);
        let found = self.inner.record(key);
        if found.is_none() {
            self.misses.set(self.misses.get() + 1);
        }
        found
    }
}

/// A [`RecordSource`] over an open [`ContentStore`].
///
/// One mapping, shared with the rest of the engine. This is why the creator path
/// needs no second copy of `Spore_Game.package` in memory: `spore-morphology`'s
/// own [`spore_morphology::PackageSource`] reads whole files with `fs::read`, and
/// the engine already has the same bytes mapped for its other content.
///
/// Public because it is the honest way to project a document against packages a
/// caller has **already** opened -- a corpus test over 68 documents, or a future
/// streaming subsystem. It is the same adapter [`load_creator`] uses, so a caller
/// that uses it directly is not on a second path.
pub struct StoreSource<'a> {
    store: &'a ContentStore,
    read_errors: Cell<usize>,
}

impl<'a> StoreSource<'a> {
    /// Wraps an open store. The store's lifetime is the mapping's lifetime, so
    /// nothing here keeps a package alive by accident.
    pub fn new(store: &'a ContentStore) -> Self {
        Self {
            store,
            read_errors: Cell::new(0),
        }
    }
}

impl core::fmt::Debug for StoreSource<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("StoreSource")
            .field("packages", &self.store.len())
            .field("read_errors", &self.read_errors.get())
            .finish()
    }
}

impl RecordSource for StoreSource<'_> {
    fn record(&self, key: &ResourceKey) -> Option<Vec<u8>> {
        match self.store.read(key) {
            Ok(bytes) => Some(bytes),
            // `NotFound` is a miss. Anything else is a record the index holds and
            // the extractor could not hand back, which is counted separately so a
            // caller is never told "no such record" about a record that is there.
            Err(AssetError::NotFound { .. }) => None,
            Err(_) => {
                self.read_errors.set(self.read_errors.get() + 1);
                None
            }
        }
    }
}

/// Writes a document as `spore-creator-doc/1`.
///
/// The save path exists so "project, save, reload, project again, compare" can be
/// run against real data rather than asserted in prose.
pub fn save_document(path: &Path, document: &CreatorDocument) -> Result<(), CreatorLoadError> {
    let text = persist::save(document).map_err(|error| CreatorLoadError::Save {
        path: path.to_path_buf(),
        reason: error.to_string(),
    })?;
    std::fs::write(path, text).map_err(|error| CreatorLoadError::Save {
        path: path.to_path_buf(),
        reason: error.to_string(),
    })
}

// ---------------------------------------------------------------------------
// Spawning
// ---------------------------------------------------------------------------

/// The buffers a projected part's geometry lifts into.
///
/// # One mesh conversion, not two
///
/// `spore_gmdl` meshes come in through [`convert::to_buffers`]; a part's `gmsh`
/// geometry is a different record with a different shape, so it is lifted here
/// into the same engine-neutral [`MeshBuffers`] and handed to the same
/// [`convert::to_bevy_mesh`]. There is exactly one normal policy, one tangent
/// policy and one index-buffer policy in this crate, and a second conversion
/// would be a second answer to each.
///
/// `uvs` and `raw_normal_bytes` are `None` because the `gmsh` record has no
/// TEXCOORD element and no NORMAL element -- not because they were skipped. The
/// consequence is stated rather than hidden: these meshes cannot use a normal
/// map, and [`convert::to_bevy_mesh`] logs exactly that rather than inventing
/// tangents.
fn part_buffers(part: &spore_morphology::ProjectedPart) -> Option<MeshBuffers> {
    let geometry = part.geometry.as_ref()?;
    Some(MeshBuffers {
        positions: geometry.positions.clone(),
        uvs: None,
        indices: geometry
            .indices
            .iter()
            .map(|index| u32::from(*index))
            .collect(),
        topology: Topology::TriangleList,
        raw_normal_bytes: None,
    })
}

/// Spawns one entity per visible block, parented as the document nests.
///
/// # Three passes, on purpose
///
/// 1. the root entity, carrying the world scale;
/// 2. one entity per visible block, in **document order** -- parentage is wired
///    afterwards, because a document may list a child before its parent and
///    "spawn the parent first" is an assumption about authoring, not a property
///    of the format;
/// 3. `add_child` for every block with an entity, parented to its authored
///    parent's entity.
///
/// A block whose parent did not resolve is parented to the root rather than
/// dropped: its geometry is real, and re-parenting to the root is the only
/// placement that is not a claim. Its own `world_position` then does not apply,
/// which is exactly why the report line names every such block.
///
/// Returns the outcome *and* the entity per block index, so a test can address an
/// entity without a query and cannot confuse "no entity" with "not asked".
pub fn spawn_projection(
    stage: &CreatorStage,
    selected: Option<usize>,
    scale: f32,
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) -> (CreatorOutcome, Vec<Option<Entity>>) {
    spawn_projection_skin(stage, selected, scale, None, commands, meshes, materials)
}

/// [`spawn_projection`], with the document's own skin colour applied.
///
/// # Why the colour comes from the DOCUMENT and not from a family lookup
///
/// `domain::paint::resolve` answers "what colour should a *model* be" by finding
/// a `crt` record whose family matches the model's group. For a creature part that
/// is the wrong question twice: a part's group (`0x40626000`) has no canonical
/// name in the committed table so the lookup returns `ModelGroupUnnamed`, and
/// where it does resolve it takes an *arbitrary* first match -- which is how every
/// unnamed creature came to wear one shipped preset's green.
///
/// The document's colour is **in the document**. `<skincolor1..3>` is present in
/// 47 of 47 creature records, `CreatorDocument::properties` has been parsing it
/// all along, and reading it from the document being edited is both simpler and
/// strictly more correct.
///
/// Split as its own function so the VIEWER's behaviour is unchanged and this stays
/// opt-in: a caller that does not pass a palette gets exactly the neutral grey it
/// got before, which is what makes the difference attributable.
pub fn spawn_projection_skin(
    stage: &CreatorStage,
    selected: Option<usize>,
    scale: f32,
    palette: Option<crate::editor::skin::SkinPalette>,
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) -> (CreatorOutcome, Vec<Option<Entity>>) {
    let counts = stage.projection.counts();
    let mut outcome = CreatorOutcome {
        blocks: stage.block_count(),
        resolved: counts.resolved,
        unresolved: counts.unresolved,
        without_geometry: 0,
        spawned: 0,
        vertices: 0,
        triangles: 0,
        bones: 0,
    };

    let root = commands
        .spawn((
            Name::new(ROOT_NAME),
            Transform::from_scale(Vec3::splat(scale)),
            OnStage,
            CreatorRootMarker,
        ))
        .id();

    let mut entities: Vec<Option<Entity>> = vec![None; stage.block_count()];
    // **Two** materials for the whole creature, not one per block. The tint is a
    // function of "is this the selection" and nothing else, so N parts means two
    // `StandardMaterial` assets rather than N, and a 47-part creature does not
    // allocate 47 identical shaders' worth of state.
    let base = ASSET_TINT.to_linear();
    let selection = SELECTION_TINT.to_linear();
    // The part material comes from the document's own skin when there is one, and
    // falls back to the neutral grey otherwise. The fallback is a FALLBACK and
    // says so in the report line -- "the document states no colour" is not the
    // same claim as "the document is grey".
    let plain_material = match palette
        .as_ref()
        .and_then(crate::editor::skin::SkinPalette::base)
    {
        Some(colour) => {
            let linear = crate::editor::skin::linear_rgb(colour);
            materials.add(texture::standard_material(
                &texture::SurfaceSet::default(),
                None,
                Some(linear),
            ))
        }
        None => materials.add(texture::standard_material(
            &texture::SurfaceSet::default(),
            None,
            Some([base.red, base.green, base.blue]),
        )),
    };
    let selected_material = materials.add(texture::standard_material(
        &texture::SurfaceSet::default(),
        None,
        Some([selection.red, selection.green, selection.blue]),
    ));

    for block in stage.projection.blocks() {
        let Some(part) = block.part.as_ref() else {
            continue;
        };
        let Some(buffers) = part_buffers(part) else {
            outcome.without_geometry += 1;
            continue;
        };
        let is_selected = selected == Some(block.block);
        let material = if is_selected {
            &selected_material
        } else {
            &plain_material
        };
        let local = block.local_position;
        // The projection's LOCAL transform, because the entity is parented. See
        // the module docs: the composed world transform is the projection's
        // `world_*`, and applying both would apply the parent chain twice.
        let transform = Transform::from_translation(Vec3::new(local.x, local.y, local.z))
            .with_rotation(orientation_to_quat(block.local_orientation))
            .with_scale(Vec3::splat(block.local_scale));
        let entity = commands
            .spawn((
                Name::new(block_name(block.block)),
                Mesh3d(meshes.add(convert::to_bevy_mesh(&buffers, NORMAL_MODE))),
                MeshMaterial3d(material.clone()),
                transform,
                OnStage,
                CreatorBlockMarker {
                    block: block.block as u32,
                    parent: block.parent.map(|parent| parent as u32),
                    ancestor_depth: block.ancestor_depth as u32,
                    selected: is_selected,
                },
            ))
            .id();
        entities[block.block] = Some(entity);
        outcome.spawned += 1;
        outcome.vertices += part.vertices.unwrap_or(0);
        outcome.triangles += part.triangles.unwrap_or(0);
        outcome.bones += part.bones.unwrap_or(0);
    }

    // Pass 3. `add_child` rather than setting `ChildOf` on one side: Bevy's
    // hierarchy invariant is that the parent carries `Children` too, and its
    // warning says plainly that a one-sided relationship behaves inconsistently.
    for block in stage.projection.blocks() {
        let Some(entity) = entities[block.block] else {
            continue;
        };
        let parent = block
            .parent
            .and_then(|index| entities.get(index).copied().flatten())
            .unwrap_or(root);
        commands.entity(parent).add_child(entity);
    }

    (outcome, entities)
}

// ---------------------------------------------------------------------------
// The plugin
// ---------------------------------------------------------------------------

/// Turns a [`StagedCreator`] into entities, and keeps the runtime state honest.
///
/// A third plugin rather than another responsibility inside
/// [`crate::loader::SporeAssetPlugin`]: that one stages *records* from a
/// [`crate::loader::LoadedScene`], this one stages a *document* from a
/// [`spore_morphology::Projection`], and the two have different inputs, different
/// ownership and different completeness reporting. Neither spawns a camera.
///
/// # Resources it requires
///
/// [`StagedCreator`] and [`CreatorRuntimeState`] must both be inserted before the
/// app is run -- [`crate::build_creator_app`] inserts them, in that order, which
/// is what lets the very first `Update` report a complete state rather than a
/// missing resource. [`CameraFraming`] is **optional**: absent, the camera keeps
/// the world's default position, which is what a projection that resolved nothing
/// should get.
#[derive(Debug, Clone, Copy, Default)]
pub struct CreatorPlugin;

impl Plugin for CreatorPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_staged_creator);
        app.add_systems(PostStartup, frame_creator_camera);
        app.add_systems(Update, tick_creator_runtime);
    }
}

fn spawn_staged_creator(
    mut commands: Commands,
    staged: Res<StagedCreator>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut state: ResMut<CreatorRuntimeState>,
) {
    let (outcome, _) = spawn_projection(
        &staged.stage,
        staged.selected,
        staged.scale,
        &mut commands,
        &mut meshes,
        &mut materials,
    );
    state.visible_block_count = outcome.spawned as u32;
    info!("openspore: staged creator -- {}", outcome.describe());
    for line in staged.stage.report_lines().iter().skip(1) {
        warn!("openspore: {line}");
    }
    commands.insert_resource(outcome);
}

/// Frames the existing camera on the projection's extent.
///
/// In `PostStartup` because [`crate::scene::ScenePlugin`] spawns the camera in
/// `Startup` and `Commands::spawn` is deferred, so a `Query` in the same system
/// that spawns would see zero cameras and leave the camera at its default. The
/// framing *mathematics* is [`crate::loader::frame_camera`]'s, not a second one.
fn frame_creator_camera(
    mut cameras: Query<&mut Transform, With<MainCamera>>,
    framing: Option<Res<CameraFraming>>,
) {
    let Some(framing) = framing else {
        return;
    };
    match cameras.single_mut() {
        Ok(mut transform) => crate::loader::frame_camera(
            &mut transform,
            framing.bounds,
            framing.vertical_fov_degrees,
        ),
        Err(error) => warn!("could not frame the camera on the creator: {error}"),
    }
}

/// Advances the readiness signal and the frame counter.
///
/// `scene_stable` becomes `true` on the first `Update`, which is the first moment
/// the spawn system's deferred commands have been applied. Automation polls
/// `scene_stable` and `frame` instead of sleeping a guessed interval: "the app
/// launched" and "the app has something on screen" are different claims, and only
/// one of them is true on frame 0.
///
/// It also republishes the camera's measured state. That half exists because of a
/// misdiagnosis worth keeping. The framing of a real creature was reported broken,
/// on the strength of a BRP read that came back `null` for the camera transform.
/// In Bevy 0.19 `bevy_transform`, `bevy_camera` and `bevy_mesh` contain **zero**
/// `register_type` calls, so `Transform` is invisible to reflection: that `null`
/// said *not registered*, and was read as *not present*. A headless probe then
/// measured the camera at `(0.0, 0.5258, 1.8056)` -- exactly what the framing
/// closed form predicts for that creature's measured extent. The framing had worked
/// the whole time, and the screenshot that prompted the search was of a six-times
/// repeated probe part.
///
/// So this reads the camera out of the world and republishes it in an
/// OpenSpore-owned reflected type. The cost is one `Transform` copy per frame; the
/// benefit is that "did the camera move" becomes a question with an answer, and
/// asking it no longer requires believing a screenshot.
/// Advances the editor-side runtime state every frame.
///
/// Public because the **editor** needs it and does not add `CreatorPlugin`:
/// without this the editor's `CreatorRuntimeState` carried the block table and
/// nothing else, so `camera_framed` and `camera_distance_to_centre` were `null`
/// in exactly the mode where a framing question is most likely -- after an edit.
/// A BRP reader would then have had `camera_framed: null` and no way to tell
/// "not framed" from "this mode does not report framing".
pub fn tick_creator_runtime(
    mut state: ResMut<CreatorRuntimeState>,
    cameras: Query<&Transform, With<MainCamera>>,
    framing: Option<Res<CameraFraming>>,
) {
    state.frame = state.frame.saturating_add(1);
    state.scene_stable = true;

    // Read the same resource `frame_creator_camera` read, so this reports what the
    // camera system actually received rather than what the projection intended.
    state.camera_framed = framing.is_some();
    state.framed_extent = framing.as_ref().map(|framing| {
        let (lo, hi) = framing.bounds;
        [hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]]
    });

    let Some(camera) = cameras.iter().next() else {
        // No camera at all. `None` is the honest reading; leaving the previous
        // frame's position in place would make a vanished camera look stationary.
        state.camera_position = None;
        state.camera_distance_to_centre = None;
        return;
    };
    let translation = camera.translation;
    state.camera_position = Some([translation.x, translation.y, translation.z]);
    let centre = framing.as_ref().map(|framing| {
        let (lo, hi) = framing.bounds;
        Vec3::new(
            (lo[0] + hi[0]) * 0.5,
            (lo[1] + hi[1]) * 0.5,
            (lo[2] + hi[2]) * 0.5,
        )
    });
    state.camera_distance_to_centre = centre.map(|centre| (translation - centre).length());
}

/// The evidence vocabulary's own spelling for a grade.
///
/// `Grade` has no `as_str` -- it carries a [`spore_core::EvidenceLevel`] rather
/// than a report string -- and inventing one here would be a second vocabulary
/// beside `spore-morphology`'s. A closed two-arm match keeps the report and the
/// evidence level one claim.
fn grade_name(grade: spore_morphology::Grade) -> &'static str {
    match grade {
        spore_morphology::Grade::Measured => "measured",
        spore_morphology::Grade::Inferred => "inferred",
    }
}

fn key_array(key: ResourceKey) -> [u32; 3] {
    [key.type_id, key.group_id, key.instance_id]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_saved_file_reports_no_family_rather_than_guessing_one() {
        // The interchange format carries no type word. Inventing a family would
        // make per-family measurements (paint, handles, limbtype) look answered.
        let source = DocumentSource::SavedFile(PathBuf::from("/tmp/whatever.sporedoc"));
        assert_eq!(source.family(), None);
        assert_eq!(source.key(), None);
        assert_eq!(source.as_str(), "saved-file");
    }

    #[test]
    fn every_family_is_recoverable_from_its_own_type_id() {
        for family in Family::ALL {
            assert_eq!(Family::from_type_id(family.type_id()), Some(family));
            let source = DocumentSource::for_record(ResourceKey::new(family.type_id(), 1, 2))
                .expect("a known family");
            assert_eq!(source.family(), Some(family));
        }
        let unknown = DocumentSource::for_record(ResourceKey::new(0xDEAD_BEEF, 1, 2))
            .expect_err("an unknown type id is refused, not re-typed");
        assert!(
            unknown.to_string().contains("authoring families"),
            "{unknown}"
        );
    }

    #[test]
    fn a_creator_with_no_packages_names_the_fix() {
        let error = load_creator(
            &DocumentSource::for_record(ResourceKey::new(Family::Creature.type_id(), 1, 2))
                .expect("known family"),
            &[],
        )
        .unwrap_err();
        let message = error.to_string();
        assert!(message.contains("--package"), "{message}");
        assert!(message.contains("Spore_Graphics.package"), "{message}");
    }

    #[test]
    fn a_creator_naming_a_missing_package_names_the_path() {
        let error = load_creator(
            &DocumentSource::for_record(ResourceKey::new(Family::Creature.type_id(), 1, 2))
                .expect("known family"),
            &[PathBuf::from("/definitely/not/here.package")],
        )
        .unwrap_err();
        assert!(
            error.to_string().contains("/definitely/not/here.package"),
            "{error}"
        );
    }

    #[test]
    fn an_empty_document_is_refused_rather_than_projected_to_nothing() {
        let error = project_document(
            CreatorDocument::default(),
            &DocumentSource::SavedFile(PathBuf::from("/tmp/x")),
            &spore_morphology::MemorySource::new(),
            0,
        )
        .unwrap_err();
        assert!(matches!(error, CreatorLoadError::NoBlocks), "{error}");
    }

    #[test]
    fn an_unresolvable_document_still_projects_one_entry_per_block() {
        let stage = stage(&document_of(3), &spore_morphology::MemorySource::new());
        assert_eq!(stage.block_count(), 3);
        assert_eq!(stage.projection.counts().unresolved, 3);
        assert_eq!(stage.visible_block_count(), 0);
        assert_eq!(stage.without_geometry_count(), 0);
        // Every miss is accounted for, so "nothing resolved" is distinguishable
        // from "the source was never wired up".
        assert!(stage.lookups >= 3);
        assert_eq!(stage.misses, stage.lookups);
    }

    #[test]
    fn the_report_line_states_the_split_even_when_nothing_resolved() {
        let stage = stage(&document_of(2), &spore_morphology::MemorySource::new());
        let lines = stage.report_lines();
        assert!(
            lines[0].starts_with("OPENSPORE-CREATOR v1 source=record"),
            "{lines:?}"
        );
        assert!(lines[0].contains("blocks=2"), "{lines:?}");
        assert!(lines[0].contains("resolved=0"), "{lines:?}");
        assert!(lines[0].contains("unresolved=2"), "{lines:?}");
        assert!(lines[0].contains("visible=0"), "{lines:?}");
        // One line per failure, each naming a stage and a reason.
        assert_eq!(lines.len(), 3, "{lines:?}");
        for line in &lines[1..] {
            assert!(line.contains("resolved=false"), "{line}");
            assert!(line.contains("stage="), "{line}");
            assert!(line.contains("reason="), "{line}");
        }
    }

    #[test]
    fn a_failed_report_line_is_versioned_and_carries_the_reason() {
        let lines = CreatorDecoded::Failed("no such record".into()).report_lines();
        assert_eq!(lines.len(), 1);
        assert!(
            lines[0].starts_with("OPENSPORE-CREATOR v1 failed:"),
            "{lines:?}"
        );
        assert!(lines[0].contains("no such record"), "{lines:?}");
    }

    #[test]
    fn a_gmsh_record_lifts_into_the_one_engine_neutral_buffer_type() {
        let stage = stage(&document_of(3), &complete_source());
        let part = stage.projection.blocks()[0]
            .part
            .as_ref()
            .expect("block 0 resolved");
        let buffers = part_buffers(part).expect("gmsh geometry decoded");
        assert_eq!(buffers.topology, Topology::TriangleList);
        assert_eq!(buffers.positions.len(), 3);
        assert_eq!(buffers.indices, vec![0, 1, 2]);
        // Stated absences, not omissions: the `gmsh` record has no TEXCOORD and
        // no NORMAL element, so there is nothing to carry.
        assert!(buffers.uvs.is_none());
        assert!(buffers.raw_normal_bytes.is_none());
        let mesh = convert::to_bevy_mesh(&buffers, NORMAL_MODE);
        assert_eq!(mesh.count_vertices(), 3);
    }

    #[test]
    fn a_part_mesh_is_lit_and_claims_no_tangent_frame() {
        let stage = stage(&document_of(3), &complete_source());
        let part = stage.projection.blocks()[0].part.as_ref().unwrap();
        let buffers = part_buffers(part).unwrap();
        let mesh = convert::to_bevy_mesh(&buffers, NORMAL_MODE);
        // The normal attribute is present under the computed policy, which is
        // what makes the part lit rather than black.
        assert!(mesh.attribute(Mesh::ATTRIBUTE_NORMAL).is_some());
        // And the tangent frame is genuinely absent, because there is no UV to
        // build one from. A mesh claiming otherwise would bind a normal map and
        // shade as though it were not there.
        assert!(mesh.attribute(Mesh::ATTRIBUTE_TANGENT).is_none());
    }

    #[test]
    fn a_part_with_no_container_skeleton_says_none_and_not_zero() {
        // 256 of 1824 real containers hold no skeleton. `None` is that state;
        // `Some(0)` would be a different claim.
        let stage = stage(&document_of(3), &complete_source());
        let part = stage.projection.blocks()[0].part.as_ref().unwrap();
        assert_eq!(part.bones, None);
        assert_eq!(part.triangles, Some(1));
        assert_eq!(part.vertices, Some(3));
        assert_eq!(stage.projection.total_bones(), 0);
    }

    #[test]
    fn the_runtime_state_lists_every_block_including_the_ones_with_no_entity() {
        let stage = stage(&document_of(3), &spore_morphology::MemorySource::new());
        let state = stage.runtime_state(Some(1));
        assert_eq!(state.block_count, 3);
        assert_eq!(state.resolved_count, 0);
        assert_eq!(state.unresolved_count, 3);
        assert_eq!(state.visible_block_count, 0, "nothing is on screen yet");
        assert_eq!(state.blocks.len(), 3);
        assert_eq!(state.selected_block, Some(1));
        for (index, block) in state.blocks.iter().enumerate() {
            assert_eq!(block.block, index as u32);
            assert!(!block.resolved);
            assert!(!block.visible);
            assert!(
                block.stop_stage.is_some(),
                "block {index} must say where it stopped"
            );
            assert!(block.stop_reason.is_some(), "block {index} must say why");
            assert_eq!(block.weakest_grade, "none");
        }
    }

    #[test]
    fn the_runtime_state_carries_the_part_keys_for_a_resolved_block() {
        let stage = stage(&document_of(3), &complete_source());
        let state = stage.runtime_state(None);
        assert_eq!(state.resolved_count, 3);
        assert_eq!(state.visible_block_count, 0, "set by the spawn, not here");
        assert_eq!(state.total_triangles, 3);
        assert_eq!(state.total_vertices, 9);
        assert_eq!(state.selected_block, None);
        for block in &state.blocks {
            assert!(block.resolved);
            assert!(block.visible, "the gmsh record decoded");
            assert_eq!(
                block.container,
                Some([
                    spore_morphology::resolve::TYPE_RW4,
                    0x4060_6000,
                    0x0001_3530
                ])
            );
            assert_eq!(
                block.geometry,
                Some([
                    spore_morphology::resolve::TYPE_GMSH,
                    0x4060_6000,
                    0x0001_3530
                ])
            );
            assert_eq!(block.block_id, Some([0x4062_6000, 0x0001_3530]));
            assert!(block.stop_stage.is_none());
        }
    }

    #[test]
    fn nothing_resolved_frames_no_camera_rather_than_the_origin() {
        let stage = stage(&document_of(2), &spore_morphology::MemorySource::new());
        assert!(
            stage.camera_framing().is_none(),
            "a creature that failed to compose must not be framed as though it succeeded"
        );
    }

    #[test]
    fn a_resolved_document_frames_from_the_projections_own_bounds() {
        let stage = stage(&document_of(3), &complete_source());
        let framing = stage
            .camera_framing()
            .expect("three resolved parts have bounds");
        assert_eq!(framing.vertical_fov_degrees, DEFAULT_VERTICAL_FOV_DEGREES);
        assert!(framing.bounds.1[0] >= framing.bounds.0[0]);
    }

    // ---- fixtures -------------------------------------------------------

    /// The part identity every fixture block names.
    const FIXTURE_BLOCK_ID: &str = "0x40626000, 0x00013530";

    /// A `<sporemodel>` document with `count` sibling blocks at one identity.
    ///
    /// Written in the corpus's own grammar rather than built as a struct, because
    /// `parse_sporemodel` is the path a real document takes and a struct literal
    /// would test the projection against something the parser never produces.
    fn document_of(count: usize) -> CreatorDocument {
        let mut blocks = String::new();
        for _ in 0..count {
            blocks.push_str(&format!(
                "<blockref><blockid>{FIXTURE_BLOCK_ID}</blockid><transform>\
                 <position>0,0,0</position><scale>1</scale><orientation>\
                 <row0>1,0,0</row0><row1>0,1,0</row1><row2>0,0,1</row2>\
                 </orientation></transform></blockref>"
            ));
        }
        let xml = format!(
            "<sporemodel><formatversion>16</formatversion>\
             <blocks count=\"{count}\">{blocks}</blocks></sporemodel>"
        );
        parse_sporemodel(Family::Creature, &xml).expect("the fixture parses")
    }

    /// A resolved creator document MUST produce a camera framing.
    ///
    /// Found by INSPECTING A SCREENSHOT, not by any test. The creature rendered as a
    /// handful of pixels in an 800x600 frame while `CreatorRuntimeState` reported 6
    /// resolved blocks, 384 triangles and `scene_stable: true`. Every number said it
    /// was staged; only the picture said it was framed wrong.
    ///
    /// The cause is that `frame_creator_camera` takes `Option<Res<CameraFraming>>`.
    /// An absent resource is not an error anywhere -- it just returns, leaving the
    /// camera at `ScenePlugin`'s hardcoded `(0, 6, 34)`. A real part's geometry is
    /// 0.41 units across with 0.36 units of authored position span, so a correctly
    /// framed creature is ~0.7 units tall and at distance 34 that is ~2% of the view.
    ///
    /// So the assertion is on the plumbing: the projection resolves, therefore the
    /// framing exists, therefore the camera moves. A test that only counted entities
    /// would have passed throughout.
    #[test]
    fn a_resolved_projection_must_yield_a_camera_framing() {
        let document = document_of(3);
        let source = complete_source();
        let stage = stage(&document, &source);
        assert_eq!(
            stage.projection.counts().unresolved,
            0,
            "the fixture is built so every block resolves"
        );
        let framing = stage
            .camera_framing()
            .expect("a projection with resolved geometry must produce camera framing");
        // A real box on the axes the fixture's geometry actually spans. Z is flat in
        // this fixture by construction, so demanding min < max on Z would be
        // asserting that the fixture is not the fixture -- an over-strict assertion
        // that fails on correct output and teaches nothing.
        let (lo, hi) = framing.bounds;
        assert!(
            hi[0] > lo[0] && hi[1] > lo[1],
            "framing bounds must be a real box on the axes the geometry spans, got {lo:?}..{hi:?}"
        );
        assert!(
            hi.iter().all(|v| v.is_finite()) && lo.iter().all(|v| v.is_finite()),
            "framing bounds must be finite, got {lo:?}..{hi:?}"
        );
    }

    /// And the negative direction: nothing resolved means NO framing.
    ///
    /// Framing a camera on an empty projection puts it in the origin, which reads as a
    /// composed creature of zero size rather than as a failure.
    #[test]
    fn a_projection_with_no_geometry_yields_no_framing() {
        let document = document_of(1);
        // An empty source resolves nothing, so every block is unresolved.
        let stage = stage(&document, &spore_morphology::MemorySource::default());
        assert!(stage.projection.counts().resolved == 0);
        assert!(
            stage.camera_framing().is_none(),
            "a creature that failed to compose must not be framed at all"
        );
    }

    /// The concrete scale mismatch, so the next person to touch framing sees it.
    #[test]
    fn a_real_part_is_far_smaller_than_the_default_camera_distance() {
        // Measured, not assumed: the `gmsh` for the composed parts is sub-unit.
        let part_span = 0.41f32;
        let authored_span = 0.36f32;
        let creature = part_span + authored_span;
        let default_distance = 34.0f32;
        let fraction = creature / default_distance;
        assert!(
            fraction < 0.05,
            "a {creature:.2}-unit creature at distance {default_distance} spans \
             {fraction:.3} of the view, which is what the screenshot showed"
        );
    }

    fn stage(document: &CreatorDocument, source: &dyn RecordSource) -> CreatorStage {
        project_document(
            document.clone(),
            &DocumentSource::for_record(ResourceKey::new(Family::Creature.type_id(), 1, 2))
                .expect("known family"),
            source,
            1,
        )
        .expect("projects")
    }

    /// A source holding the whole resolver chain for one part identity.
    ///
    /// The byte layouts are the ones `spore-morphology`'s hermetic resolver tests
    /// use, so the chain walked here is the same chain walked there.
    fn complete_source() -> spore_morphology::MemorySource {
        // `spore-morphology` re-exports the four chain type ids, which is why
        // this fixture needs no direct dependency on `spore-rigblock`.
        use spore_morphology::resolve::{
            GROUP_PART, TYPE_GMSH, TYPE_PROP, TYPE_RIGBLOCK, TYPE_RW4,
        };

        const GROUP: u32 = 0x4062_6000;
        const INSTANCE: u32 = 0x0001_3530;

        // rigblock: k = 0, so a 36-byte header and no cells.
        let mut rigblock = vec![0u8; 36];
        for axis in 0..6 {
            rigblock[4 * axis..4 * axis + 4].copy_from_slice(&1.0f32.to_le_bytes());
        }
        // prop: one validated 16-byte pointer entry.
        let mut prop = Vec::new();
        prop.extend_from_slice(&INSTANCE.to_le_bytes());
        prop.extend_from_slice(&TYPE_RW4.to_le_bytes());
        prop.extend_from_slice(&GROUP_PART.to_le_bytes());
        prop.extend_from_slice(&0x00A0_0CE4u32.to_le_bytes());

        let gmsh = gmsh_record();

        let mut source = spore_morphology::MemorySource::new();
        source.insert(ResourceKey::new(TYPE_RIGBLOCK, GROUP, INSTANCE), rigblock);
        source.insert(ResourceKey::new(TYPE_PROP, GROUP, INSTANCE), prop);
        // An empty container: the part resolves and its skeleton is `None`, which
        // is the real corpus state of 256 of 1824 containers.
        source.insert(ResourceKey::new(TYPE_RW4, GROUP_PART, INSTANCE), Vec::new());
        source.insert(ResourceKey::new(TYPE_GMSH, GROUP_PART, INSTANCE), gmsh);
        source
    }

    /// A minimal real `gmsh` record: 3 vertices, 1 triangle, 1 bone.
    fn gmsh_record() -> Vec<u8> {
        let mut out = vec![0u8; 0x18];
        out[0x00..0x04].copy_from_slice(&0x0100_0000u32.to_le_bytes());
        out[0x04..0x08].copy_from_slice(&0x0100_0000u32.to_le_bytes());
        out[0x08..0x0C].copy_from_slice(&0x0000_0400u32.to_le_bytes());
        out[0x0C..0x10].copy_from_slice(&0x0001_0300u32.to_le_bytes());
        out[0x10..0x14].copy_from_slice(&3u32.to_le_bytes());
        out[0x14..0x16].copy_from_slice(&3u16.to_be_bytes());
        out[0x16] = 0x00;
        out[0x17] = 0x0C;
        for corner in [[0.0f32, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]] {
            for value in corner {
                out.extend_from_slice(&value.to_le_bytes());
            }
        }
        // Block fields are big-endian: id, count, elemSize.
        out.extend_from_slice(&0x0000_0001u32.to_be_bytes());
        out.extend_from_slice(&3u32.to_be_bytes());
        out.extend_from_slice(&2u16.to_be_bytes());
        for index in [0u16, 1, 2] {
            out.extend_from_slice(&index.to_le_bytes());
        }
        out.extend_from_slice(&0x1500_0608u32.to_be_bytes());
        out.extend_from_slice(&1u32.to_be_bytes());
        out.extend_from_slice(&4u16.to_be_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(&0x0A00_0400u32.to_be_bytes());
        out.extend_from_slice(&3u32.to_be_bytes());
        out.extend_from_slice(&16u16.to_be_bytes());
        for _ in 0..3 {
            for value in [0.0f32, 0.0, 0.0, 1.0] {
                out.extend_from_slice(&value.to_le_bytes());
            }
        }
        // Tail: nv, K1 = 2, the K1 payload, nBones, the 4/0/0/0 word, one range,
        // then the 14-byte T2 entry. `K1` is a SECOND ordinal index space, not
        // the bone count, which is why the bone word is written separately.
        out.extend_from_slice(&3u32.to_be_bytes());
        out.push(2);
        out.push(0);
        for index in 0..2u8 {
            out.push(index);
            out.extend_from_slice(&[0xFF, 0xFF, 0x00]);
        }
        out.extend_from_slice(&1u32.to_be_bytes());
        out.extend_from_slice(&[4, 0, 0, 0]);
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(&3u32.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(&4u32.to_le_bytes());
        out.extend_from_slice(&0u32.to_be_bytes());
        out.extend_from_slice(&31u16.to_be_bytes());
        out.extend_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
        out.extend_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
        out
    }
}
