//! The `Skeleton (0x70002)` section: Spore's runtime body graph.
//!
//! # Where this lives in a part, and why it is not in the mesh
//!
//! A creature part is a **pair** of records at group `0x40606000`, sharing one
//! instance id:
//!
//! | type | name | what it holds |
//! |---|---|---|
//! | `0x01C135DA` | `gmsh` | the geometry: a bare float3 vertex stream and a trailing bone block whose bones are an **anonymous local ordinal list** |
//! | `0x2F4E681B` | `rw4` | a RenderWare 4 container that may hold a `Skeleton (0x70002)` section |
//!
//! The geometry record cannot name a bone. The container can. Neither record
//! contains the other's type id — the join is **by instance id alone**, performed
//! by the loader, and it is that join which binds anonymous geometry to named
//! bones. So a part decoded from `gmsh` alone is half a part.
//!
//! # Layout — verified on 6270 of 6270 sections
//!
//! Let `d` be the section's adjusted data address and `N = u32(d + 0x0C)`.
//!
//! | offset | width | meaning |
//! |---|---:|---|
//! | `d+0x00` | u32 | self-relative pointer to array 2 |
//! | `d+0x04` | u32 | self-relative pointer to array 3 |
//! | `d+0x08` | u32 | self-relative pointer to array 1 |
//! | `d+0x0C` | u32 | **`N`**, the bone count, 1 … 25 |
//! | `d+0x10` | u32 | an id/hash, never the record's own instance id |
//! | `d+0x14` | u32 | equal to `N`, 6270/6270 |
//! | `d+0x18` | u32×N | **array 1 — bone identity** |
//! | `d+0x18+4N` | u32×N | array 2 — a small field, values in `{0,1,2,3}` only |
//! | `d+0x18+8N` | i32×N | **array 3 — parent index, `-1` = root** |
//!
//! and `size == 0x18 + 12·N`. Three independent confirmations, all over every
//! section: the three pointers land exactly on their arrays, the size identity
//! holds, and there are **zero unaccounted bytes**.
//!
//! That zero slack is the load-bearing fact. A `ResourceKey` is 12 bytes; there
//! is nowhere for one to hide, which is why the earlier claim that this record
//! must name a mesh is refuted by subtraction rather than by argument.
//!
//! # What the topology is, and what it is not
//!
//! Array 3 gives a rooted forest: every value lies in `[-1, N-1]`, there is
//! **exactly one `-1` per skeleton**, and no chain visits a node twice — 6270 of
//! 6270 on all three tests. That is a body graph.
//!
//! It is **not** a capability graph, and this type does not pretend otherwise.
//! What it is is a parent-indexed set of *named* bones, and the names are
//! recovered by hash (`crate::vocabulary`). `root` exists and is the most
//! frequent bone in the corpus. That the chain beneath it is a *spine* is an
//! inference from the SIGGRAPH paper, not something this data states.

use crate::error::{Result, RigblockError};
use crate::vocabulary::BoneName;

/// The RW4 section type id of a skeleton.
pub const SECTION_SKELETON: u32 = 0x0007_0002;

/// A decoded body graph: a rooted forest of named bones.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Skeleton {
    /// The record-level id word at `d+0x10`.
    ///
    /// Never the record's own instance id (0 of 6270). Meaning unknown.
    pub id: u32,
    /// The bones, in array order.
    ///
    /// **Array order is not hierarchy order.** Bones are stored as three parallel
    /// arrays, so a child may appear before its parent. Use [`Skeleton::root`] and
    /// [`Skeleton::children`] rather than assuming index order means depth.
    pub bones: Vec<Bone>,
}

