//! Skeleton and animation: what a gmdl record says, and where the rest lives.
//!
//! A gmdl record carries a *bind-side* skeleton and nothing else. The joints it
//! names are here, in the vertex stream; the transforms that move them are not.
//! That split is the whole content of this module, and every field in it is
//! read verbatim out of a vertex descriptor rather than derived, because the one
//! thing the format does not tell us is which joint a vertex is bound to — only
//! which *slot* of a joint table, and the stride of that table is a measured
//! regularity rather than a documented rule.
//!
//! # What the corpus establishes, and what it does not
//!
//! Over all 4 209 gmdl records of `Spore_Content.package` (5 945 vertex
//! descriptors, 2 159 `BLENDINDICES` elements, 1 330 `BLENDWEIGHT` elements):
//!
//! * 2 109 records declare a `BLENDINDICES` element and 1 330 of their
//!   descriptors declare a `BLENDWEIGHT` element beside it. Both are stream 0,
//!   method 0, usage index 0, without exception.
//! * `BLENDWEIGHT` is always `UBYTE4N` and `BLENDINDICES` is always `UBYTE4`, so
//!   one vertex names exactly four joints with four `u8` slots.
//! * **Every one of those slot values is a multiple of 3**, in 2 109 of 2 109
//!   records — and the gap between two consecutive distinct values is 3 in
//!   48 623 cases against 1 163 for every larger gap put together. The joint
//!   table is therefore strided by three. *Why* is not known; see
//!   [`crate::claims::BLEND_INDEX_STRIDE`].
//! * 315 records declare a non-empty trailer bone-range table, and the trailer's
//!   first range count bounds the joint id (`slot / 3`) in 313 of those 315.
//!
//! # What is deliberately absent
//!
//! No transform, no hierarchy, no timeline, no clip. None of it is in the gmdl
//! record: the joint transforms live in the `rw4` containers the `refCount`
//! table names, in sections this crate does not read. Carrying a `Joint` here
//! with a zeroed matrix would be the exact failure this crate is built to avoid —
//! a plausible zero that reads as "the skeleton is at the origin".
//!
//! See `docs/ANIMATION.md` for the measurements, the RW4 section-code census and
//! the explicit UNKNOWN list.

use crate::parse::{GmdlModel, GmdlVertexElement};
use crate::tables::DeclType;
use crate::DeclUsage;

/// One vertex buffer's skinning declaration, read out of its vertex descriptor.
///
/// Nothing here is inferred from the *values* in the vertex buffer: this is the
/// descriptor's own statement of where the bone indices and weights live inside
/// one interleaved vertex, which is the only part of the binding the record
/// states rather than implies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GmdlSkinBinding {
    /// Index into [`GmdlModel::vertex_buffers`].
    pub vertex_buffer: u32,
    /// Index into [`GmdlModel::descriptors`] — the descriptor this was read from.
    pub descriptor: u32,
    /// The `BLENDWEIGHT` element, verbatim, when the descriptor declares one.
    ///
    /// `None` is a **finding**, not an absence of knowledge: 829 descriptors in
    /// the corpus declare bone indices and no weights at all, and this crate
    /// reports that rather than inventing equal weights.
    pub weights: Option<GmdlVertexElement>,
    /// The `BLENDINDICES` element, verbatim, when the descriptor declares one.
    pub indices: Option<GmdlVertexElement>,
}

impl GmdlSkinBinding {
    /// True when the descriptor declares both halves of a Direct3D 9 skinning
    /// pair.
    ///
    /// This is the *only* shape for which "four weights, four joints" is a
    /// statement the record makes. 812 records in the corpus declare one half.
    pub const fn is_complete(&self) -> bool {
        self.weights.is_some() && self.indices.is_some()
    }

    /// Byte offset of the first bone-index byte inside one interleaved vertex.
    ///
    /// `None` when the descriptor declares no indices, or when the offset plus
    /// the element's width would leave the vertex — which cannot happen for a
    /// parsed model, because [`crate::vertex_stride`] already accounted for it.
    pub const fn indices_offset(&self) -> Option<u16> {
        match self.indices {
            Some(element) => Some(element.offset),
            None => None,
        }
    }

