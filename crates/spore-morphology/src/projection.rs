//! Projection: a [`CreatorDocument`] and its [`Composition`] into placed geometry.
//!
//! # What this is, and what it deliberately is not
//!
//! The layer where authoring state becomes something a renderer can draw. It reads
//! [`CreatorDocument`] — the authoring source of truth — together with the
//! [`Composition`] that resolved each block to real part geometry, and produces one
//! [`ProjectedBlock`] per block: authored transform, composed world transform, real
//! geometry, and an explicit resolved-or-not verdict.
//!
//! It is a *projection*, not a scene graph. It owns no ECS entity, holds no
//! `bevy::math::Transform`, and never mutates the document. Everything it produces
//! is derivable again from the same two inputs, which is what makes save/reload
//! verifiable: project, save, reload, project again, compare.
//!
//! [`Orientation`] comes from `spore-core`, which both this crate and `spore-engine`
//! already depend on, so nothing here depends on the engine. Putting it anywhere else
//! would mean one of those two crates depending on the other, or a second orientation
//! type in this crate that would have to be kept in step with the engine's — and two
//! spellings of "a rotation" is exactly the ambiguity [`Orientation`] exists to
//! remove.
//!
//! # The three invariants
//!
//! 1. **One entry per block, always.** A block that did not resolve still appears,
//!    carrying the stage it stopped at and why. There is no path from this module to
//!    a list of only the parts that worked, so a partial creature cannot be mistaken
//!    for a whole one.
//! 2. **The authored orientation is carried, never narrowed.** `local_orientation`
//!    is always `Orientation::General`, because a creator block's `<orientation>` is
//!    a row-major 3x3 that is not a Y rotation in general. `world_orientation` is
//!    likewise `General` once a parent has contributed anything.
//! 3. **Scale stays separate from orientation.** A creator block writes `scale` as
//!    its own field. Folding it into the orientation matrix would destroy the
//!    ability to tell which was which, and the corpus already shows `orientation`
//!    and `triangledirection` disagreeing on 102 of 680 blocks — so this layer is
//!    exactly where an invented relationship would hide.

use spore_core::ResourceKey;
use spore_creator_core::document::{BlockIndex, BlockTransform, CreatorDocument, Matrix3, Vec3};
use spore_creator_core::BlockId;

use spore_core::Orientation;

use crate::composition::{Composition, Counts, PartSlot};
use crate::provenance::{Grade, Stage, StopReason};

/// One block, projected for display.
///
/// Carries the authored values beside the composed ones so a reader can see the
/// parent's contribution instead of having to reconstruct it.
#[derive(Debug, Clone)]
pub struct ProjectedBlock {
    /// Which block in the document this is. `blocks()[i].block == i`.
    pub block: BlockIndex,
    /// The `<blockid>`, when the document carried one.
    pub block_id: Option<BlockId>,
    /// The block's own `<position>`, verbatim.
    pub local_position: Vec3,
    /// The block's own `<orientation>`, verbatim and never narrowed.
    pub local_orientation: Orientation,
    /// The block's own `<scale>`, verbatim.
    pub local_scale: f32,
    /// The parent chain applied to this block's transform.
    pub world_position: Vec3,
    /// The parent chain applied to this block's orientation.
    pub world_orientation: Orientation,
    /// The parent chain applied to this block's scale.
    ///
    /// `None` for a root, which is different from `Some(1.0)`: a root's scale is
    /// its own authored value and no parent multiplied it.
    pub parent: Option<BlockIndex>,
    /// How many ancestors contributed to `world_*`. A root is `0`.
    pub ancestor_depth: usize,
    /// The real geometry, when the block resolved.
    pub part: Option<ProjectedPart>,
    /// Why it did not resolve, when it did not.
    pub unresolved: Option<Unresolved>,
}

impl ProjectedBlock {
    /// Whether this block produced geometry.
    pub fn is_resolved(&self) -> bool {
        self.part.is_some()
    }
}

