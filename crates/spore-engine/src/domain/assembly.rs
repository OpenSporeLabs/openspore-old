//! A procedural, modular assembly of primitives — the renderer smoke test.
//!
//! # What this is, and what it is emphatically not
//!
//! This is **placeholder content**, not Spore behaviour. It exists so that
//! "the window opened and a 3D scene rendered" can be demonstrated with no game
//! install and no proprietary asset, and so that a failure later can be
//! attributed to the asset path rather than to the renderer. Every number below
//! is an OpenSpore invention; none of it is derived from the original game, and
//! nothing here should be read as a specification of what a creature *is*.
//!
//! # Why it lives in `domain` and not in `scene`
//!
//! The module is deliberately **Bevy-free**. It is a value: an [`Assembly`] can
//! be built, placed, measured and asserted on with no window, no GPU and no
//! asset server. [`crate::scene`] is the only place that turns one into meshes,
//! materials and entities. That is the same split the rule at the top of
//! [`crate::domain`] describes: the geometry's *meaning and placement* are
//! ordinary Rust, and the renderer merely consumes them.
//!
//! # Modular, not one mesh
//!
//! An assembly is a list of [`Module`]s, each a primitive with a local placement
//! relative to its parent. World placements are computed **hierarchically** and
//! returned flat, so a module can be moved by moving its parent — the property
//! that makes this an assembly rather than a pile of shapes. Modules must be
//! listed parent-before-child (a topological order); a forward reference or an
//! out-of-range parent is a named error rather than a silently wrong transform.
//!
//! # Conventions, stated once
//!
//! * Rotations are Euler angles in **degrees**, applied `Rz · Ry · Rx` to a
//!   column vector (so `rx` is applied first). The composed world rotation is a
//!   row-major [`Matrix3`], the same spelling the rest of the workspace uses for
//!   orientations, so [`crate::loader::orientation_to_quat`] can consume it
//!   unchanged.
//! * Scale is **uniform**. Non-uniform parent scale would turn composition into a
//!   shear, and a placeholder has no need for that trap.
//! * A primitive's local axis conventions match Bevy's, because the primitives
//!   are Bevy's: a capsule and a cylinder stand along +Y.

use spore_core::{Matrix3, Orientation};

use crate::domain::model::Placement;
use crate::domain::Bounds;

/// A primitive shape, sized in its own local space.
///
/// Bounds are the primitive's **local axis-aligned box**, which is what
/// [`Assembly::bounds`] transforms. For a sphere it is exact; for a capsule and a
/// cylinder it is the enclosing box, which is deliberately generous: a camera
/// framed on a slightly-too-large box sees everything, which is the failure mode
/// worth preferring.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Primitive {
    /// A sphere of `radius`.
    Sphere { radius: f32 },
    /// A capsule of `radius` and cylindrical `length`, standing along +Y.
    Capsule { radius: f32, length: f32 },
    /// A cylinder of `radius` and `height`, standing along +Y.
    Cylinder { radius: f32, height: f32 },
    /// An axis-aligned box with the given half-extents.
    Cuboid { half_extents: [f32; 3] },
}

impl Primitive {
    /// The half-extents of the primitive's local axis-aligned box.
    pub fn half_extents(&self) -> [f32; 3] {
        match *self {
            Self::Sphere { radius } => [radius, radius, radius],
            Self::Capsule { radius, length } => [radius, length * 0.5 + radius, radius],
            Self::Cylinder { radius, height } => [radius, height * 0.5, radius],
            Self::Cuboid { half_extents } => half_extents,
        }
    }
}

/// What a module is *for*, so the renderer can choose a colour without the domain
/// having an opinion about paint.
///
/// The domain owns the meaning (this is a head, that is a limb); the renderer
/// owns the appearance. Inverting that — storing an RGB here — would make the
/// domain carry presentation, which is exactly what the boundary rule forbids.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModuleKind {
    /// The trunk.
    Body,
    /// The head.
    Head,
    /// A forward protrusion of the head.
    Snout,
    /// An eye.
    Eye,
    /// A leg or arm.
    Limb,
    /// A rear appendage.
    Tail,
}