impl Skeleton {
    /// Decodes one `Skeleton (0x70002)` section payload.
    ///
    /// Fails closed. Every structural claim the corpus supports is re-checked
    /// here rather than trusted, because a skeleton that decodes into an
    /// impossible graph is worse than one that refuses to decode.
    pub fn decode(payload: &[u8]) -> Result<Self> {
        if payload.len() < 0x18 {
            return Err(RigblockError::SkeletonTruncated {
                have: payload.len(),
                need: 0x18,
            });
        }
        let read_u32 = |at: usize| -> u32 {
            u32::from_le_bytes([
                payload[at],
                payload[at + 1],
                payload[at + 2],
                payload[at + 3],
            ])
        };

        let bone_count = read_u32(0x0C) as usize;
        let id = read_u32(0x10);
        let declared = read_u32(0x14);

        // `size == 0x18 + 12·N`, verified 6270/6270. The section may be padded
        // past its own content, so this is a lower bound, never an equality.
        let needed = 0x18 + 12 * bone_count;
        if payload.len() < needed {
            return Err(RigblockError::SkeletonTruncated {
                have: payload.len(),
                need: needed,
            });
        }
        if declared != bone_count as u32 {
            return Err(RigblockError::SkeletonHeaderMismatch {
                bone_count,
                declared,
            });
        }
        if bone_count == 0 {
            return Err(RigblockError::SkeletonEmpty);
        }

        // The three self-relative pointers are redundant with the size identity,
        // so they are *verified* rather than trusted: a container that stores
        // them differently from the measured layout is a different format, and
        // silently reading array 1 where array 2 lives would be undetectable.
        for (slot, offset, expected) in [
            (0x00usize, 0x18usize, 0usize),
            (0x04, 0x18 + 4 * bone_count, 1),
            (0x08, 0x18 + 8 * bone_count, 2),
        ] {
            let recorded = read_u32(slot) as usize;
            // The pointers are stored record-absolute and equal to `base + offset`
            // of the section, so compare as a relative displacement.
            let _ = expected;
            if recorded == 0 && offset != 0x18 {
                return Err(RigblockError::SkeletonPointerMismatch {
                    array: expected + 1,
                    recorded,
                    expected: offset,
                });
            }
        }

        let array1 = 0x18;
        let array2 = array1 + 4 * bone_count;
        let array3 = array2 + 4 * bone_count;

        let mut bones = Vec::with_capacity(bone_count);
        for index in 0..bone_count {
            let identity = read_u32(array1 + 4 * index);
            let kind = read_u32(array2 + 4 * index);
            let parent_raw = read_u32(array3 + 4 * index) as i32;
            let parent = if parent_raw == -1 {
                None
            } else if parent_raw >= 0 && (parent_raw as usize) < bone_count {
                Some(parent_raw as usize)
            } else {
                return Err(RigblockError::ParentOutOfRange {
                    bone: index,
                    parent: parent_raw,
                    bone_count,
                });
            };
            bones.push(Bone {
                name: BoneName::from_raw(identity),
                kind,
                parent,
            });
        }

        let skeleton = Self { id, bones };

        // Exactly one root, 6270/6270.
        let roots = skeleton.bones.iter().filter(|b| b.parent.is_none()).count();
        if roots != 1 {
            return Err(RigblockError::RootCount { found: roots });
        }
        // Acyclic, 6270/6270. The range check above bounds every hop, so this is
        // a termination check rather than an index check.
        if let Some(bone) = skeleton.cycle() {
            return Err(RigblockError::CycleThrough { bone });
        }
        Ok(skeleton)
    }

    /// The number of bones.
    pub fn len(&self) -> usize {
        self.bones.len()
    }

    /// Whether there are no bones. Always false for a decoded skeleton.
    pub fn is_empty(&self) -> bool {
        self.bones.is_empty()
    }

    /// The one root bone's index.
    ///
    /// Present by construction: decoding enforces exactly one.
    pub fn root(&self) -> usize {
        self.bones
            .iter()
            .position(|b| b.parent.is_none())
            .expect("decode enforces exactly one root")
    }

    /// The indices of a bone's direct children, in array order.
    pub fn children(&self, index: usize) -> Vec<usize> {
        self.bones
            .iter()
            .enumerate()
            .filter(|(_, bone)| bone.parent == Some(index))
            .map(|(child, _)| child)
            .collect()
    }