/// The real geometry behind a projected block.
///
/// Every record the chain went through is named, so "what part is this?" is
/// answerable without re-walking the chain.
#[derive(Debug, Clone)]
pub struct ProjectedPart {
    /// The `<blockid>` the chain started from.
    pub block_id: BlockId,
    /// The `rw4` container half of the part pair.
    pub container_key: ResourceKey,
    /// The `gmsh` geometry half of the pair.
    pub geometry_key: Option<ResourceKey>,
    /// The `prop` record the pointer was read from.
    pub prop_key: Option<ResourceKey>,
    /// The decoded rigblock envelope: the part's authored, orientation-free extent.
    pub rigblock: spore_rigblock::Rigblock,
    /// Vertices in the decoded geometry, or `None` when the record would not decode.
    pub vertices: Option<usize>,
    /// Triangles in the decoded geometry, or `None` when the record would not decode.
    pub triangles: Option<usize>,
    /// Bones in the body graph, or `None` when the container holds none.
    ///
    /// `None` is a real corpus state — 256 of 1824 containers hold no skeleton —
    /// and is not the same as zero bones.
    pub bones: Option<usize>,
    /// How many bones recovered a name.
    pub named_bones: usize,
    /// The decoded geometry, for a caller that will build a mesh.
    pub geometry: Option<spore_rigblock::GmshMesh>,
    /// The body's own graph, when the container holds one.
    pub skeleton: Option<spore_rigblock::Skeleton>,
    /// The weakest grade anywhere on the chain that produced this part.
    ///
    /// `Inferred` here means the geometry is real but at least one hop was followed
    /// rather than witnessed — 485 of 678 parts in the composed corpus.
    pub weakest_grade: Grade,
}

/// Why a block produced no geometry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unresolved {
    /// The furthest stage the chain reached.
    pub stage: Stage,
    /// Why it stopped there.
    pub reason: StopReason,
}

/// A whole document, projected.
#[derive(Debug, Clone)]
pub struct Projection {
    /// The document's `formatversion`, carried unexamined.
    pub document_format_version: Option<u32>,
    blocks: Vec<ProjectedBlock>,
}

impl Projection {
    /// One entry per block, in document order. `blocks()[i].block == i`.
    pub fn blocks(&self) -> &[ProjectedBlock] {
        &self.blocks
    }

    /// The counts, so completeness is one field read away.
    pub fn counts(&self) -> Counts {
        let resolved = self.blocks.iter().filter(|b| b.is_resolved()).count();
        Counts {
            resolved,
            unresolved: self.blocks.len() - resolved,
        }
    }

    /// Every block that produced geometry.
    pub fn resolved(&self) -> impl Iterator<Item = &ProjectedBlock> {
        self.blocks.iter().filter(|b| b.part.is_some())
    }

    /// Every block that did not, with its reason.
    pub fn unresolved(&self) -> impl Iterator<Item = &ProjectedBlock> {
        self.blocks.iter().filter(|b| b.part.is_none())
    }

    /// One block by index.
    pub fn block(&self, index: BlockIndex) -> Option<&ProjectedBlock> {
        self.blocks.get(index)
    }

    /// Vertices across every resolved block.
    pub fn total_vertices(&self) -> usize {
        self.resolved()
            .filter_map(|b| b.part.as_ref())
            .filter_map(|p| p.vertices)
            .sum()
    }

    /// Triangles across every resolved block.
    pub fn total_triangles(&self) -> usize {
        self.resolved()
            .filter_map(|b| b.part.as_ref())
            .filter_map(|p| p.triangles)
            .sum()
    }

    /// Bones across every resolved block.
    pub fn total_bones(&self) -> usize {
        self.resolved()
            .filter_map(|b| b.part.as_ref())
            .filter_map(|p| p.bones)
            .sum()
    }

    /// The deepest authored chain in this document.
    pub fn max_depth(&self) -> usize {
        self.blocks
            .iter()
            .map(|b| b.ancestor_depth)
            .max()
            .unwrap_or(0)
    }

    /// How many resolved parts rest on an `Inferred` hop.
    pub fn parts_with_an_inferred_hop(&self) -> usize {
        self.resolved()
            .filter(|b| {
                b.part
                    .as_ref()
                    .is_some_and(|p| p.weakest_grade == Grade::Inferred)
            })
            .count()
    }

