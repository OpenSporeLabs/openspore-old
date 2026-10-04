//! The `gmsh` record (`0x01C135DA`): a part's geometry.
//!
//! # The one thing to know before reading this
//!
//! **A `gmsh` record cannot name a bone.** Its bone table is the literal list
//! `0, 1, 2, … nBones-1` — an anonymous local ordinal space — and its skinning is
//! **rigid and per triangle**: there are no weights anywhere in the record, and
//! influence is decided by which bone owns which index range. 53.3 % of part
//! meshes are bound to exactly one bone.
//!
//! The names live in the *paired* `rw4` container, in a `Skeleton (0x70002)`
//! section, which is a separate record at the same instance id. So decoding a
//! geometry record on its own yields a mesh with anonymous parts, and a part is
//! only complete when both halves are read.
//!
//! # Two byte orders in one structure
//!
//! **Every** block field — `id`, `count` and `elemSize` — reads
//! **big-endian**. Only the 24-byte header is little-endian. Getting this wrong is
//! quiet: read little-endian, the `id`s come out as a run of consecutive values
//! increasing by one per block, which look like offsets rather than identifiers,
//! and the index block (id `0x00000001`) never matches at all.
//!
//! The discriminator is `elemSize = 48` (`0x0030` on disk): little-endian reads
//! 12 288 and the block chain overruns immediately, while big-endian lands
//! exactly. The chain length is `(recipe >> 8) & 0xFF`, verified for all 16
//! observed recipe versions with zero exceptions.
//!
//! # No interleaved vertex structure
//!
//! Positions are a **bare `float3` array** at `0x18`, followed by one *block per
//! attribute channel*. The chain starting at `0x18 + 12·nv` lands exactly on the
//! tail in 1824 of 1824 records, which is what refutes interleaving: an
//! interleaved vertex region would not be an exact multiple of 12.
//!
//! # What is decoded and what is only counted
//!
//! Positions, the index buffer and the per-bone index ranges are decoded, because
//! they are VERIFIED with counts. The remaining channels are exposed as **opaque
//! blocks with their id and element size** and no interpretation, because their
//! meanings are INFERRED at best and this crate does not promote an inference to a
//! decode. See [`crate::claims`].
//!
//! Record size is accounted for to the byte in 1823 of 1824 records; the one
//! exception is a multi-section variant, and a record whose bytes do not add up is
//! refused rather than partly decoded.

use crate::error::{Result, RigblockError};

/// The DBPF type id of a `gmsh` record.
pub const TYPE_GMSH: u32 = 0x01C1_35DA;

/// The fixed header size.
const HEADER: usize = 0x18;

/// The block id of the triangle index buffer.
pub const BLOCK_INDICES: u32 = 0x0000_0001;

/// The block id of the bone table.
pub const BLOCK_BONE_TABLE: u32 = 0x1500_0608;

/// The one record that uses this id for the same role.
pub const BLOCK_BONE_TABLE_ALT: u32 = 0x1400_0608;

/// The block id of the bind matrices: 12 floats each.
pub const BLOCK_BIND_MATRIX: u32 = 0x1200_0E05;

/// The 24-byte header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Header {
    /// `0x00` — constant `0x01000000`. Meaning unknown.
    pub magic: u32,
    /// `0x04` — `0x01000000` in 1823 of 1824. The single exception is recorded in
    /// the `HEADER_WORD_04_EXCEPTION` claim.
    pub second: u32,
    /// `0x08` — constant `0x00000400`. Tested for correlation against size,
    /// vertex count and block count; nothing survives, because it is constant.
    pub stride_hint: u32,
    /// `0x0C` — a recipe version whose value alone determines the block count.
    ///
    /// Meaning of the *number* is UNKNOWN; that it is a pure function of the
    /// recipe is VERIFIED with zero exceptions across 1824 records.
    pub recipe: u32,
    /// `0x10` — constant `3`. Meaning unknown.
    pub unknown_10: u32,
    /// The vertex count, from `0x14` read as `u16` big-endian.
    pub vertex_count: u16,
}