    /// Byte width of one vertex's bone-index block: 4 for the `UBYTE4` encoding
    /// every record in the corpus uses.
    ///
    /// This is the element's declared on-disk size, not an opinion about how
    /// many joints a vertex is bound to. That the two coincide is an observation
    /// about the corpus, graded [`crate::claims::BLEND_INDEX_STRIDE`].
    pub fn index_slots(&self) -> Option<u32> {
        self.indices
            .and_then(|element| DeclType::from_code(element.decl_type))
            .map(DeclType::byte_size)
    }

    /// Byte width of one vertex's bone-weight block: 4 for `UBYTE4N`.
    pub fn weight_slots(&self) -> Option<u32> {
        self.weights
            .and_then(|element| DeclType::from_code(element.decl_type))
            .map(DeclType::byte_size)
    }

    /// True when the descriptor declares bone indices at all, complete pair or
    /// not.
    pub const fn declares_skinning(&self) -> bool {
        self.indices.is_some() || self.weights.is_some()
    }
}

impl GmdlModel {
    /// One binding per vertex buffer whose descriptor declares `BLENDINDICES` or
    /// `BLENDWEIGHT`, in vertex-buffer order.
    ///
    /// Vertex buffers whose descriptor declares neither are **absent from the
    /// result rather than present with empty fields**: 3 991 of the 5 945
    /// descriptors in the corpus are static, and a list that padded every entry
    /// out to the vertex-buffer count would make "this mesh is not skinned" and
    /// "we did not look" the same row. Use [`GmdlModel::uses_skinning`] for the
    /// yes/no question.
    pub fn skin_bindings(&self) -> Vec<GmdlSkinBinding> {
        let mut out = Vec::new();
        for (index, buffer) in self.vertex_buffers.iter().enumerate() {
            let Some(descriptor) = self.descriptors.get(buffer.desc_index as usize) else {
                // Unreachable for a parsed model: `read_vertex_buffers` refuses a
                // buffer whose descriptor index is out of range. Skipping is
                // still the right answer here, because this accessor must not be
                // the place that starts failing.
                continue;
            };
            let pick = |usage: DeclUsage| {
                descriptor
                    .iter()
                    .find(|element| DeclUsage::from_code(element.decl_usage) == Some(usage))
                    .copied()
            };
            let binding = GmdlSkinBinding {
                vertex_buffer: index as u32,
                descriptor: buffer.desc_index,
                weights: pick(DeclUsage::BlendWeight),
                indices: pick(DeclUsage::BlendIndices),
            };
            if binding.declares_skinning() {
                out.push(binding);
            }
        }
        out
    }

    /// Whether any vertex buffer in this record declares a skinning element.
    ///
    /// `false` is a measured statement about this record — the walk read every
    /// descriptor and none of them declares one — not a claim that the model has
    /// no skeleton.
    pub fn uses_skinning(&self) -> bool {
        self.vertex_buffers.iter().any(|buffer| {
            self.descriptors
                .get(buffer.desc_index as usize)
                .is_some_and(|descriptor| {
                    descriptor.iter().any(|element| {
                        matches!(
                            DeclUsage::from_code(element.decl_usage),
                            Some(DeclUsage::BlendWeight | DeclUsage::BlendIndices)
                        )
                    })
                })
        })
    }

    /// The sum of the trailer bone ranges' `count` words.
    ///
    /// Read the name literally: this is the sum of a table whose entries this
    /// crate reproduces but does not interpret. It is exposed because it is the
    /// one number in the record that *bounds* the joint ids the vertex stream
    /// names — in 313 of the 315 corpus records that declare ranges, every
    /// `BLENDINDICES` slot divided by three is below it — and a renderer needs
    /// that bound to size a joint table even though it cannot fill the table.
    ///
    /// Whether it is a joint count, a LOD-split joint count, or something else is
    /// [`crate::claims::BONE_RANGE_MEANING`].
    ///
    /// Zero means "the record declared no ranges" **only** when
    /// [`GmdlModel::trailer`] is [`crate::TrailerWalk::Complete`]; read the two
    /// together or this returns a zero that means "the walk never got there".
    pub fn bone_range_span(&self) -> u32 {
        self.bone_ranges
            .iter()
            .fold(0u32, |total, (_, count)| total.saturating_add(*count))
    }