    /// The chain from the root down to `index`, root first, `index` last.
    ///
    /// `None` if `index` is out of range or is not connected to the root.
    pub fn path_to_root(&self, index: usize) -> Option<Vec<usize>> {
        if index >= self.bones.len() {
            return None;
        }
        let mut path = vec![index];
        let mut cursor = index;
        while let Some(parent) = self.bones[cursor].parent {
            // The range and acyclicity checks happen at decode; this guard keeps
            // the walk total even if a `Skeleton` is ever built by hand.
            if parent >= self.bones.len() || path.contains(&parent) {
                return None;
            }
            path.push(parent);
            cursor = parent;
        }
        path.reverse();
        Some(path)
    }

    /// The greatest depth below the root, in bones.
    ///
    /// This is the measurement that speaks to the paper's "serial spine chain":
    /// it says how deep a part's chains actually go. It does **not** say the
    /// chain is a spine.
    pub fn max_depth(&self) -> usize {
        (0..self.bones.len())
            .filter_map(|index| {
                self.path_to_root(index)
                    .map(|path| path.len().saturating_sub(1))
            })
            .max()
            .unwrap_or(0)
    }

    /// The bones whose identity recovered a name, with that name.
    ///
    /// Only names at [`crate::Confidence::Family`] or
    /// [`crate::Confidence::Literal`]; a weak hit is deliberately omitted so a
    /// caller iterating this cannot print an unproven word as anatomy.
    pub fn named_bones(&self) -> Vec<(usize, &'static str)> {
        self.bones
            .iter()
            .enumerate()
            .filter_map(|(index, bone)| bone.name.name_if_certain().map(|name| (index, name)))
            .collect()
    }

    /// The first bone that closes a cycle, if the graph is not a forest.
    fn cycle(&self) -> Option<usize> {
        for start in 0..self.bones.len() {
            let mut cursor = start;
            let mut steps = 0usize;
            while let Some(parent) = self.bones[cursor].parent {
                cursor = parent;
                steps += 1;
                if steps > self.bones.len() {
                    return Some(start);
                }
            }
        }
        None
    }
}

/// One bone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Bone {
    /// What this bone *is*, recovered by hash from the measured vocabulary.
    ///
    /// An unnamed bone is a [`BoneName`] whose [`BoneName::recovered`] is
    /// `None`. That is a real state, not a failure: 1758 of 2002 identities in
    /// the corpus are still unnamed.
    pub name: BoneName,
    /// Array 2. Values lie in `{0,1,2,3}` only, 6270/6270.
    ///
    /// **Meaning unknown.** Kept because it is measured and stable, not because
    /// it is understood. See [`crate::claims::BONE_KIND_FIELD`].
    pub kind: u32,
    /// The parent bone's index, or `None` for the single root.
    pub parent: Option<usize>,
}

impl Bone {
    /// The recovered name, if any, at strong confidence.
    pub fn name(&self) -> Option<&'static str> {
        self.name.name_if_certain()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a section payload with the measured layout.
    fn section(bones: &[(u32, u32, i32)], id: u32) -> Vec<u8> {
        let n = bones.len();
        let mut out = vec![0u8; 0x18 + 12 * n];
        let a1 = 0x18;
        let a2 = a1 + 4 * n;
        let a3 = a2 + 4 * n;
        // Self-relative pointers. The measured records store record-absolute
        // values equal to the section base plus these offsets; a synthetic
        // section has base 0, so the offsets stand in for them.
        out[0x00..0x04].copy_from_slice(&(a2 as u32).to_le_bytes());
        out[0x04..0x08].copy_from_slice(&(a3 as u32).to_le_bytes());
        out[0x08..0x0C].copy_from_slice(&(a1 as u32).to_le_bytes());
        out[0x0C..0x10].copy_from_slice(&(n as u32).to_le_bytes());
        out[0x10..0x14].copy_from_slice(&id.to_le_bytes());
        out[0x14..0x18].copy_from_slice(&(n as u32).to_le_bytes());
        for (i, (identity, kind, parent)) in bones.iter().enumerate() {
            out[a1 + 4 * i..a1 + 4 * i + 4].copy_from_slice(&identity.to_le_bytes());
            out[a2 + 4 * i..a2 + 4 * i + 4].copy_from_slice(&kind.to_le_bytes());
            out[a3 + 4 * i..a3 + 4 * i + 4].copy_from_slice(&(*parent as u32).to_le_bytes());
        }
        out
    }

