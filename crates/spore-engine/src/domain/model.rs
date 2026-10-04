//! One model record, decoded: its parts, its materials, its extent.
//!
//! A *model definition* is what one `gmdl` record is. A *model instance* is
//! where one definition is placed. Keeping them apart is what makes a scene able
//! to hold the same record twice — and what makes a two-mesh record and a
//! two-record creature different kinds of thing, which they are.

use spore_assets::{LoadedModel, ModelFormat};
use spore_core::ResourceKey;
use spore_gmdl::Mesh;

use crate::convert::{self, MeshBuffers, NormalMode};
use crate::domain::materials::ModelMaterials;
use crate::domain::{grow, mesh_triangles, union_bounds, Bounds};

/// How far a record's decode got.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecodeStatus {
    /// The walk closed exactly on the record end.
    Complete,
    /// The walk stopped at a known stage, with the reason.
    Stopped {
        /// Where it stopped, as `spore-gmdl` spells it.
        stage: String,
        /// Why, as `spore-gmdl` spells it.
        error: String,
    },
}

impl DecodeStatus {
    /// The canonical spelling for a report line.
    pub fn describe(&self) -> String {
        match self {
            Self::Complete => "complete".to_owned(),
            Self::Stopped { stage, error } => format!("{stage}: {error}"),
        }
    }

    /// Whether the geometry itself is complete.
    ///
    /// A `Stopped { stage: "material-info" }` answer still means every index
    /// buffer, vertex descriptor, vertex buffer, mesh and material id was
    /// validated, because those tables sit *behind* the point where the walk
    /// stopped. Which is why `creature` renders with 47 016 triangles and a
    /// `decode=material-info` report.
    pub fn geometry_complete(&self) -> bool {
        match self {
            Self::Complete => true,
            Self::Stopped { stage, .. } => {
                matches!(stage.as_str(), "material-info" | "trailer" | "unknown-key")
            }
        }
    }
}

/// One mesh of a model, with the material id the record gave it.
#[derive(Debug, Clone)]
pub struct ModelPart {
    /// The mesh's index in the record's mesh table.
    pub mesh_index: u32,
    /// The decoded geometry.
    pub mesh: Mesh,
    /// The material id associated with this mesh. A bare word; no registry
    /// names it.
    pub material_id: u32,
}

impl ModelPart {
    /// Lifts the geometry into engine-neutral buffers.
    pub fn buffers(&self) -> MeshBuffers {
        convert::to_buffers(&self.mesh)
    }

    /// Triangles this part contributes, after fan expansion.
    pub fn triangles(&self) -> usize {
        mesh_triangles(&self.buffers())
    }
}

/// A model record, decoded and ready to be instanced.
#[derive(Debug, Clone)]
pub struct ModelDefinition {
    /// The identity that was loaded.
    pub key: ResourceKey,
    /// Which package answered.
    pub package_name: String,
    /// The container it decoded as.
    pub format: ModelFormat,
    /// One part per mesh, in record order.
    pub parts: Vec<ModelPart>,
    /// The roles each part's material resolved to.
    ///
    /// `None` means texture resolution was **not requested**, which is not the
    /// same as "the model has no textures" and not the same as "every reference
    /// failed". The report line distinguishes all three.
    pub materials: Option<ModelMaterials>,
    /// How far the decode got.
    pub decode: DecodeStatus,
}

impl ModelDefinition {
    /// Builds a definition from a `spore-assets` load and, optionally, a role
    /// table.
    ///
    /// The material ids come from the gmdl record where there is one, and are
    /// **not** invented for a record that has none: a missing id is `0`, which
    /// is a bare word like any other and claims nothing.
    pub fn from_loaded(model: &LoadedModel, materials: Option<ModelMaterials>) -> Self {
        let material_ids = model
            .gmdl
            .as_ref()
            .map(|gmdl| gmdl.material_ids.clone())
            .unwrap_or_default();
        let parts = model
            .meshes
            .iter()
            .enumerate()
            .map(|(mesh_index, mesh)| ModelPart {
                mesh_index: mesh_index as u32,
                mesh: mesh.clone(),
                material_id: material_ids.get(mesh_index).copied().unwrap_or(0),
            })
            .collect();
        Self {
            key: model.key,
            package_name: model.package_name.clone(),
            format: model.format,
            parts,
            materials,
            decode: match model.stopped_at.as_ref() {
                None => DecodeStatus::Complete,
                Some(stop) => DecodeStatus::Stopped {
                    stage: stop.stage.as_str().to_owned(),
                    // The raw error only. `DecodeStatus::describe` composes
                    // `stage: error`, and pre-formatting it here printed the
                    // stage twice -- which is how a report line ends up saying
                    // `decode=material-info: material-info: gmdl: ...`.
                    error: stop.error.to_string(),
                },
            },
        }
    }

