//! A scene: what is shown, and where.
//!
//! # Multiple assets, not one preset
//!
//! This is the point of the module. The engine used to hold exactly one decoded
//! record, so "show two things" was not expressible. A [`Scene`] is a list of
//! objects, each of which is a single model or a [`Creature`], each with its own
//! [`Placement`], and each placed by a deterministic [`Layout`] computed from
//! the objects' **own measured bounds**.
//!
//! # Layout is computed, not configured
//!
//! Objects are spread along +X by their own widths plus a gap. That is a
//! deliberate choice over a fixed grid: a grid needs a cell size, and the only
//! honest cell size is the largest object, which wastes space and, worse, makes
//! a scene's appearance depend on which *other* assets were loaded. Spacing by
//! measured width keeps each object's position independent of its neighbours,
//! so adding an asset does not move the ones already there — except along X,
//! where order is the layout.
//!
//! Y is lifted so each object's lowest point sits on the floor, from its own
//! bounds. A record with no meshes has no floor to sit on and is reported
//! rather than dropped.

use spore_core::ResourceKey;

use crate::domain::creature::Creature;
use crate::domain::model::ModelInstance;
use crate::domain::{grow, Bounds};

/// One thing on stage.
#[derive(Debug, Clone, PartialEq)]
pub enum SceneObject {
    /// A single placed model.
    Model(ModelInstance),
    /// A creature: one or more placed models presented together.
    Creature(Creature),
}

impl SceneObject {
    /// Every model instance the object places.
    pub fn instances(&self) -> &[ModelInstance] {
        match self {
            Self::Model(instance) => std::slice::from_ref(instance),
            Self::Creature(creature) => &creature.instances,
        }
    }

    /// Every distinct record identity the object needs decoded.
    pub fn records(&self) -> Vec<ResourceKey> {
        let mut keys: Vec<ResourceKey> = self.instances().iter().map(|i| i.key).collect();
        keys.sort();
        keys.dedup();
        keys
    }

    /// The object's kind, for a report.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Model(_) => "model",
            Self::Creature(_) => "creature",
        }
    }

    /// A label, when one was given.
    pub fn label(&self) -> Option<&str> {
        match self {
            Self::Model(instance) => instance.label.as_deref(),
            Self::Creature(creature) => creature.label.as_deref(),
        }
    }
}

/// How objects are spread.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layout {
    /// Along +X, each object offset by its own width plus the gap.
    Row,
    /// On a square grid on the XZ plane, row-major, cells sized by the widest
    /// and deepest object plus the gap.
    Grid,
}

impl Layout {
    /// The canonical spelling, for logs and reports.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Row => "row",
            Self::Grid => "grid",
        }
    }

    /// Parses the spelling, or `None` when it is not one this build has.
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "row" => Some(Self::Row),
            "grid" => Some(Self::Grid),
            _ => None,
        }
    }
}

/// The measured extent of one scene object, used to place its neighbours.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ObjectExtent {
    /// Where the object begins, before placement.
    pub bounds_min: [f32; 3],
    /// Where it ends, before placement.
    pub bounds_max: [f32; 3],
}

impl ObjectExtent {
    /// Width along X.
    pub fn width(&self) -> f32 {
        self.bounds_max[0] - self.bounds_min[0]
    }

    /// Depth along Z.
    pub fn depth(&self) -> f32 {
        self.bounds_max[2] - self.bounds_min[2]
    }
}

/// Everything that is on stage.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Scene {
    /// The objects, in declaration order. The order is the layout order.
    pub objects: Vec<SceneObject>,
}

impl Scene {
    /// An empty scene.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds an object.
    pub fn push(&mut self, object: SceneObject) -> &mut Self {
        self.objects.push(object);
        self
    }

    /// How many objects are on stage.
    pub fn len(&self) -> usize {
        self.objects.len()
    }

    /// Whether the scene is empty.
    pub fn is_empty(&self) -> bool {
        self.objects.is_empty()
    }

    /// Every distinct record the scene needs, ascending.
    pub fn records(&self) -> Vec<ResourceKey> {
        let mut keys: Vec<ResourceKey> = self
            .objects
            .iter()
            .flat_map(|object| object.records())
            .collect();
        keys.sort();
        keys.dedup();
        keys
    }

    /// Every model instance the scene places, in object order.
    pub fn instances(&self) -> Vec<&ModelInstance> {
        self.objects
            .iter()
            .flat_map(|object| object.instances().iter())
            .collect()
    }

