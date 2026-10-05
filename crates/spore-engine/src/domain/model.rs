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
use crate::domain::paint::{self, MeshPaint, PaintBinding};
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
    /// Where this model's colour came from, and with what grade.
    ///
    /// [`PaintBinding::Unavailable`] means no `sporemodel` record was found. It
    /// is **not** a colour and **not** a neutral tint: see
    /// [`crate::domain::paint`]. Resolution is independent of `materials` — a
    /// model can be fully textured and still have no colour, which is the
    /// corpus's normal state for everything but the mask and normal families.
    pub paint: PaintBinding,
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
    ///
    /// `paint` is passed in rather than resolved here so the store is read once
    /// per record by the caller, in one place, beside the model load it belongs
    /// with.
    pub fn from_loaded(
        model: &LoadedModel,
        materials: Option<ModelMaterials>,
        paint: PaintBinding,
    ) -> Self {
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
            paint,
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

    /// The colour source bound to this model.
    pub fn paint(&self) -> &PaintBinding {
        &self.paint
    }

    /// One mesh's resolved base colour.
    ///
    /// The whole colour path, from the record's bytes to the numbers the
    /// renderer multiplies into `base_color`, in one call — and callable with no
    /// window, no GPU and no game install.
    pub fn mesh_paint(&self, mesh_index: u32) -> MeshPaint {
        paint::mesh_paint(&self.paint, mesh_index)
    }

    /// How many of this model's parts will be given a colour from a
    /// `sporemodel` record.
    pub fn parts_painted(&self) -> usize {
        match &self.paint {
            PaintBinding::Bound(_) => self.parts.len(),
            PaintBinding::Unavailable { .. } => 0,
        }
    }
}

/// How a placement's orientation is spelled.
///
/// Re-exported from [`spore_core`], where it lives because `spore-creator-core` needs
/// the same vocabulary and neither may depend on the other. The reasoning is in
/// [`spore_core::orientation`]; the short version is that a single definition of "a
/// rotation" is what stops the authoring layer and the engine from spelling it
/// differently.
///
/// The `Placement` field below is `General` whenever it came from a creator block,
/// because a creator block's `<orientation>` is a row-major 3×3 that is not a Y
/// rotation in general. `Placement::rotated_y` is the constructor for the case that
/// genuinely is one, so the two cases stay distinguishable at the call site.
pub use spore_core::Orientation;

/// Where one model instance sits in the world.
///
/// Plain numbers, not `bevy::math::Transform`, because this is the domain and the
/// domain does not depend on the renderer. The conversion happens once, in
/// [`crate::loader`].
///
/// `orientation` replaces a bare `rotation_y_degrees` field. The old field could
/// not express a creator block's `<orientation>`, which the save format spells as a
/// row-major 3x3 and which is **not** a Y rotation in general — collapsing it would
/// have quietly moved parts. `Orientation::YRotation` keeps every existing
/// hand-authored placement working and readable, and `Placement::rotation_y_degrees`
/// is retained as the accessor for code that genuinely only wants the angle.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placement {
    /// World translation.
    pub translation: [f32; 3],
    /// How the orientation is spelled.
    pub orientation: Orientation,
    /// Uniform scale.
    pub scale: f32,
}