    /// Triangles across every part, after fan expansion.
    pub fn triangles(&self) -> usize {
        self.parts.iter().map(ModelPart::triangles).sum()
    }

    /// The union of every part's bounds, or `None` when there are none.
    pub fn bounds(&self) -> Bounds {
        union_bounds(self.parts.iter().map(|part| &part.mesh))
    }

    /// The bounds grown by `extra`, for instancing.
    pub fn bounds_within(&self, extra: Bounds) -> Bounds {
        grow(self.bounds(), extra)
    }

    /// How many of this model's parts carry a normal map.
    ///
    /// `0` when textures were not requested, which the report distinguishes from
    /// "resolved and none was a normal map".
    pub fn parts_with_normal_maps(&self) -> usize {
        self.plans()
            .iter()
            .filter(|plan| plan.normal_map.is_some())
            .count()
    }

    /// How many mask channels this model offers, across every part.
    pub fn mask_channels(&self) -> usize {
        self.plans().iter().map(|plan| plan.masks.len()).sum()
    }

    /// The per-mesh material plans, or an empty list when textures were not
    /// requested.
    pub fn plans(&self) -> Vec<crate::domain::materials::MeshMaterialPlan> {
        self.materials
            .as_ref()
            .map_or_else(Vec::new, ModelMaterials::plans)
    }

    /// The role table, when textures were resolved.
    pub fn materials(&self) -> Option<&ModelMaterials> {
        self.materials.as_ref()
    }
}

/// Where one model instance sits in the world.
///
/// Plain numbers, not `bevy::math::TransformV3`, because this is the domain and
/// the domain does not depend on the renderer. The conversion happens once, in
/// [`crate::loader`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placement {
    /// World translation.
    pub translation: [f32; 3],
    /// Rotation about +Y, in degrees.
    pub rotation_y_degrees: f32,
    /// Uniform scale.
    pub scale: f32,
}

impl Default for Placement {
    fn default() -> Self {
        Self {
            translation: [0.0; 3],
            rotation_y_degrees: 0.0,
            scale: 1.0,
        }
    }
}

impl Placement {
    /// An identity placement at `translation`.
    pub fn at(translation: [f32; 3]) -> Self {
        Self {
            translation,
            ..Self::default()
        }
    }

    /// The translation the model must be lifted by so its lowest point sits on
    /// `floor_y`, given its own bounds.
    ///
    /// Returns `None` for absent bounds rather than zero: a model with no
    /// meshes has no floor to sit on.
    pub fn lifted_onto(&self, floor_y: f32, bounds: Bounds) -> Option<[f32; 3]> {
        let (lo, _hi) = bounds?;
        let mut translation = self.translation;
        translation[1] += floor_y - lo[1];
        Some(translation)
    }
}

/// One placed copy of a model definition.
#[derive(Debug, Clone, PartialEq)]
pub struct ModelInstance {
    /// Which record to instantiate.
    pub key: ResourceKey,
    /// A label.
    ///
    /// `None` unless the caller supplied one. **DBPF has no string table**, so
    /// no name is ever derived from a record; a label is either given by the
    /// user or absent.
    pub label: Option<String>,
    /// Where it sits.
    pub placement: Placement,
}

impl ModelInstance {
    /// An instance with a derived placement and no label.
    pub fn new(key: ResourceKey, placement: Placement) -> Self {
        Self {
            key,
            label: None,
            placement,
        }
    }
}

/// The normal policy this build uses when staging geometry.
pub const DEFAULT_NORMAL_MODE: NormalMode = NormalMode::Computed;

#[cfg(test)]
mod tests {
    use super::*;
    use spore_assets::{LoadedModel, ModelFormat};

    fn triangle_mesh() -> Mesh {
        Mesh {
            positions: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            normals: vec![[0.0, 0.0, 1.0]; 3],
            uvs: vec![[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]],
            indices: vec![0, 1, 2],
            topology: spore_gmdl::Topology::TriangleList,
            bounds_min: [0.0, -2.0, 0.0],
            bounds_max: [1.0, 0.0, 1.0],
            radius: 1.5,
        }
    }