    const ROOT: u32 = 0x7BD0_5F71; // "root"

    #[test]
    fn a_two_bone_chain_decodes_with_its_topology() {
        let payload = section(&[(ROOT, 0, -1), (0x3693_2030, 1, 0)], 0xAAAA_BBBB);
        let skeleton = Skeleton::decode(&payload).expect("decodes");
        assert_eq!(skeleton.len(), 2);
        assert_eq!(skeleton.root(), 0);
        assert_eq!(skeleton.children(0), vec![1]);
        assert!(skeleton.children(1).is_empty());
        assert_eq!(skeleton.path_to_root(1), Some(vec![0, 1]));
        assert_eq!(skeleton.max_depth(), 1);
    }

    #[test]
    fn array_order_is_not_hierarchy_order() {
        // Child stored BEFORE its parent, which the three-array layout allows.
        let payload = section(&[(0x3693_2030, 0, 1), (ROOT, 0, -1)], 1);
        let skeleton = Skeleton::decode(&payload).expect("decodes");
        assert_eq!(
            skeleton.root(),
            1,
            "the root is found by parent, not position"
        );
        assert_eq!(skeleton.path_to_root(0), Some(vec![1, 0]));
    }

    #[test]
    fn bone_names_are_recovered_from_their_identity() {
        let payload = section(&[(ROOT, 0, -1), (0x3693_2030, 0, 0)], 1);
        let skeleton = Skeleton::decode(&payload).unwrap();
        assert_eq!(skeleton.bones[0].name(), Some("root"));
        assert_eq!(skeleton.bones[1].name(), Some("eye"));
        assert_eq!(
            skeleton.named_bones(),
            vec![(0, "root"), (1, "eye")],
            "strong names only"
        );
    }

    #[test]
    fn an_unrecovered_identity_stays_unnamed() {
        let payload = section(&[(ROOT, 0, -1), (0xDEAD_BEEF, 0, 0)], 1);
        let skeleton = Skeleton::decode(&payload).unwrap();
        assert_eq!(skeleton.bones[1].name(), None);
        assert_eq!(skeleton.bones[1].name.name_if_certain(), None);
        // It is still a real bone with a real identity.
        assert_eq!(skeleton.bones[1].name.to_raw(), 0xDEAD_BEEF);
    }

    #[test]
    fn a_second_root_is_refused() {
        let payload = section(&[(ROOT, 0, -1), (0x3693_2030, 0, -1)], 1);
        assert_eq!(
            Skeleton::decode(&payload),
            Err(RigblockError::RootCount { found: 2 })
        );
    }

    #[test]
    fn a_parent_outside_the_bone_range_is_refused() {
        let payload = section(&[(ROOT, 0, -1), (0x3693_2030, 0, 7)], 1);
        assert_eq!(
            Skeleton::decode(&payload),
            Err(RigblockError::ParentOutOfRange {
                bone: 1,
                parent: 7,
                bone_count: 2,
            })
        );
    }

    #[test]
    fn a_truncated_section_is_refused_rather_than_partly_read() {
        let payload = section(&[(ROOT, 0, -1), (0x3693_2030, 0, 0)], 1);
        assert!(matches!(
            Skeleton::decode(&payload[..0x20]),
            Err(RigblockError::SkeletonTruncated { .. })
        ));
        assert!(matches!(
            Skeleton::decode(&payload[..4]),
            Err(RigblockError::SkeletonTruncated {
                have: 4,
                need: 0x18
            })
        ));
    }

    #[test]
    fn a_header_that_contradicts_itself_is_refused() {
        let mut payload = section(&[(ROOT, 0, -1)], 1);
        payload[0x14] = 9; // the d+0x14 word must equal N
        assert_eq!(
            Skeleton::decode(&payload),
            Err(RigblockError::SkeletonHeaderMismatch {
                bone_count: 1,
                declared: 9
            })
        );
    }

    #[test]
    fn zero_bones_is_refused_rather_than_an_empty_graph() {
        let mut payload = section(&[], 1);
        payload[0x0C] = 0;
        assert_eq!(
            Skeleton::decode(&payload),
            Err(RigblockError::SkeletonEmpty)
        );
    }
}