    /// The axis-aligned extent of the projected creature, or `None` when nothing
    /// resolved.
    ///
    /// Built from each part's **decoded geometry**, placed at the block's world
    /// position. NOT from the rigblock envelope, which was the obvious choice and is
    /// `None` is a real answer. "No geometry" is not "geometry at the origin", and a
    /// creature that failed to compose must not be framed as though it succeeded.
    ///
    /// # Why geometry and not the rigblock
    ///
    /// A rigblock is a **placement-allowance box** -- it marks where a part may be
    /// put -- not the part's extent, so it is the wrong input for framing what is
    /// drawn. That is the whole reason this reads `geometry.positions`.
    ///
    /// An earlier version of this comment justified the change with a specific and
    /// **false** claim: that a `k == 0` rigblock is "a 36-byte header with no cells,
    /// so its envelope is identically zero and contributes a degenerate point". A
    /// review measured it against the record this comment cited
    /// (`0x2b978c46:0x40626200:0x05e3df13`) and found:
    ///
    /// ```text
    /// block 0 rigblock  levels = 0
    ///                  min_high = [ 0.01655769,  0.057751544, 0.00017166184]
    ///                  max_low  = [-0.01655769, -0.057751544, -0.037069116]
    ///                  cells    = 0
    /// ```
    ///
    /// So there is no zero envelope and no degenerate point: `Rigblock::envelope()`
    /// returns `None` when there are no cells, and the *header* floats that the old
    /// `bounds()` actually read carry real values. `cc20c3a`'s commit message says
    /// exactly this ("I assumed ... an all-zero envelope. It does not"); the comment
    /// it shipped contradicted its own message.
    ///
    /// There is also no single ratio between the two, so none is quoted here. Four
    /// readings of "how big is the rigblock" disagree, and at the median axis the
    /// geometry can be **ten times smaller** than the rigblock rather than larger.
    /// `catalog::RigblockBounds` carries the readings by name; see
    /// `tests/catalog_corpus.rs` for the census and `work/experiments/LEDGER.md` for
    /// the correction of the "median 2.01x" figure that used to stand here.
    ///
    ///
    pub fn bounds(&self) -> Option<Bounds> {
        let mut out: Option<Bounds> = None;
        for block in self.resolved() {
            let Some(part) = block.part.as_ref() else {
                continue;
            };
            // `continue`, not `?`. A `?` here would abandon the ENTIRE bounds
            // because one part lacked geometry, reporting a creature as
            // unframeable when in fact every other part was fine. One part with no
            // geometry must not erase the others' extent.
            let Some(geometry) = part.geometry.as_ref() else {
                continue;
            };
            let mut local: Option<Bounds> = None;
            for position in geometry.positions.iter() {
                let point = [
                    block.world_position.x + position[0],
                    block.world_position.y + position[1],
                    block.world_position.z + position[2],
                ];
                local = Some(match local {
                    None => Bounds {
                        min: point,
                        max: point,
                    },
                    Some(previous) => previous.union(Bounds {
                        min: point,
                        max: point,
                    }),
                });
            }
            if let Some(local) = local {
                out = Some(match out {
                    None => local,
                    Some(previous) => previous.union(local),
                });
            }
        }
        out
    }
}

/// An axis-aligned extent.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bounds {
    /// The low corner.
    pub min: [f32; 3],
    /// The high corner.
    pub max: [f32; 3],
}

impl Bounds {
    /// The smallest extent containing both.
    pub fn union(self, other: Self) -> Self {
        let mut min = [0.0f32; 3];
        let mut max = [0.0f32; 3];
        for i in 0..3 {
            min[i] = self.min[i].min(other.min[i]);
            max[i] = self.max[i].max(other.max[i]);
        }
        Self { min, max }
    }

    /// The centre.
    pub fn center(&self) -> [f32; 3] {
        [
            (self.min[0] + self.max[0]) * 0.5,
            (self.min[1] + self.max[1]) * 0.5,
            (self.min[2] + self.max[2]) * 0.5,
        ]
    }