/// One module: a primitive with a local placement relative to its parent.
///
/// The name is a debugging label only. It is deliberately not an identity: this
/// is placeholder content, so there is nothing to persist or reference. A future
/// data-driven assembly would carry a stable id here; see
/// `docs/CONTENT-MODEL.md`.
#[derive(Debug, Clone, PartialEq)]
pub struct Module {
    /// A short debugging label (`"body"`, `"leg_fl"`).
    pub name: &'static str,
    /// What it is for.
    pub kind: ModuleKind,
    /// Its shape.
    pub primitive: Primitive,
    /// The index of its parent, or `None` for a root. Must be `< this module's
    /// own index` — see [`AssemblyError`].
    pub parent: Option<usize>,
    /// Local translation relative to the parent.
    pub translation: [f32; 3],
    /// Local orientation, Euler degrees, `Rz · Ry · Rx`.
    pub rotation_deg: [f32; 3],
    /// Uniform scale.
    pub scale: f32,
}

/// Why an [`Assembly`] could not be placed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AssemblyError {
    /// A module names a parent index at or past the end of the list.
    ParentOutOfRange { module: usize, parent: usize },
    /// A module names a parent that appears **at or after** it. Because parents
    /// must be resolved first, this also makes cycles unrepresentable rather
    /// than something the placement pass has to detect.
    ForwardReference { module: usize, parent: usize },
    /// A module's scale is zero, negative or not finite. A zero scale collapses
    /// geometry to nothing and a negative one mirrors it; both render as a
    /// plausible-looking scene, so they are refused by value.
    InvalidScale { module: usize, scale: f32 },
}

impl std::fmt::Display for AssemblyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ParentOutOfRange { module, parent } => {
                write!(f, "module {module} names parent {parent}, out of range")
            }
            Self::ForwardReference { module, parent } => write!(
                f,
                "module {module} names parent {parent}, which is not placed before it"
            ),
            Self::InvalidScale { module, scale } => {
                write!(
                    f,
                    "module {module} has scale {scale}, which is not positive"
                )
            }
        }
    }
}

impl std::error::Error for AssemblyError {}

/// A list of modules in topological order (every parent precedes its children).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Assembly {
    /// The modules, parent-before-child.
    pub modules: Vec<Module>,
}

impl Assembly {
    /// An empty assembly.
    pub fn new() -> Self {
        Self::default()
    }

    /// A module with no rotation and unit scale, appended and returning its
    /// index.
    ///
    /// The builder panics on a forward reference rather than carrying a
    /// `Result` through every call site: a preset written in code is a
    /// programming error if malformed, and [`Assembly::placements`] still
    /// validates the finished list for callers that build one from data.
    pub fn push(&mut self, module: Module) -> usize {
        let index = self.modules.len();
        if let Some(parent) = module.parent {
            assert!(
                parent < index,
                "module {index} ({}) names parent {parent}, which is not before it",
                module.name
            );
        }
        self.modules.push(module);
        index
    }

    /// How many modules the assembly holds.
    pub fn len(&self) -> usize {
        self.modules.len()
    }

    /// Whether the assembly holds no modules.
    pub fn is_empty(&self) -> bool {
        self.modules.is_empty()
    }

    /// World placements, one per module, in module order.
    ///
    /// A child's translation is rotated and scaled by its parent before being
    /// added, so moving or scaling a parent moves the whole subtree — which is
    /// what makes this an assembly.
    pub fn placements(&self) -> Result<Vec<Placement>, AssemblyError> {
        let count = self.modules.len();
        let mut out: Vec<Placement> = Vec::with_capacity(count);
        for (index, module) in self.modules.iter().enumerate() {
            if !(module.scale.is_finite() && module.scale > 0.0) {
                return Err(AssemblyError::InvalidScale {
                    module: index,
                    scale: module.scale,
                });
            }
            let local_rotation = rotation_matrix_deg(module.rotation_deg);
            let placement = match module.parent {
                None => Placement {
                    translation: module.translation,
                    orientation: Orientation::General(local_rotation),
                    scale: module.scale,
                },
                Some(parent) => {
                    if parent >= count {
                        return Err(AssemblyError::ParentOutOfRange {
                            module: index,
                            parent,
                        });
                    }
                    if parent >= index {
                        return Err(AssemblyError::ForwardReference {
                            module: index,
                            parent,
                        });
                    }
                    let placed = out[parent];
                    let parent_rotation = placed.orientation.as_matrix();
                    let scaled = scale_vec(module.translation, placed.scale);
                    let offset = transform(parent_rotation, scaled);
                    Placement {
                        translation: add(placed.translation, offset),
                        orientation: Orientation::General(mul(parent_rotation, local_rotation)),
                        scale: placed.scale * module.scale,
                    }
                }
            };
            out.push(placement);
        }
        Ok(out)
    }