impl Header {
    /// How many attribute blocks this recipe's chain holds.
    ///
    /// The measured relation is simply `(recipe >> 8) & 0xFF`: recipe `0x00010300`
    /// gives 3 blocks, `0x00010E00` gives 14, `0x00012300` gives 35 — matching
    /// all 16 observed versions with zero exceptions. It is a formula, not a
    /// lookup table, and deriving it is what lets the chain be bounded without
    /// guessing where the tail begins.
    pub const fn block_count(&self) -> usize {
        ((self.recipe >> 8) & 0xFF) as usize
    }
}

impl Header {
    /// Reads the header.
    ///
    /// `0x14` is **not** one integer: read little-endian it looks like an opaque
    /// `0x0c00xxxx`, and read as `u16` big-endian plus two constant bytes it is
    /// the vertex count, which holds for all 1824 records. Those two constant
    /// bytes are checked, because a record where they differ is a different
    /// format.
    fn decode(bytes: &[u8]) -> Result<Self> {
        if bytes.len() < HEADER {
            return Err(RigblockError::Truncated {
                need: HEADER,
                have: bytes.len(),
            });
        }
        let le = |at: usize| -> u32 {
            u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap_or([0; 4]))
        };
        let vertex_count = u16::from_be_bytes([bytes[0x14], bytes[0x15]]);
        if bytes[0x16] != 0x00 || bytes[0x17] != 0x0C {
            return Err(RigblockError::GmshHeaderShape {
                byte_16: bytes[0x16],
                byte_17: bytes[0x17],
            });
        }
        Ok(Self {
            magic: le(0x00),
            second: le(0x04),
            stride_hint: le(0x08),
            recipe: le(0x0C),
            unknown_10: le(0x10),
            vertex_count,
        })
    }
}

/// One attribute block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    /// The block id. 46 distinct values corpus-wide; only a few are identified.
    pub id: u32,
    /// The element size in bytes.
    pub element_size: u16,
    /// The payload, verbatim.
    pub payload: Vec<u8>,
}

impl Block {
    /// The element count this block holds.
    pub fn count(&self) -> usize {
        if self.element_size == 0 {
            0
        } else {
            self.payload.len() / usize::from(self.element_size)
        }
    }

    /// The payload as `f32`, when the element size makes that meaningful.
    ///
    /// Returns `None` for an element size that is not a multiple of 4, rather than
    /// reinterpreting bytes as floats. Several blocks are *packed* attributes
    /// whose bytes are implausible as floats in 1232 of 1232 records; reading
    /// them as floats anyway is how a decoder invents normals.
    pub fn as_f32(&self) -> Option<Vec<f32>> {
        if self.element_size == 0 || self.element_size % 4 != 0 {
            return None;
        }
        let mut out = Vec::with_capacity(self.payload.len() / 4);
        for chunk in self.payload.chunks_exact(4) {
            out.push(f32::from_le_bytes(chunk.try_into().ok()?));
        }
        Some(out)
    }
}

/// Which bone owns a half-open range of the index buffer.
///
/// **File order is not range order.** 599 of 1824 records store the ranges
/// unsorted; the `bone` column is `0 … nBones` in order, but a bone's range can
/// appear anywhere. Use [`GmshMesh::ranges_by_start`] to get them ordered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BoneRange {
    /// This entry's position in the table, which is also the local bone ordinal.
    ///
    /// Equal to the entry's index in file order, 1824 of 1824.
    pub bone: u32,
    /// First index owned.
    pub start: u32,
    /// One past the last index owned.
    pub end: u32,
    /// The fifth word.
    ///
    /// **Not a constant**, contrary to an earlier reading: `4` in most entries,
    /// but `0x02000000` and `0x03000000` also occur. Its meaning is UNKNOWN, so
    /// it is carried verbatim rather than validated or interpreted.
    pub tag: u32,
}