impl Default for Placement {
    fn default() -> Self {
        Self {
            translation: [0.0; 3],
            orientation: Orientation::default(),
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

    /// A placement rotated `degrees` about +Y at `translation`.
    ///
    /// The constructor for the case that genuinely is a Y rotation, so a caller
    /// saying "rotate this by 30 degrees" produces `YRotation(30.0)` rather than a
    /// matrix that would compare unequal to one built the other way.
    pub fn rotated_y(translation: [f32; 3], degrees: f32) -> Self {
        Self {
            translation,
            orientation: Orientation::YRotation(degrees),
            scale: 1.0,
        }
    }

    /// A placement carrying `orientation` verbatim at `translation`.
    pub fn oriented(translation: [f32; 3], orientation: Orientation) -> Self {
        Self {
            translation,
            orientation,
            scale: 1.0,
        }
    }

    /// The Y angle in degrees, when this placement is a Y rotation.
    ///
    /// `None` for a general orientation. See [`Orientation::as_y_rotation`] for why
    /// there is deliberately no fallback number here: a number there would make a
    /// tilted part look merely miscalibrated instead of wrong.
    pub fn rotation_y_degrees(&self) -> Option<f32> {
        self.orientation.as_y_rotation()
    }

    /// The translation the model must be lifted by so its lowest point sits on
    /// `floor_y`, given its own bounds.
    ///
    /// Returns `None` for absent bounds rather than zero: a model with no meshes has
    /// no floor to sit on.
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
        let model = ModelDefinition::from_loaded(
            &loaded(vec![triangle_mesh()]),
            Some(empty_materials()),
            no_paint(),
        );
        let (lo, hi) = model.bounds().unwrap();
        assert_eq!(lo, [0.0, -2.0, 0.0]);
        assert_eq!(hi, [1.0, 0.0, 1.0]);
        let empty =
            ModelDefinition::from_loaded(&loaded(Vec::new()), Some(empty_materials()), no_paint());
        assert_eq!(empty.bounds(), None);
    }

    #[test]
    fn a_definition_whose_textures_were_never_requested_says_so() {
        let model = ModelDefinition::from_loaded(&loaded(vec![triangle_mesh()]), None, no_paint());
        assert_eq!(model.materials(), None, "not asked is not 'none found'");
        assert!(model.plans().is_empty());
        assert_eq!(model.parts_with_normal_maps(), 0);
        assert_eq!(model.mask_channels(), 0);

        let resolved = ModelDefinition::from_loaded(
            &loaded(vec![triangle_mesh()]),
            Some(empty_materials()),
            no_paint(),
        );
        assert!(resolved.materials().is_some(), "asked, and looked");
        assert_eq!(resolved.materials().unwrap().attempted(), 0);
    }

    fn empty_materials() -> ModelMaterials {
        ModelMaterials::empty(None)
    }

    /// The binding a unit test means by "we did not look for a colour": the
    /// unavailable variant, never a neutral tint.
    fn no_paint() -> PaintBinding {
        PaintBinding::Unavailable {
            reason: crate::domain::paint::PaintUnavailableReason::NoSporemodelRecords,
            fact: spore_core::evidence::Fact::unavailable("unit test"),
        }
    }

    #[test]
    fn a_definition_without_a_colour_source_paints_nothing_and_says_so() {
        let model = ModelDefinition::from_loaded(&loaded(vec![triangle_mesh()]), None, no_paint());
        assert!(!model.paint().is_bound());
        assert_eq!(model.parts_painted(), 0);
        // Not black, not a default tint: an absence the renderer can report.
        assert_eq!(model.mesh_paint(0).base_color, None);
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

#[cfg(test)]
mod orientation_tests {
    use super::*;
    use spore_creator_core::document::Matrix3;

    fn matrix(rows: [[f32; 3]; 3]) -> Matrix3 {
        Matrix3 {
            row0: rows[0],
            row1: rows[1],
            row2: rows[2],
        }
    }

    #[test]
    fn a_default_orientation_is_a_zero_y_rotation_and_not_a_matrix() {
        // Structurally, not numerically: both denote the identity, so only the
        // variant can tell an unspecified orientation from a chosen one.
        assert_eq!(Orientation::default(), Orientation::YRotation(0.0));
        assert_ne!(
            Orientation::default(),
            Orientation::General(Matrix3::IDENTITY),
            "defaulting to a matrix would make an unspecified orientation look chosen"
        );
    }

    #[test]
    fn the_identity_counts_as_a_y_rotation_which_a_nonzero_test_would_miss() {
        // The identity has both off-diagonal terms zero, so a "is the sin term
        // non-zero" test cannot see it. It is still a Y rotation about any axis.
        assert!(Orientation::General(Matrix3::IDENTITY).is_y_rotation());
        assert_eq!(
            Orientation::General(Matrix3::IDENTITY).as_y_rotation(),
            Some(0.0)
        );
    }

    #[test]
    fn a_general_orientation_reports_no_y_angle_rather_than_a_best_effort_one() {
        // A rotation about X by 30 degrees. Reporting its Y component (0.0 here)
        // would make a tilted part look merely miscalibrated instead of wrong.
        let m = matrix([
            [1.0, 0.0, 0.0],
            [0.0, 0.866_025_4, -0.5],
            [0.0, 0.5, 0.866_025_4],
        ]);
        let o = Orientation::General(m);
        assert!(!o.is_y_rotation());
        assert_eq!(o.as_y_rotation(), None);
    }

    #[test]
    fn a_y_rotation_survives_the_round_trip_through_the_matrix() {
        for degrees in [0.0f32, 1.0, 30.0, 90.0, 179.0, -45.0, 360.0] {
            let o = Orientation::YRotation(degrees);
            assert!(
                o.is_y_rotation(),
                "{degrees} degrees is a Y rotation by construction and must be \
                 recognised as one"
            );
            let back = o.as_y_rotation().expect("is_y_rotation said yes");
            let expected = degrees.rem_euclid(360.0);
            // Angles are circular, so the error is the SHORT way round. Comparing
            // plain subtraction would make 0 and 360 look maximally different.
            let raw = back - expected;
            let delta = raw.abs().min((raw.abs() - 360.0).abs());
            assert!(
                delta < 1e-3,
                "{degrees} degrees came back as {back}, expected {expected}, \
                 circular error {delta}"
            );
        }
    }

    #[test]
    fn a_y_rotation_matrix_is_orthonormal_and_right_handed() {
        for degrees in [0.0f32, 17.0, 90.0, 223.0] {
            let o = Orientation::YRotation(degrees);
            assert!(
                o.orthonormality_error() < 1e-6,
                "{} degrees gave error {}",
                degrees,
                o.orthonormality_error()
            );
        }
    }

    #[test]
    fn a_creator_orientation_survives_verbatim_through_the_placement() {
        // The reason Orientation exists: a creator block's <orientation> is a
        // row-major 3x3 and is not a Y rotation in general. A rotation about X by
        // 40 degrees, which no Y rotation can denote.
        let (s, c) = 40.0f32.to_radians().sin_cos();
        let m = matrix([[1.0, 0.0, 0.0], [0.0, c, -s], [0.0, s, c]]);
        let placement = Placement::oriented([1.0, 2.0, 3.0], Orientation::General(m));
        assert_eq!(
            placement.orientation.as_matrix(),
            m,
            "nine numbers, unchanged"
        );
        assert_eq!(placement.rotation_y_degrees(), None);
        assert_eq!(placement.translation, [1.0, 2.0, 3.0]);
    }

    #[test]
    fn a_sheared_orientation_reports_its_error_instead_of_looking_like_a_matrix() {
        // A shear is a valid-looking 3x3 and renders as a wrong transform. The
        // caller gets to see that rather than trusting the shape.
        let sheared = matrix([[1.0, 0.0, 0.0], [0.5, 1.0, 0.0], [0.0, 0.0, 1.0]]);
        let o = Orientation::General(sheared);
        assert!(
            o.orthonormality_error() > 1e-3,
            "a shear must not read as clean"
        );
    }

    #[test]
    fn a_reflection_reports_an_error_because_it_mirrors_rather_than_rotates() {
        // Determinant -1 with orthonormal rows: a mirror, not a rotation.
        let mirrored = matrix([[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, -1.0]]);
        let o = Orientation::General(mirrored);
        assert!(
            (o.orthonormality_error() - 2.0).abs() < 1e-6,
            "det -1 must read as 2.0 away from a rotation, got {}",
            o.orthonormality_error()
        );
    }

    #[test]
    fn a_creator_orientation_decodes_to_the_general_spelling_and_is_never_narrowed() {
        // A creator transform carries a 3x3 by definition, so reading one must never
        // produce YRotation. If it does, something started guessing.
        //
        // Also pinned: Matrix3::default() is ALL ZEROS, not the identity. A
        // BlockTransform built with `..Default::default()` therefore has a
        // non-orthonormal orientation, and orthonormality_error() says so instead of
        // the zero matrix passing as a rotation.
        let transform = spore_creator_core::document::BlockTransform {
            orientation: Matrix3::IDENTITY,
            ..Default::default()
        };
        let o = Orientation::General(transform.orientation);
        assert!(matches!(o, Orientation::General(_)));

        let zero_default = spore_creator_core::document::BlockTransform::default();
        assert!(
            Orientation::error_of(zero_default.orientation) > 1e-3,
            "Matrix3::default() is zeros, so it must read as far from a rotation"
        );
        assert_eq!(
            spore_creator_core::document::Matrix3::default(),
            spore_creator_core::document::Matrix3 {
                row0: [0.0; 3],
                row1: [0.0; 3],
                row2: [0.0; 3],
            },
            "documenting the landmine: Default is not IDENTITY"
        );
    }

    #[test]
    fn rotated_y_and_a_matching_matrix_agree_on_the_denoted_transform() {
        // Two spellings of the same orientation must denote the same matrix, or the
        // enum is leaking a distinction the renderer can observe.
        for degrees in [0.0f32, 30.0, 90.0, -120.0] {
            let a = Placement::rotated_y([0.0; 3], degrees);
            let b = Placement::oriented([0.0; 3], Orientation::General(a.orientation.as_matrix()));
            assert_eq!(a.orientation.as_matrix(), b.orientation.as_matrix());
        }
    }
}