    fn loaded(meshes: Vec<Mesh>) -> LoadedModel {
        LoadedModel {
            key: ResourceKey::new(0x00E6_BCE5, 0x4062_7100, 0x067C_79D2),
            package_name: "mini".into(),
            format: ModelFormat::Gmdl,
            gmdl: None,
            rw4: None,
            meshes,
            texture_refs: Vec::new(),
            stopped_at: None,
        }
    }

    #[test]
    fn a_complete_record_says_complete_and_claims_complete_geometry() {
        let status = DecodeStatus::Complete;
        assert_eq!(status.describe(), "complete");
        assert!(status.geometry_complete());
    }

    #[test]
    fn a_material_info_stop_still_claims_complete_geometry() {
        // The creature record: 47 016 triangles and `decode=material-info`.
        let status = DecodeStatus::Stopped {
            stage: "material-info".into(),
            error: "material-info: undocumented shader-data id 0x218".into(),
        };
        assert!(status.geometry_complete(), "the mesh table is behind it");
        assert!(status.describe().contains("0x218"));
    }

    #[test]
    fn a_mesh_table_stop_does_not_claim_complete_geometry() {
        let status = DecodeStatus::Stopped {
            stage: "mesh-table".into(),
            error: "truncated".into(),
        };
        assert!(!status.geometry_complete());
    }

    #[test]
    fn triangles_are_counted_after_fan_expansion() {
        // Three fan indices are already exactly one triangle; four become two.
        for (indices, expected) in [(vec![0, 1, 2], 1), (vec![0, 1, 2, 0], 2)] {
            let mut fan = triangle_mesh();
            fan.topology = spore_gmdl::Topology::TriangleFan;
            fan.indices = indices;
            let part = ModelPart {
                mesh_index: 0,
                mesh: fan,
                material_id: 0,
            };
            assert_eq!(part.triangles(), expected);
            assert_eq!(part.buffers().indices.len() % 3, 0);
        }
    }

    #[test]
    fn a_triangle_list_reports_one_triangle() {
        let part = ModelPart {
            mesh_index: 0,
            mesh: triangle_mesh(),
            material_id: 0x40DE_AD00,
        };
        assert_eq!(part.triangles(), 1);
        assert_eq!(part.material_id, 0x40DE_AD00);
    }

    #[test]
    fn bounds_are_the_union_and_absent_for_no_parts() {
        let model =
            ModelDefinition::from_loaded(&loaded(vec![triangle_mesh()]), Some(empty_materials()));
        let (lo, hi) = model.bounds().unwrap();
        assert_eq!(lo, [0.0, -2.0, 0.0]);
        assert_eq!(hi, [1.0, 0.0, 1.0]);
        let empty = ModelDefinition::from_loaded(&loaded(Vec::new()), Some(empty_materials()));
        assert_eq!(empty.bounds(), None);
    }

    #[test]
    fn a_definition_whose_textures_were_never_requested_says_so() {
        let model = ModelDefinition::from_loaded(&loaded(vec![triangle_mesh()]), None);
        assert_eq!(model.materials(), None, "not asked is not 'none found'");
        assert!(model.plans().is_empty());
        assert_eq!(model.parts_with_normal_maps(), 0);
        assert_eq!(model.mask_channels(), 0);

        let resolved =
            ModelDefinition::from_loaded(&loaded(vec![triangle_mesh()]), Some(empty_materials()));
        assert!(resolved.materials().is_some(), "asked, and looked");
        assert_eq!(resolved.materials().unwrap().attempted(), 0);
    }

    fn empty_materials() -> ModelMaterials {
        ModelMaterials::empty(None)
    }

    #[test]
    fn a_placement_lifts_a_model_onto_a_floor_and_reports_no_floor_without_bounds() {
        let placement = Placement::at([1.0, 0.0, 2.0]);
        assert_eq!(
            placement.lifted_onto(0.0, Some(([0.0, -2.0, 0.0], [1.0, 0.0, 0.0]))),
            Some([1.0, 2.0, 2.0])
        );
        assert_eq!(placement.lifted_onto(0.0, None), None);
    }

    #[test]
    fn a_model_instance_has_no_label_unless_one_was_given() {
        let key = ResourceKey::new(1, 2, 3);
        let instance = ModelInstance::new(key, Placement::default());
        assert_eq!(instance.label, None);
        // Nothing derives one: DBPF has no string table.
        assert_eq!(instance.key, key);
    }
}