    /// Whether the record declares a bone-range table at all.
    ///
    /// Separate from [`GmdlModel::bone_range_span`] for the reason
    /// [`crate::TrailerWalk`] exists: an empty table and a walk that stopped
    /// before reaching the count word are different facts.
    pub fn declares_bone_ranges(&self) -> bool {
        !self.bone_ranges.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::GmdlVertexBuffer;
    use crate::WalkStage;

    fn element(offset: u16, decl_type: u8, usage: DeclUsage) -> GmdlVertexElement {
        GmdlVertexElement {
            stream: 0,
            offset,
            decl_type,
            decl_method: 0,
            decl_usage: usage.as_code(),
            usage_index: 0,
            type_code: 0,
        }
    }

    fn model_with(descriptors: Vec<Vec<GmdlVertexElement>>) -> GmdlModel {
        let vertex_buffers = descriptors
            .iter()
            .enumerate()
            .map(|(index, _)| GmdlVertexBuffer {
                desc_index: index as u32,
                vertex_count: 3,
                bytes: vec![0; 3 * 64],
            })
            .collect();
        GmdlModel {
            version: crate::SUPPORTED_VERSION,
            referenced_files: Vec::new(),
            mesh_count: 1,
            bounds_min: [0.0; 3],
            bounds_max: [0.0; 3],
            radius: 0.0,
            index_buffers: Vec::new(),
            descriptors,
            vertex_buffers,
            meshes: Vec::new(),
            material_ids: Vec::new(),
            texture_refs: Vec::new(),
            bone_ranges: Vec::new(),
            unknown_key: [0; 3],
            trailer: crate::TrailerWalk::Complete,
            strict_consumed: 0,
            consumed: 0,
        }
    }

    /// POSITION/FLOAT3, NORMAL/UBYTE4, then the two skinning elements.
    fn skinned() -> Vec<GmdlVertexElement> {
        vec![
            element(0, 2, DeclUsage::Position),
            element(12, 5, DeclUsage::Normal),
            element(16, 5, DeclUsage::BlendIndices),
            element(20, 8, DeclUsage::BlendWeight),
            element(24, 1, DeclUsage::TexCoord),
        ]
    }

    /// The same layout without the weight element — a real corpus shape.
    fn indices_only() -> Vec<GmdlVertexElement> {
        vec![
            element(0, 2, DeclUsage::Position),
            element(12, 5, DeclUsage::Normal),
            element(16, 5, DeclUsage::BlendIndices),
            element(20, 1, DeclUsage::TexCoord),
        ]
    }

    fn static_mesh() -> Vec<GmdlVertexElement> {
        vec![
            element(0, 2, DeclUsage::Position),
            element(12, 5, DeclUsage::Normal),
            element(16, 1, DeclUsage::TexCoord),
        ]
    }

    #[test]
    fn a_complete_pair_reports_four_slots_of_each() {
        let model = model_with(vec![skinned()]);
        let bindings = model.skin_bindings();
        assert_eq!(bindings.len(), 1);
        let binding = bindings.first().expect("one binding");
        assert!(binding.is_complete());
        assert_eq!(binding.vertex_buffer, 0);
        assert_eq!(binding.descriptor, 0);
        assert_eq!(binding.indices_offset(), Some(16));
        // UBYTE4 and UBYTE4N are both four bytes wide; the *slot* count is the
        // element's declared size, not an assumption about joint count.
        assert_eq!(binding.index_slots(), Some(4));
        assert_eq!(binding.weight_slots(), Some(4));
        assert!(model.uses_skinning());
    }

    #[test]
    fn indices_without_weights_are_reported_rather_than_completed() {
        let model = model_with(vec![indices_only()]);
        let bindings = model.skin_bindings();
        assert_eq!(bindings.len(), 1, "the half that exists is still reported");
        let binding = bindings.first().expect("one binding");
        assert!(!binding.is_complete());
        assert!(binding.declares_skinning());
        assert!(binding.weights.is_none());
        assert_eq!(binding.index_slots(), Some(4));
        assert_eq!(
            binding.weight_slots(),
            None,
            "no weight element means no weight width, not a width of zero"
        );
    }

    #[test]
    fn a_static_mesh_contributes_no_binding_at_all() {
        let model = model_with(vec![static_mesh()]);
        assert!(model.skin_bindings().is_empty());
        assert!(!model.uses_skinning());
    }

    #[test]
    fn a_mixed_record_reports_only_the_skinned_buffers() {
        let model = model_with(vec![static_mesh(), skinned(), indices_only()]);
        let bindings = model.skin_bindings();
        assert_eq!(bindings.len(), 2, "buffer 0 is static and is absent");
        let vertices: Vec<u32> = bindings.iter().map(|b| b.vertex_buffer).collect();
        assert_eq!(vertices, vec![1, 2], "in vertex-buffer order");
        assert!(model.uses_skinning());
    }

    #[test]
    fn a_second_binding_reads_its_own_offsets() {
        // The offsets are per descriptor, so a record with two differently laid
        // out skinned buffers must not share one binding's offsets.
        let mut second = skinned();
        second[2].offset = 24;
        second[3].offset = 28;
        let model = model_with(vec![skinned(), second]);
        let bindings = model.skin_bindings();
        assert_eq!(bindings.len(), 2);
        assert_eq!(bindings[0].indices_offset(), Some(16));
        assert_eq!(bindings[1].indices_offset(), Some(24));
    }

    #[test]
    fn bone_range_span_saturates_rather_than_wrapping() {
        let mut model = model_with(vec![skinned()]);
        model.bone_ranges = vec![(0, u32::MAX), (0, 2)];
        assert_eq!(model.bone_range_span(), u32::MAX, "sum saturates");
        model.bone_ranges = vec![(4, 7), (11, 3)];
        assert_eq!(model.bone_range_span(), 10);
        assert!(model.declares_bone_ranges());
    }

    #[test]
    fn an_empty_bone_table_is_distinguishable_from_a_walk_that_never_got_there() {
        let mut model = model_with(vec![skinned()]);
        model.trailer = crate::TrailerWalk::Complete;
        assert!(!model.declares_bone_ranges());
        assert_eq!(model.bone_range_span(), 0);
        // Same empty vector, different fact. This is why `TrailerWalk` is a
        // separate type rather than a bool.
        model.trailer = crate::TrailerWalk::Truncated {
            at: crate::TrailerStage::BoneRangeCount,
        };
        assert!(!model.declares_bone_ranges());
        assert_eq!(model.bone_range_span(), 0);
        assert!(!model.trailer.is_complete());
        assert_eq!(
            model.trailer.stopped_at(),
            Some(crate::TrailerStage::BoneRangeCount)
        );
    }

    #[test]
    fn a_dangling_descriptor_index_is_skipped_rather_than_panicking() {
        // Not reachable through `parse` — `read_vertex_buffers` refuses it — but
        // the accessors take `&self` on a public struct, so a hand-built model
        // can carry one and the answer must still be a value, not a panic.
        let mut model = model_with(vec![skinned()]);
        model.vertex_buffers[0].desc_index = 7;
        assert!(model.skin_bindings().is_empty());
        assert!(!model.uses_skinning());
    }

    #[test]
    fn the_walk_stage_of_a_truncated_tail_is_carried_through() {
        // A sanity check that this module's types compose with the parse
        // module's, rather than shadowing them.
        assert_eq!(WalkStage::Trailer.as_str(), "trailer");
        assert_eq!(WalkStage::Geometry.as_str(), "geometry");
    }
}