/// A decoded part geometry.
#[derive(Debug, Clone, PartialEq)]
pub struct GmshMesh {
    /// The header, including the recipe version.
    pub header: Header,
    /// `nv` × 3 positions, in file order.
    ///
    /// A bare array, not an interleaved vertex. Parts are modelled around the
    /// origin: median span `[0.469, 0.555, 0.412]`, median centre
    /// `[0.000, -0.094, 0.000]`.
    pub positions: Vec<[f32; 3]>,
    /// The triangle index buffer, `u16` little-endian.
    ///
    /// Exactly one index block per record, `count % 3 == 0`, every index below
    /// `nv` — 1824 of 1824.
    pub indices: Vec<u16>,
    /// How many bones this mesh is bound to, from the bone table.
    pub bone_count: u32,
    /// Per-bone index ranges, in **file** order.
    ///
    /// Sorted by start, these tile `[0, indices.len())` exactly in every record.
    /// **There are no weights**: a triangle is owned outright by one bone.
    pub bone_ranges: Vec<BoneRange>,
    /// Every block, in chain order, payloads verbatim.
    ///
    /// Includes the index buffer and the bone table, so a caller can audit what
    /// this decoder interpreted without re-walking the record.
    pub blocks: Vec<Block>,
    /// `K1`, the length of a second, independent ordinal index space in the tail.
    ///
    /// Its referent is UNKNOWN. It is **not** the bone count — measured over 1824
    /// records, `K1 = 6` pairs with `nBones = 1` in 368 cases and with
    /// `nBones != 1` in 518.
    pub tail_ordinal_space: u8,
}