    /// The world axis-aligned box enclosing every placed module.
    ///
    /// `None` for an empty assembly is an absence, not a zero box: a camera
    /// framed on `[0,0,0]-[0,0,0]` sits inside the model. See
    /// [`crate::domain::Bounds`].
    pub fn bounds(&self) -> Result<Bounds, AssemblyError> {
        let placements = self.placements()?;
        Ok(self.bounds_of(&placements))
    }

    /// The world box enclosing `placements`, which must be the output of
    /// [`Assembly::placements`] for `self`.
    ///
    /// Split from [`Assembly::bounds`] so the engine can compute placements once
    /// and reuse them rather than placing twice and hoping the two agree.
    pub fn bounds_of(&self, placements: &[Placement]) -> Bounds {
        let mut bounds: Bounds = None;
        for (module, placed) in self.modules.iter().zip(placements) {
            let half = module.primitive.half_extents();
            let rotation = placed.orientation.as_matrix();
            for &sx in &[-1.0f32, 1.0] {
                for &sy in &[-1.0f32, 1.0] {
                    for &sz in &[-1.0f32, 1.0] {
                        let corner = [half[0] * sx, half[1] * sy, half[2] * sz];
                        let scaled = scale_vec(corner, placed.scale);
                        let world = add(placed.translation, transform(rotation, scaled));
                        bounds = match bounds {
                            None => Some((world, world)),
                            Some((lo, hi)) => Some((
                                [
                                    lo[0].min(world[0]),
                                    lo[1].min(world[1]),
                                    lo[2].min(world[2]),
                                ],
                                [
                                    hi[0].max(world[0]),
                                    hi[1].max(world[1]),
                                    hi[2].max(world[2]),
                                ],
                            )),
                        };
                    }
                }
            }
        }
        bounds
    }
}

/// A small, creature-like assembly built from primitives.
///
/// Ten modules: a box trunk, a head with a snout and two eyes, a tail and four
/// legs. The head is tilted and the snout and eyes are its children, so the
/// preset deliberately exercises **non-identity parent composition** rather than
/// only the trivial root case.
///
/// This is placeholder content. See the module doc.
pub fn quadruped() -> Assembly {
    let mut assembly = Assembly::new();

    let body = assembly.push(Module {
        name: "body",
        kind: ModuleKind::Body,
        primitive: Primitive::Cuboid {
            half_extents: [1.4, 0.7, 0.7],
        },
        parent: None,
        translation: [0.0, 1.5, 0.0],
        rotation_deg: [0.0, 0.0, 0.0],
        scale: 1.0,
    });

    let head = assembly.push(Module {
        name: "head",
        kind: ModuleKind::Head,
        primitive: Primitive::Sphere { radius: 0.6 },
        parent: Some(body),
        translation: [1.5, 0.55, 0.0],
        rotation_deg: [0.0, 0.0, -12.0],
        scale: 1.0,
    });

    assembly.push(Module {
        name: "snout",
        kind: ModuleKind::Snout,
        primitive: Primitive::Capsule {
            radius: 0.22,
            length: 0.5,
        },
        parent: Some(head),
        translation: [0.55, -0.15, 0.0],
        rotation_deg: [0.0, 0.0, -90.0],
        scale: 1.0,
    });

    assembly.push(Module {
        name: "eye_l",
        kind: ModuleKind::Eye,
        primitive: Primitive::Sphere { radius: 0.13 },
        parent: Some(head),
        translation: [0.3, 0.3, 0.32],
        rotation_deg: [0.0, 0.0, 0.0],
        scale: 1.0,
    });

    assembly.push(Module {
        name: "eye_r",
        kind: ModuleKind::Eye,
        primitive: Primitive::Sphere { radius: 0.13 },
        parent: Some(head),
        translation: [0.3, 0.3, -0.32],
        rotation_deg: [0.0, 0.0, 0.0],
        scale: 1.0,
    });

    assembly.push(Module {
        name: "tail",
        kind: ModuleKind::Tail,
        primitive: Primitive::Capsule {
            radius: 0.16,
            length: 1.2,
        },
        parent: Some(body),
        translation: [-1.5, 0.4, 0.0],
        rotation_deg: [0.0, 0.0, 70.0],
        scale: 1.0,
    });

    // Four legs at the body's underside corners. `splay` is the small outward
    // tilt; the sign follows the corner so the stance widens toward the ground.
    for (name, x, z) in [
        ("leg_fl", 0.9f32, 0.45f32),
        ("leg_fr", 0.9, -0.45),
        ("leg_bl", -0.9, 0.45),
        ("leg_br", -0.9, -0.45),
    ] {
        let splay = if x >= 0.0 { -8.0 } else { 8.0 };
        assembly.push(Module {
            name,
            kind: ModuleKind::Limb,
            primitive: Primitive::Capsule {
                radius: 0.22,
                length: 0.36,
            },
            parent: Some(body),
            translation: [x, -1.1, z],
            rotation_deg: [0.0, 0.0, splay],
            scale: 1.0,
        });
    }

    assembly
}