    /// Assigns every instance a placement from the measured extents.
    ///
    /// `extents` must be parallel to [`Self::objects`]. An extent of `None` for
    /// an object makes the whole layout unavailable, because a gap cannot be
    /// computed for an object with no width — and inventing one would push the
    /// neighbours somewhere arbitrary.
    pub fn layout(
        &mut self,
        extents: &[Option<ObjectExtent>],
        style: Layout,
        gap: f32,
        floor_y: f32,
    ) -> Option<()> {
        if extents.len() != self.objects.len() {
            return None;
        }
        let measured: Vec<ObjectExtent> = extents.iter().copied().collect::<Option<_>>()?;
        // A row has no cells, so no cell size is needed; a grid's cells are
        // sized by the widest and deepest object so nothing can intersect.
        let cell_depth = match style {
            Layout::Row => 0.0,
            Layout::Grid => measured.iter().map(ObjectExtent::depth).fold(0.0, f32::max),
        };
        let mut cursor_x = 0.0f32;
        let mut cursor_z = 0.0f32;
        let mut row = 0usize;
        for (index, extent) in measured.iter().enumerate() {
            let (column, row_index) = match style {
                Layout::Row => (0usize, 0usize),
                Layout::Grid => {
                    let columns = (self.objects.len() as f32).sqrt().ceil().max(1.0) as usize;
                    (index % columns, index / columns)
                }
            };
            if row_index != row {
                // Rows recede along **-Z**, because that is the direction the
                // default camera looks from (+Z). Rows marching towards the
                // viewer would stack in front of the first row.
                row = row_index;
                cursor_x = 0.0;
                cursor_z -= cell_depth + gap;
            }
            // Lift so the object's lowest point sits on the floor, and shift so
            // its own minimum X lands on the cursor.
            let translation = [
                cursor_x - extent.bounds_min[0],
                floor_y - extent.bounds_min[1],
                cursor_z - extent.bounds_min[2],
            ];
            for instance in self.objects[index].instances_mut() {
                let mut placement = instance.placement;
                placement.translation = [
                    translation[0] + placement.translation[0],
                    translation[1] + placement.translation[1],
                    translation[2] + placement.translation[2],
                ];
                instance.placement = placement;
            }
            cursor_x += extent.width() + gap;
            let _ = column;
        }
        Some(())
    }
}

impl SceneObject {
    /// Every instance, mutably.
    pub fn instances_mut(&mut self) -> &mut [ModelInstance] {
        match self {
            Self::Model(instance) => std::slice::from_mut(instance),
            Self::Creature(creature) => &mut creature.instances,
        }
    }
}

/// The union of a scene's extents, once laid out.
///
/// `None` when no object had an extent: "there were no measurable objects" and
/// "the measurable objects occupy no space" are different, and a camera framed
/// on a zero box sits inside the model.
pub fn scene_bounds(extents: &[Option<ObjectExtent>], placements: &[Option<[f32; 3]>]) -> Bounds {
    let mut bounds: Bounds = None;
    for (extent, placement) in extents.iter().zip(placements) {
        let (Some(extent), Some(placement)) = (extent, placement) else {
            continue;
        };
        bounds = grow(
            bounds,
            Some((
                [
                    extent.bounds_min[0] + placement[0],
                    extent.bounds_min[1] + placement[1],
                    extent.bounds_min[2] + placement[2],
                ],
                [
                    extent.bounds_max[0] + placement[0],
                    extent.bounds_max[1] + placement[1],
                    extent.bounds_max[2] + placement[2],
                ],
            )),
        );
    }
    bounds
}