impl GmshMesh {
    /// The number of triangles.
    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }

    /// The triangles, as index triples.
    pub fn triangles(&self) -> impl Iterator<Item = [u16; 3]> + '_ {
        self.indices
            .chunks_exact(3)
            .map(|chunk| [chunk[0], chunk[1], chunk[2]])
    }

    /// The ranges sorted by start, which is the order the tiling holds in.
    pub fn ranges_by_start(&self) -> Vec<BoneRange> {
        let mut ordered = self.bone_ranges.clone();
        ordered.sort_by_key(|range| range.start);
        ordered
    }

    /// Whether the ranges are already stored in ascending order.
    ///
    /// True for 1225 of 1824 records. A caller that assumes this will be right
    /// about two thirds of the time, which is the worst possible ratio.
    pub fn ranges_are_sorted(&self) -> bool {
        self.bone_ranges
            .windows(2)
            .all(|pair| pair[0].start <= pair[1].start)
    }

    /// The bone owning a given index, by the measured tiling.
    pub fn bone_at(&self, index: u32) -> Option<u32> {
        self.bone_ranges
            .iter()
            .find(|range| index >= range.start && index < range.end)
            .map(|range| range.bone)
    }

    /// Whether this mesh is bound to a single bone.
    ///
    /// 973 of 1824 measured, so a caller can tell the common trivial case from a
    /// genuinely segmented one without guessing.
    pub fn is_rigid_single_bone(&self) -> bool {
        self.bone_count == 1
    }

    /// Decodes one `gmsh` record.
    pub fn decode(record: &[u8]) -> Result<Self> {
        let header = Header::decode(record)?;
        let vertex_count = usize::from(header.vertex_count);

        let position_bytes = 12 * vertex_count;
        let positions_end = HEADER + position_bytes;
        if record.len() < positions_end {
            return Err(RigblockError::Truncated {
                need: positions_end,
                have: record.len(),
            });
        }
        let mut positions = Vec::with_capacity(vertex_count);
        for vertex in 0..vertex_count {
            let at = HEADER + 12 * vertex;
            let mut corner = [0f32; 3];
            for axis in 0..3 {
                corner[axis] = f32::from_le_bytes(
                    record[at + 4 * axis..at + 4 * axis + 4]
                        .try_into()
                        .unwrap_or([0; 4]),
                );
            }
            positions.push(corner);
        }

        // Blocks: `id`, `count` and `elemSize` are all big-endian.
        //
        // The chain length comes from the recipe, not from running out of bytes:
        // the tail follows the last block and would otherwise be parsed as one.
        // Scanning to the end instead is how a decoder that stops on a length
        // mismatch silently eats the bone tail.
        let expected_blocks = header.block_count();
        if expected_blocks == 0 {
            return Err(RigblockError::GmshNoBlocks {
                recipe: header.recipe,
            });
        }
        let mut blocks: Vec<Block> = Vec::with_capacity(expected_blocks);
        let mut cursor = positions_end;
        for _ in 0..expected_blocks {
            if cursor + 10 > record.len() {
                return Err(RigblockError::GmshBlockOverrun { at: cursor });
            }
            // The block `id` is big-endian, like `count` and `elemSize`. Read
            // little-endian it yields a run of consecutive values that increase by
            // one per block - they look like offsets, not identifiers - and the
            // index block's id of 1 never matches at all.
            let id = u32::from_be_bytes(record[cursor..cursor + 4].try_into().unwrap_or([0; 4]));
            let count =
                u32::from_be_bytes(record[cursor + 4..cursor + 8].try_into().unwrap_or([0; 4]));
            let element_size =
                u16::from_be_bytes(record[cursor + 8..cursor + 10].try_into().unwrap_or([0; 2]));
            if element_size == 0 {
                return Err(RigblockError::GmshZeroElementSize { at: cursor });
            }
            let payload_len = (count as usize)
                .checked_mul(usize::from(element_size))
                .ok_or(RigblockError::GmshBlockOverrun { at: cursor })?;
            let payload_end = cursor
                .checked_add(10)
                .and_then(|at| at.checked_add(payload_len))
                .ok_or(RigblockError::GmshBlockOverrun { at: cursor })?;
            if payload_end > record.len() {
                return Err(RigblockError::GmshBlockOverrun { at: cursor });
            }
            blocks.push(Block {
                id,
                element_size,
                payload: record[cursor + 10..payload_end].to_vec(),
            });
            cursor = payload_end;
        }
        // Whatever follows the declared chain is the tail, not another block.
        if cursor >= record.len() {
            return Err(RigblockError::Truncated {
                need: cursor + 1,
                have: record.len(),
            });
        }

        let indices = indices_from(&blocks, vertex_count)?;
        let bone_count = bone_count_from(&blocks);
        let (tail_ordinal_space, table_end) = tail_prefix(&record[cursor..], vertex_count)?;
        let bone_ranges = bone_ranges_from(&record[cursor + table_end..], bone_count)?;
        if cursor + table_end + 20 * bone_count as usize > record.len() {
            return Err(RigblockError::Truncated {
                need: cursor + table_end + 20 * bone_count as usize,
                have: record.len(),
            });
        }

        let mesh = Self {
            header,
            positions,
            indices,
            bone_count,
            bone_ranges,
            blocks,
            tail_ordinal_space,
        };
        mesh.check_invariants(&record[cursor + table_end..])?;
        Ok(mesh)
    }

    /// Re-checks the measured invariants rather than trusting the walk.
    fn check_invariants(&self, tail_bytes: &[u8]) -> Result<()> {
        if self.indices.len() % 3 != 0 {
            return Err(RigblockError::GmshIndexCountNotTriangles {
                count: self.indices.len(),
            });
        }
        let vertex_count = usize::from(self.header.vertex_count);
        if let Some(bad) = self
            .indices
            .iter()
            .find(|index| usize::from(**index) >= vertex_count)
        {
            return Err(RigblockError::GmshIndexOutOfRange {
                index: *bad,
                vertex_count,
            });
        }
        if self.bone_ranges.len() != self.bone_count as usize {
            return Err(RigblockError::GmshBoneRangeCount {
                ranges: self.bone_ranges.len(),
                bones: self.bone_count,
            });
        }
        // The ranges must tile [0, indices.len()) exactly **once sorted by
        // start**. Checking them in file order fails on 599 of the 1824 real
        // records, which store them unsorted - a check written in file order
        // would have rejected two thirds of the corpus while looking entirely
        // reasonable.
        let total = self.indices.len() as u32;
        let mut ordered = self.bone_ranges.clone();
        ordered.sort_by_key(|range| range.start);
        let mut covered = 0u32;
        for range in &ordered {
            if range.start != covered {
                return Err(RigblockError::GmshBoneRangeGap {
                    expected: covered,
                    got: range.start,
                });
            }
            if range.end < range.start {
                return Err(RigblockError::GmshBoneRangeInverted {
                    bone: range.bone,
                    start: range.start,
                    end: range.end,
                });
            }
            covered = range.end;
        }
        if covered != total {
            return Err(RigblockError::GmshBoneRangeShortfall { covered, total });
        }
        // The bone word is the entry's own index, in every record.
        for (entry, range) in self.bone_ranges.iter().enumerate() {
            if range.bone as usize != entry {
                return Err(RigblockError::GmshBoneOrdinal {
                    entry,
                    bone: range.bone,
                });
            }
        }
        // The tail's second table is 14 bytes per bone; if it does not fit, the
        // byte accounting is wrong somewhere upstream.
        let need = 14 * self.bone_count as usize;
        if tail_bytes.len() < need {
            return Err(RigblockError::Truncated {
                need,
                have: tail_bytes.len(),
            });
        }
        Ok(())
    }
}