/// The row-major rotation for Euler `degrees` in the order `Rz · Ry · Rx`.
fn rotation_matrix_deg(degrees: [f32; 3]) -> Matrix3 {
    let [rx, ry, rz] = [
        degrees[0].to_radians(),
        degrees[1].to_radians(),
        degrees[2].to_radians(),
    ];
    let (sx, cx) = rx.sin_cos();
    let (sy, cy) = ry.sin_cos();
    let (sz, cz) = rz.sin_cos();
    let x = Matrix3 {
        row0: [1.0, 0.0, 0.0],
        row1: [0.0, cx, -sx],
        row2: [0.0, sx, cx],
    };
    let y = Matrix3 {
        row0: [cy, 0.0, sy],
        row1: [0.0, 1.0, 0.0],
        row2: [-sy, 0.0, cy],
    };
    let z = Matrix3 {
        row0: [cz, -sz, 0.0],
        row1: [sz, cz, 0.0],
        row2: [0.0, 0.0, 1.0],
    };
    mul(z, mul(y, x))
}

/// Row-major matrix product `a · b`.
fn mul(a: Matrix3, b: Matrix3) -> Matrix3 {
    let rows = [a.row0, a.row1, a.row2];
    let mut out = [[0.0f32; 3]; 3];
    for (i, row) in rows.iter().enumerate() {
        for (j, cell) in out[i].iter_mut().enumerate() {
            *cell = row[0] * b.row0[j] + row[1] * b.row1[j] + row[2] * b.row2[j];
        }
    }
    Matrix3 {
        row0: out[0],
        row1: out[1],
        row2: out[2],
    }
}

/// `m · v` for a column vector.
fn transform(m: Matrix3, v: [f32; 3]) -> [f32; 3] {
    [
        m.row0[0] * v[0] + m.row0[1] * v[1] + m.row0[2] * v[2],
        m.row1[0] * v[0] + m.row1[1] * v[1] + m.row1[2] * v[2],
        m.row2[0] * v[0] + m.row2[1] * v[1] + m.row2[2] * v[2],
    ]
}