/// The default gap between objects, in world units.
///
/// Small on purpose: it exists so two objects do not z-fight, not to make a
/// readable grid. Callers override it with `--spacing`.
pub const DEFAULT_GAP: f32 = 1.5;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::model::Placement;

    fn model(group: u32) -> SceneObject {
        SceneObject::Model(ModelInstance::new(
            ResourceKey::new(0x00E6_BCE5, 0x4062_7100, group),
            Placement::default(),
        ))
    }

    fn extent(min_x: f32, max_x: f32, min_y: f32, min_z: f32, max_z: f32) -> ObjectExtent {
        ObjectExtent {
            bounds_min: [min_x, min_y, min_z],
            bounds_max: [max_x, 0.0, max_z],
        }
    }

    #[test]
    fn an_empty_scene_reports_nothing_and_lays_out_to_nothing() {
        let mut scene = Scene::new();
        assert!(scene.is_empty());
        assert!(scene.records().is_empty());
        assert_eq!(scene.layout(&[], Layout::Row, DEFAULT_GAP, 0.0), Some(()));
        assert_eq!(scene_bounds(&[], &[]), None);
    }

    #[test]
    fn records_are_deduplicated_and_ascending() {
        let mut scene = Scene::new();
        scene.push(model(3)).push(model(1)).push(model(3));
        let records = scene.records();
        assert_eq!(records.len(), 2, "two distinct records, three placements");
        assert!(records[0] < records[1]);
    }

    #[test]
    fn a_row_layout_offsets_each_object_by_its_own_width() {
        let mut scene = Scene::new();
        scene.push(model(1)).push(model(2));
        // First object is 2 wide starting at x=0, second is 4 wide starting at 2.
        let extents = [
            Some(extent(0.0, 2.0, -1.0, 0.0, 1.0)),
            Some(extent(0.0, 4.0, -3.0, 0.0, 1.0)),
        ];
        assert_eq!(scene.layout(&extents, Layout::Row, 1.0, 0.0), Some(()));
        let first = scene.objects[0].instances()[0].placement.translation;
        let second = scene.objects[1].instances()[0].placement.translation;
        assert_eq!(first, [0.0, 1.0, 0.0], "lifted onto the floor");
        assert_eq!(second, [3.0, 3.0, 0.0], "2 wide + 1 gap = x 3, lifted by 3");
    }

    #[test]
    fn a_grid_layout_uses_a_square_cell_and_starts_a_new_row() {
        let mut scene = Scene::new();
        for i in 1..=4 {
            scene.push(model(i));
        }
        let extents = [Some(extent(0.0, 2.0, 0.0, 0.0, 2.0)); 4];
        assert_eq!(scene.layout(&extents, Layout::Grid, 1.0, 0.0), Some(()));
        let at = |i: usize| scene.objects[i].instances()[0].placement.translation;
        assert_eq!(at(0), [0.0, 0.0, 0.0]);
        assert_eq!(at(1), [3.0, 0.0, 0.0]);
        assert_eq!(at(2), [0.0, 0.0, -3.0]);
        assert_eq!(at(3), [3.0, 0.0, -3.0]);
    }

    #[test]
    fn a_layout_is_unavailable_when_an_extent_is_missing_rather_than_guessed() {
        let mut scene = Scene::new();
        scene.push(model(1)).push(model(2));
        let extents = [Some(extent(0.0, 2.0, 0.0, 0.0, 1.0)), None];
        assert_eq!(scene.layout(&extents, Layout::Row, 1.0, 0.0), None);
        // And nothing moved.
        assert_eq!(
            scene.objects[0].instances()[0].placement.translation,
            [0.0; 3]
        );
    }

    #[test]
    fn a_layout_is_unavailable_when_the_extent_list_does_not_match() {
        let mut scene = Scene::new();
        scene.push(model(1));
        assert_eq!(scene.layout(&[], Layout::Row, 1.0, 0.0), None);
    }

    #[test]
    fn an_object_without_bounds_is_skipped_by_the_scene_extent_not_zeroed() {
        let extents = [None, Some(extent(0.0, 1.0, 0.0, 0.0, 1.0))];
        let placements = [Some([0.0; 3]), Some([5.0; 3])];
        let (lo, hi) = scene_bounds(&extents, &placements).unwrap();
        // The object with no extent contributes nothing -- not a zero box.
        assert_eq!(lo, [5.0, 5.0, 5.0]);
        assert_eq!(hi, [6.0, 5.0, 6.0]);
        assert_eq!(scene_bounds(&[None], &[Some([0.0; 3])]), None);
    }

    #[test]
    fn a_creature_object_reports_every_instance_and_record() {
        let creature = Creature::single(ModelInstance::new(
            ResourceKey::new(0x00E6_BCE5, 0x4062_7100, 1),
            Placement::default(),
        ))
        .with(ModelInstance::new(
            ResourceKey::new(0x00E6_BCE5, 0x4062_7100, 2),
            Placement::default(),
        ));
        let object = SceneObject::Creature(creature);
        assert_eq!(object.instances().len(), 2);
        assert_eq!(object.records().len(), 2);
        assert_eq!(object.kind(), "creature");
        assert_eq!(object.label(), None);
    }

    #[test]
    fn layout_spellings_are_a_closed_set() {
        assert_eq!(Layout::parse("row"), Some(Layout::Row));
        assert_eq!(Layout::parse("grid"), Some(Layout::Grid));
        assert_eq!(Layout::parse("spiral"), None);
        assert_eq!(Layout::Row.as_str(), "row");
    }
}