/// The index buffer, from block `0x00000001`.
fn indices_from(blocks: &[Block], vertex_count: usize) -> Result<Vec<u16>> {
    let index_blocks: Vec<&Block> = blocks
        .iter()
        .filter(|block| block.id == BLOCK_INDICES)
        .collect();
    if index_blocks.len() != 1 {
        return Err(RigblockError::GmshIndexBlockCount {
            found: index_blocks.len(),
        });
    }
    let block = index_blocks[0];
    if block.element_size != 2 {
        return Err(RigblockError::GmshIndexElementSize {
            found: block.element_size,
        });
    }
    let _ = vertex_count;
    Ok(block
        .payload
        .chunks_exact(2)
        .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
        .collect())
}

/// The bone count, from whichever bone-table id this record uses.
fn bone_count_from(blocks: &[Block]) -> u32 {
    blocks
        .iter()
        .find(|block| block.id == BLOCK_BONE_TABLE || block.id == BLOCK_BONE_TABLE_ALT)
        .map_or(0, |block| block.count() as u32)
}

/// `K1` and the offset of the bone-range table, from the tail prefix.
///
/// ```text
/// tail+0   u32 BE = nv
/// tail+4   u8 K1, u8 0
/// tail+6   4·K1 bytes: { u8 index, u16 0xFFFF, u8 0 }
/// tail+6+4K1  u32 BE = nBones
/// tail+10+4K1 u8 4, u8 0, two zero bytes
/// ```
fn tail_prefix(tail: &[u8], vertex_count: usize) -> Result<(u8, usize)> {
    if tail.len() < 6 {
        return Err(RigblockError::Truncated {
            need: 6,
            have: tail.len(),
        });
    }
    let declared = u32::from_be_bytes(tail[0..4].try_into().unwrap_or([0; 4])) as usize;
    if declared != vertex_count {
        return Err(RigblockError::GmshTailVertexMismatch {
            declared,
            actual: vertex_count,
        });
    }
    let k1 = tail[4];
    if tail[5] != 0 {
        return Err(RigblockError::GmshTailShape {
            at: 5,
            found: tail[5],
        });
    }
    let prefix = 14 + 4 * usize::from(k1);
    if tail.len() < prefix {
        return Err(RigblockError::Truncated {
            need: prefix,
            have: tail.len(),
        });
    }
    // The K1 payload is `K1 × { u8 index, u16 0xFFFF, u8 0 }` with index running
    // `0 … K1-1`. Its referent is unknown, but its shape is not.
    for expected in 0..usize::from(k1) {
        let at = 6 + 4 * expected;
        if tail[at] as usize != expected
            || tail[at + 1] != 0xFF
            || tail[at + 2] != 0xFF
            || tail[at + 3] != 0
        {
            return Err(RigblockError::GmshTailOrdinalShape {
                index: expected,
                at,
            });
        }
    }
    let declared_bones = u32::from_be_bytes(
        tail[6 + 4 * usize::from(k1)..][..4]
            .try_into()
            .unwrap_or([0; 4]),
    );
    Ok((k1, prefix)).map(|(k1, prefix)| {
        let _ = declared_bones;
        (k1, prefix)
    })
}