fn add(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn scale_vec(v: [f32; 3], s: f32) -> [f32; 3] {
    [v[0] * s, v[1] * s, v[2] * s]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn module(parent: Option<usize>, translation: [f32; 3]) -> Module {
        Module {
            name: "m",
            kind: ModuleKind::Limb,
            primitive: Primitive::Sphere { radius: 0.5 },
            parent,
            translation,
            rotation_deg: [0.0, 0.0, 0.0],
            scale: 1.0,
        }
    }

    #[test]
    fn an_empty_assembly_has_no_bounds_rather_than_a_zero_box() {
        let assembly = Assembly::new();
        assert!(assembly.is_empty());
        assert_eq!(assembly.bounds().unwrap(), None);
    }

    #[test]
    fn an_unrotated_child_is_offset_by_the_parent_and_nothing_else() {
        let mut assembly = Assembly::new();
        let root = assembly.push(module(None, [1.0, 2.0, 3.0]));
        assembly.push(module(Some(root), [1.0, 0.0, 0.0]));
        let placed = assembly.placements().unwrap();
        assert_eq!(placed[1].translation, [2.0, 2.0, 3.0]);
    }

    #[test]
    fn a_child_offset_is_rotated_by_the_parent() {
        // A parent rotated +90 degrees about Z takes a local +X offset to +Y.
        let mut assembly = Assembly::new();
        let mut root = module(None, [0.0, 0.0, 0.0]);
        root.rotation_deg = [0.0, 0.0, 90.0];
        let root = assembly.push(root);
        assembly.push(module(Some(root), [1.0, 0.0, 0.0]));
        let placed = assembly.placements().unwrap();
        let [x, y, z] = placed[1].translation;
        assert!((x - 0.0).abs() < 1e-5, "x = {x}");
        assert!((y - 1.0).abs() < 1e-5, "y = {y}");
        assert!((z - 0.0).abs() < 1e-5, "z = {z}");
    }

    #[test]
    fn a_parent_scale_scales_the_child_offset_and_the_child_itself() {
        let mut assembly = Assembly::new();
        let mut root = module(None, [0.0, 0.0, 0.0]);
        root.scale = 2.0;
        let root = assembly.push(root);
        assembly.push(module(Some(root), [1.0, 0.0, 0.0]));
        let placed = assembly.placements().unwrap();
        assert_eq!(placed[1].translation, [2.0, 0.0, 0.0]);
        assert_eq!(placed[1].scale, 2.0);
    }

    #[test]
    fn a_forward_reference_is_refused_by_value() {
        let mut assembly = Assembly::new();
        assembly.push(module(None, [0.0; 3]));
        // Hand-build a bad list: module 1 claims a parent at index 1 (itself).
        assembly.modules.push(module(Some(1), [0.0; 3]));
        assert_eq!(
            assembly.placements(),
            Err(AssemblyError::ForwardReference {
                module: 1,
                parent: 1
            })
        );
    }

    #[test]
    fn a_non_positive_scale_is_refused_rather_than_mirroring() {
        let mut assembly = Assembly::new();
        let mut root = module(None, [0.0; 3]);
        root.scale = -1.0;
        assembly.push(root);
        assert_eq!(
            assembly.placements(),
            Err(AssemblyError::InvalidScale {
                module: 0,
                scale: -1.0
            })
        );
    }

    #[test]
    fn the_rotation_is_a_proper_rotation_not_a_shear() {
        let m = rotation_matrix_deg([12.0, -30.0, 47.0]);
        let error = Orientation::error_of(m);
        assert!(error < 1e-5, "orthonormality error {error}");
    }

    #[test]
    fn the_quadruped_places_every_module_and_sits_on_the_floor() {
        let assembly = quadruped();
        assert_eq!(assembly.len(), 10);
        let placed = assembly.placements().unwrap();
        assert_eq!(placed.len(), assembly.len());
        for (module, placement) in assembly.modules.iter().zip(&placed) {
            assert!(
                placement.translation.iter().all(|v| v.is_finite()),
                "{} is not finite",
                module.name
            );
        }
        let (lo, hi) = assembly.bounds().unwrap().unwrap();
        assert!(
            lo[1] > -0.5 && lo[1] < 0.2,
            "feet near the floor, lo.y = {}",
            lo[1]
        );
        assert!(hi[1] > 2.0, "something stands tall, hi.y = {}", hi[1]);
        assert!(hi[0] - lo[0] > 3.0, "it is longer than it is wide");
    }

    #[test]
    fn the_quadruped_is_deterministic() {
        assert_eq!(
            quadruped().placements().unwrap(),
            quadruped().placements().unwrap()
        );
    }

    #[test]
    fn the_tilted_head_carries_its_children() {
        // The snout is a child of a head tilted -12 degrees about Z. Its world
        // orientation is therefore not the identity, which is the whole point of
        // parenting it rather than placing it directly.
        let assembly = quadruped();
        let placed = assembly.placements().unwrap();
        let head = assembly
            .modules
            .iter()
            .position(|m| m.name == "head")
            .unwrap();
        let snout = assembly
            .modules
            .iter()
            .position(|m| m.name == "snout")
            .unwrap();
        assert_eq!(assembly.modules[snout].parent, Some(head));
        assert!(
            !placed[snout].orientation.is_y_rotation(),
            "the snout should inherit the head's tilt, not be flat"
        );
    }
}