    /// The largest half-extent, for framing a camera.
    pub fn radius(&self) -> f32 {
        let c = self.center();
        let d = [self.max[0] - c[0], self.max[1] - c[1], self.max[2] - c[2]];
        (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
    }
}

/// Builds the projection of `document` given the `composition` resolved from it.
///
/// The two must correspond, and the correspondence is checked structurally rather
/// than assumed: the slot count must equal the block count, and slot `i` must name
/// block `i`. Zipping a mismatched pair would attach a part from one creature to a
/// transform from another, which renders as a plausible creature rather than as an
/// error.
pub fn project(
    document: &CreatorDocument,
    composition: &Composition,
) -> Result<Projection, ProjectionError> {
    if document.blocks.len() != composition.slots().len() {
        return Err(ProjectionError::BlockCountMismatch {
            document: document.blocks.len(),
            composition: composition.slots().len(),
        });
    }
    for (slot_index, slot) in composition.slots().iter().enumerate() {
        let names = match slot {
            PartSlot::Resolved(part) => part.block(),
            PartSlot::Unresolved(unresolved) => unresolved.block,
        };
        if names != slot_index {
            return Err(ProjectionError::SlotOrderMismatch {
                slot: slot_index,
                names,
            });
        }
    }

    // Recursive rather than iterative because the authored forest is 19 deep at its
    // maximum, which is comfortably inside the stack, and the recursion makes the
    // one thing that must be right obvious: a child's world transform composes its
    // parent's WORLD transform, not the parent's local one. Getting that wrong
    // compounds silently and the outermost part is the only one that looks wrong.
    fn descend(
        document: &CreatorDocument,
        index: BlockIndex,
        parent_world: &(Vec3, Orientation, f32),
        depth: usize,
        out: &mut [Option<ProjectedBlock>],
    ) {
        let Some(block) = document.blocks.get(index) else {
            return;
        };
        let transform: &BlockTransform = &block.transform;

        // `parent * local`, row-major. Scale multiplies separately because a
        // creator block's scale is its own field.
        let world_orientation = multiply(parent_world.1, transform.orientation);
        let world_position = add(
            parent_world.0,
            rotate(parent_world.1, transform.position, parent_world.2),
        );
        let world_scale = parent_world.2 * transform.scale;

        // The depth is the one PASSED IN, never recomputed here. The previous version
        // derived it from `out[..index]` -- the maximum over EARLIER BLOCKS, which are
        // siblings and unrelated to this block's ancestry. It reported a maximum depth
        // of 18 for a forest the corpus measures at 19, because the deepest block was
        // preceded by a shallower sibling. A second version compared the parent's
        // world transform against the identity to guess root-ness, which is a third
        // wrong answer: a root with a non-identity orientation is not a root.
        let parent = document.parent_of(index);

        out[index] = Some(ProjectedBlock {
            block: index,
            block_id: block.block_id,
            local_position: transform.position,
            local_orientation: Orientation::General(transform.orientation),
            local_scale: transform.scale,
            world_position,
            world_orientation,
            parent,
            ancestor_depth: depth,
            part: None,
            unresolved: None,
        });

        let carried = (world_position, world_orientation, world_scale);
        for child in block.child_indices() {
            descend(document, *child, &carried, depth + 1, out);
        }
    }

    let identity = (Vec3::default(), Orientation::default(), 1.0f32);
    let mut slots: Vec<Option<ProjectedBlock>> = vec![None; document.blocks.len()];
    for root in document.roots() {
        descend(document, root, &identity, 0, &mut slots);
    }
    // A block unreachable from any root -- which only a cyclic document produces,
    // since every block in a forest is reachable from some root -- still gets an
    // entry, computed as if it were a root. The one-slot-per-block invariant is the
    // point of this module, so a graph defect must not silently shorten the list.
    for index in 0..document.blocks.len() {
        if slots[index].is_some() {
            continue;
        }
        descend(document, index, &identity, 0, &mut slots);
        if let Some(filled) = slots[index].as_mut() {
            // It was reached with an identity parent, so it has no authored parent to
            // report. Saying "no parent" is the truth for a block nothing points at.
            filled.parent = None;
        }
    }

    // Attach the resolution half, which needs the composition rather than the
    // document's geometry.
    for (index, projected) in slots.iter_mut().enumerate() {
        let Some(projected) = projected.as_mut() else {
            continue;
        };
        match &composition.slots()[index] {
            PartSlot::Resolved(resolved) => {
                let part = resolved.as_ref();
                projected.part = Some(ProjectedPart {
                    block_id: part.block_id,
                    container_key: part.container_key,
                    geometry_key: part.geometry_key,
                    prop_key: part.prop_key,
                    rigblock: part.rigblock.clone(),
                    vertices: part.vertex_count(),
                    triangles: part.triangle_count(),
                    bones: part.bones().map(<[spore_rigblock::Bone]>::len),
                    named_bones: part.named_bones().len(),
                    geometry: part.geometry.clone(),
                    skeleton: part.skeleton.clone(),
                    weakest_grade: part.trail.weakest_grade(),
                });
                projected.unresolved = None;
            }
            PartSlot::Unresolved(unresolved) => {
                projected.part = None;
                projected.unresolved = Some(Unresolved {
                    stage: unresolved.stage,
                    reason: unresolved.reason,
                });
            }
        }
    }

    // The recursive fill cannot leave a hole for a reachable block, so this is a
    // belt-and-braces check rather than a path that should be taken.
    let blocks = slots
        .into_iter()
        .enumerate()
        .map(|(index, slot)| slot.ok_or(ProjectionError::UnreachableBlock { block: index }))
        .collect::<Result<Vec<_>, _>>()?;

    Ok(Projection {
        document_format_version: composition.document_format_version,
        blocks,
    })
}

/// `a * b` for row-major 3×3 matrices.
///
/// Row-major means `row[i][j]` multiplies `basis[j]`, so composing a child's local
/// matrix onto a parent's world matrix is `world * local` under exactly this
/// multiplication. Written out rather than delegated to a matrix library so that the
/// transpose a rotation needs cannot happen by accident — and getting it backwards
/// mirrors a part instead of rotating it, which looks like a modelling decision.
fn multiply(parent: Orientation, local: Matrix3) -> Orientation {
    let [a00, a01, a02, a10, a11, a12, a20, a21, a22] = parent.as_matrix().to_array();
    let [b00, b01, b02, b10, b11, b12, b20, b21, b22] = local.to_array();
    Orientation::General(Matrix3 {
        row0: [
            a00 * b00 + a01 * b10 + a02 * b20,
            a00 * b01 + a01 * b11 + a02 * b21,
            a00 * b02 + a01 * b12 + a02 * b22,
        ],
        row1: [
            a10 * b00 + a11 * b10 + a12 * b20,
            a10 * b01 + a11 * b11 + a12 * b21,
            a10 * b02 + a11 * b12 + a12 * b22,
        ],
        row2: [
            a20 * b00 + a21 * b10 + a22 * b20,
            a20 * b01 + a21 * b11 + a22 * b21,
            a20 * b02 + a21 * b12 + a22 * b22,
        ],
    })
}

fn add(a: Vec3, b: Vec3) -> Vec3 {
    Vec3 {
        x: a.x + b.x,
        y: a.y + b.y,
        z: a.z + b.z,
    }
}

/// Places `point` under `orientation` and `scale`.
///
/// `S * R * p`, in that order. Scale is applied innermost because it is the parent's
/// scale acting on the child's own offset — a child offset of 1 under a parent
/// scaled by 2 sits 2 units out, which is what `S * R * p` gives and what `R * S * p`
/// would not.
fn rotate(orientation: Orientation, point: Vec3, scale: f32) -> Vec3 {
    let m = orientation.as_matrix();
    let p = [point.x * scale, point.y * scale, point.z * scale];
    // Row i of the result is row i of the matrix dotted with the point. Written as
    // an explicit dot per row rather than an index loop, because the transpose is
    // exactly the mistake here: `out[i] = sum_k m[i][k]*p[k]` is the correct row-major
    // form, and swapping the two indices mirrors a part instead of rotating it.
    let rows = [m.row0, m.row1, m.row2];
    let mut out = [0.0f32; 3];
    for (i, row) in rows.iter().enumerate() {
        out[i] = row[0] * p[0] + row[1] * p[1] + row[2] * p[2];
    }
    Vec3 {
        x: out[0],
        y: out[1],
        z: out[2],
    }
}

/// Why a document and a composition could not be projected together.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectionError {
    /// The document and the composition disagree about how many blocks there are.
    BlockCountMismatch {
        /// Blocks in the document.
        document: usize,
        /// Slots in the composition.
        composition: usize,
    },
    /// A composition slot does not name the block at the same index.
    SlotOrderMismatch {
        /// The slot's position.
        slot: usize,
        /// The block that slot names.
        names: usize,
    },
    /// A block that the document's own graph could not reach.
    ///
    /// Only a cyclic document produces this, since every block in a forest is
    /// reachable from some root. It is an error rather than a skipped block because
    /// "one slot per block" is the invariant this module exists to hold.
    UnreachableBlock {
        /// The block that could not be reached.
        block: BlockIndex,
    },
}

impl core::fmt::Display for ProjectionError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::BlockCountMismatch {
                document,
                composition,
            } => write!(
                f,
                "document has {document} blocks but composition has {composition} slots; \
                 these are not the same document"
            ),
            Self::SlotOrderMismatch { slot, names } => write!(
                f,
                "composition slot {slot} names block {names}; slots must be in document \
                 order for a positional projection to mean anything"
            ),
            Self::UnreachableBlock { block } => write!(
                f,
                "block {block} is unreachable from any root, so it has no world \
                 transform; the document's graph contains a cycle"
            ),
        }
    }
}

impl std::error::Error for ProjectionError {}