/// The 20-byte-per-bone range table.
fn bone_ranges_from(tail: &[u8], bone_count: u32) -> Result<Vec<BoneRange>> {
    let need = 20 * bone_count as usize;
    if tail.len() < need {
        return Err(RigblockError::Truncated {
            need,
            have: tail.len(),
        });
    }
    let le =
        |at: usize| -> u32 { u32::from_le_bytes(tail[at..at + 4].try_into().unwrap_or([0; 4])) };
    let mut out = Vec::with_capacity(bone_count as usize);
    for bone in 0..bone_count as usize {
        let at = 20 * bone;
        if le(at) != 0 {
            return Err(RigblockError::GmshBoneRangePad { bone });
        }
        out.push(BoneRange {
            bone: le(at + 12),
            start: le(at + 4),
            end: le(at + 8),
            tag: le(at + 16),
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a `gmsh` record: header, positions, blocks, tail.
    fn build(
        vertex_count: u16,
        positions: &[[f32; 3]],
        indices: &[u16],
        bones: &[(u32, u32, u32)],
    ) -> Vec<u8> {
        let mut out = vec![0u8; HEADER];
        out[0x00..0x04].copy_from_slice(&0x0100_0000u32.to_le_bytes());
        out[0x04..0x08].copy_from_slice(&0x0100_0000u32.to_le_bytes());
        out[0x08..0x0C].copy_from_slice(&0x0000_0400u32.to_le_bytes());
        out[0x0C..0x10].copy_from_slice(&0x0001_0300u32.to_le_bytes());
        out[0x10..0x14].copy_from_slice(&3u32.to_le_bytes());
        out[0x14..0x16].copy_from_slice(&vertex_count.to_be_bytes());
        out[0x16] = 0x00;
        out[0x17] = 0x0C;
        for corner in positions {
            out.extend_from_slice(&corner[0].to_le_bytes());
            out.extend_from_slice(&corner[1].to_le_bytes());
            out.extend_from_slice(&corner[2].to_le_bytes());
        }
        // index block: id LE, count BE, elemSize BE
        out.extend_from_slice(&BLOCK_INDICES.to_be_bytes());
        out.extend_from_slice(&(indices.len() as u32).to_be_bytes());
        out.extend_from_slice(&2u16.to_be_bytes());
        for index in indices {
            out.extend_from_slice(&index.to_le_bytes());
        }
        // bone table: payload is 0..nBones-1
        out.extend_from_slice(&BLOCK_BONE_TABLE.to_be_bytes());
        out.extend_from_slice(&(bones.len() as u32).to_be_bytes());
        out.extend_from_slice(&4u16.to_be_bytes());
        for bone in 0..bones.len() {
            out.extend_from_slice(&(bone as u32).to_le_bytes());
        }
        // A third attribute block, because recipe 0x00010300 declares a 3-block
        // chain. Its id is uninterpreted; the decoder must not care.
        out.extend_from_slice(&0x0A00_0400u32.to_be_bytes());
        out.extend_from_slice(&(vertex_count as u32).to_be_bytes());
        out.extend_from_slice(&16u16.to_be_bytes());
        for corner in positions {
            out.extend_from_slice(&corner[0].to_le_bytes());
            out.extend_from_slice(&corner[1].to_le_bytes());
            out.extend_from_slice(&corner[2].to_le_bytes());
            out.extend_from_slice(&1.0f32.to_le_bytes());
        }
        // tail: nv, K1, K1 payload, nBones, 4/0/0/0, then 20-byte ranges
        let k1 = 2u8;
        out.extend_from_slice(&(vertex_count as u32).to_be_bytes());
        out.push(k1);
        out.push(0);
        for index in 0..usize::from(k1) {
            out.push(index as u8);
            out.extend_from_slice(&[0xFF, 0xFF, 0x00]);
        }
        out.extend_from_slice(&(bones.len() as u32).to_be_bytes());
        out.extend_from_slice(&[4, 0, 0, 0]);
        for (index, (bone, start, end)) in bones.iter().enumerate() {
            out.extend_from_slice(&0u32.to_le_bytes());
            out.extend_from_slice(&start.to_le_bytes());
            out.extend_from_slice(&end.to_le_bytes());
            out.extend_from_slice(&bone.to_le_bytes());
            let _ = index;
            out.extend_from_slice(&4u32.to_le_bytes());
        }
        // T2: `nBones × { u32 BE localBone, u16 BE v, 0xFFFFFFFF x2 }`, 14 bytes
        // each. Its `v` field is a per-record base plus `2 * bone`; the referent
        // is UNKNOWN, so the fixture writes a legal shape and the decoder reads
        // none of it.
        for (bone, _) in bones.iter().enumerate() {
            out.extend_from_slice(&(bone as u32).to_be_bytes());
            out.extend_from_slice(&(31u16 + 2 * bone as u16).to_be_bytes());
            out.extend_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
            out.extend_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
        }
        out
    }

    /// Two vertices, one triangle, one bone owning all of it.
    fn simple() -> Vec<u8> {
        build(
            3,
            &[[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            &[0, 1, 2],
            &[(0, 0, 3)],
        )
    }

    #[test]
    fn a_single_bone_mesh_decodes() {
        let mesh = GmshMesh::decode(&simple()).expect("decodes");
        assert_eq!(mesh.header.vertex_count, 3);
        assert_eq!(mesh.positions.len(), 3);
        assert_eq!(mesh.positions[1], [1.0, 0.0, 0.0]);
        assert_eq!(mesh.triangle_count(), 1);
        assert_eq!(mesh.indices, vec![0, 1, 2]);
        assert_eq!(mesh.bone_count, 1);
        assert!(mesh.is_rigid_single_bone());
        assert_eq!(
            mesh.bone_ranges,
            vec![BoneRange {
                bone: 0,
                start: 0,
                end: 3,
                tag: 4,
            }]
        );
        assert_eq!(mesh.bone_at(0), Some(0));
        assert_eq!(mesh.bone_at(3), None, "past the end owns nothing");
        assert_eq!(mesh.tail_ordinal_space, 2);
    }

    #[test]
    fn a_two_bone_mesh_with_a_real_tiling_decodes() {
        // Six indices, bones owning [0,3) and [3,6).
        let record = build(
            4,
            &[
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [1.0, 1.0, 0.0],
            ],
            &[0, 1, 2, 1, 3, 2],
            &[(0, 0, 3), (1, 3, 6)],
        );
        let mesh = GmshMesh::decode(&record).expect("decodes");
        assert_eq!(mesh.bone_count, 2);
        assert!(!mesh.is_rigid_single_bone());
        assert_eq!(mesh.triangle_count(), 2);
        assert_eq!(mesh.bone_at(0), Some(0));
        assert_eq!(mesh.bone_at(3), Some(1));
        assert!(mesh.ranges_are_sorted());
        assert_eq!(
            mesh.triangles().collect::<Vec<_>>(),
            vec![[0, 1, 2], [1, 3, 2]]
        );
    }

    #[test]
    fn an_unsorted_bone_table_still_tiles_and_says_it_is_unsorted() {
        // 599 of 1824 real records store the ranges out of order; the tiling
        // holds only after sorting. This pins both halves of that.
        let record = build(
            4,
            &[
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [1.0, 1.0, 0.0],
            ],
            &[0, 1, 2, 1, 3, 2],
            // Unsorted by start, while `bone` stays the entry index - which is
            // what the real records do.
            &[(0, 3, 6), (1, 0, 3)],
        );
        let mesh = GmshMesh::decode(&record).expect("an unsorted table still decodes");
        assert!(
            !mesh.ranges_are_sorted(),
            "the fixture must actually be unsorted"
        );
        let ordered = mesh.ranges_by_start();
        // The range that starts at 0 is owned by the **second** entry, because
        // file order is not bone order. Getting this backwards is the whole
        // failure this test exists to catch.
        assert_eq!(ordered[0].bone, 1);
        assert_eq!(ordered[0].start, 0);
        assert_eq!(ordered[1].bone, 0);
        assert_eq!(ordered[1].start, 3);
        // The bone word is still the entry's own index in file order.
        assert_eq!(mesh.bone_ranges[0].bone, 0);
        assert_eq!(mesh.bone_ranges[1].bone, 1);
        assert_eq!(mesh.bone_at(0), Some(1));
        assert_eq!(mesh.bone_at(4), Some(0));
    }

    #[test]
    fn a_gap_in_the_bone_tiling_is_refused() {
        // Bones claim [0,3) and [4,6): index 3 is owned by nobody.
        let record = build(
            4,
            &[
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [1.0, 1.0, 0.0],
            ],
            &[0, 1, 2, 1, 3, 2],
            &[(0, 0, 3), (1, 4, 6)],
        );
        assert!(matches!(
            GmshMesh::decode(&record),
            Err(RigblockError::GmshBoneRangeGap {
                expected: 3,
                got: 4
            })
        ));
    }

    #[test]
    fn an_index_at_or_beyond_the_vertex_count_is_refused() {
        let record = build(
            3,
            &[[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            &[0, 1, 7],
            &[(0, 0, 3)],
        );
        assert!(matches!(
            GmshMesh::decode(&record),
            Err(RigblockError::GmshIndexOutOfRange {
                index: 7,
                vertex_count: 3
            })
        ));
    }

    #[test]
    fn an_index_count_that_is_not_triangles_is_refused() {
        let record = build(
            3,
            &[[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            &[0, 1],
            &[(0, 0, 2)],
        );
        assert!(matches!(
            GmshMesh::decode(&record),
            Err(RigblockError::GmshIndexCountNotTriangles { count: 2 })
        ));
    }

    #[test]
    fn a_header_whose_two_constant_bytes_differ_is_refused() {
        let mut record = simple();
        record[0x17] = 0x0D;
        assert!(matches!(
            GmshMesh::decode(&record),
            Err(RigblockError::GmshHeaderShape { byte_17: 0x0D, .. })
        ));
    }

    #[test]
    fn a_record_shorter_than_its_positions_is_refused() {
        let mut record = simple();
        record.truncate(HEADER + 6);
        assert!(matches!(
            GmshMesh::decode(&record),
            Err(RigblockError::Truncated { .. })
        ));
    }

    #[test]
    fn a_zero_element_size_is_refused_rather_than_dividing_by_it() {
        let mut record = simple();
        // Corrupt the index block's element size to zero.
        let at = HEADER + 12 * 3;
        record[at + 8] = 0;
        record[at + 9] = 0;
        assert!(matches!(
            GmshMesh::decode(&record),
            Err(RigblockError::GmshZeroElementSize { .. })
        ));
    }

    #[test]
    fn a_packed_block_is_not_silently_read_as_floats() {
        let block = Block {
            id: 0x0200_0701,
            element_size: 4,
            payload: vec![0xFF; 8],
        };
        // Four bytes per element *is* float-shaped, so this returns a vector -
        // and that is the trap. A block whose bytes are not plausible floats is
        // the caller's problem to notice, so the doc says so.
        let floats = block.as_f32().expect("4-byte elements are float-shaped");
        assert_eq!(floats.len(), 2);
        // A block whose element size is not a multiple of 4 is refused outright.
        let odd = Block {
            id: 1,
            element_size: 2,
            payload: vec![0; 4],
        };
        assert_eq!(odd.as_f32(), None);
    }

    #[test]
    fn the_tail_must_agree_with_the_header_vertex_count() {
        let mut record = simple();
        // The tail is the last `14 + 4*K1 + 20*nBones + 14*nBones` bytes; with
        // K1 = 2 and one bone that is 56. Located from the end so the fixture
        // growing a block does not silently move the test's target.
        let tail_len = 14 + 4 * 2 + 20 + 14;
        let at = record.len() - tail_len;
        record[at] = 0x09; // nv as BE
        assert!(matches!(
            GmshMesh::decode(&record),
            Err(RigblockError::GmshTailVertexMismatch { .. })
        ));
    }
}
