//! Shared test support: the committed fixture loader and an independent
//! synthetic-record builder.
//!
//! # Why an independent builder
//!
//! `tests/fixtures/mini.gmdl` is a *committed* record: it proves the parser
//! agrees with bytes someone else wrote. It cannot prove the parser agrees with
//! this crate's own idea of the layout, because both could be wrong together in
//! the same way. [`Builder`] emits a record from a forward description of the
//! format — written from `docs/ASSET-PATH.md` §3 and the section list in
//! `src/assets/Gmdl.cpp`, not from the parser's code — so a round trip through
//! [`spore_gmdl::parse`] pins the layout in the *writing* direction as well as
//! the reading one.
//!
//! # Why the builder can lie
//!
//! Several fields exist purely to write a record that is wrong in exactly one
//! way: a count word that claims more buffers than are present, a size word that
//! claims more payload than exists. That is what lets each negative test in
//! `tests/parse_errors.rs` name its failure mode instead of computing an offset
//! into the middle of a record and hoping. Every such knob is named
//! `*_count`/`*_word`/`declared_*`; anything not so named is a plain value.

#![allow(dead_code)]

use std::fs;
use std::path::PathBuf;

/// Path to the committed synthetic gmdl fixture.
pub fn fixture_path() -> PathBuf {
    PathBuf::from(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fixtures/mini.gmdl"
    ))
}

/// Path to the committed semantic snapshot of a real gmdl record.
pub fn real_record_json_path() -> PathBuf {
    PathBuf::from(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/expected/real_gmdl_1006.json"
    ))
}

/// Reads the committed fixture.
pub fn mini_gmdl() -> Vec<u8> {
    let path = fixture_path();
    fs::read(&path).unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()))
}

/// The fixture's byte length, asserted rather than assumed by callers.
pub const MINI_GMDL_LEN: usize = 1266;

/// One vertex element to write.
#[derive(Debug, Clone, Copy)]
pub struct Element {
    pub stream: u16,
    pub offset: u16,
    pub decl_type: u8,
    pub decl_method: u8,
    pub decl_usage: u8,
    pub usage_index: u8,
    pub type_code: u32,
}

impl Element {
    /// A stream-0, method-0 element with `usage_index` 0 and `type_code` 0.
    pub const fn new(offset: u16, decl_type: u8, decl_usage: u8) -> Self {
        Self {
            stream: 0,
            offset,
            decl_type,
            decl_method: 0,
            decl_usage,
            usage_index: 0,
            type_code: 0,
        }
    }

    /// Overrides the stream.
    pub const fn stream(mut self, stream: u16) -> Self {
        self.stream = stream;
        self
    }

    /// Overrides the method.
    pub const fn method(mut self, method: u8) -> Self {
        self.decl_method = method;
        self
    }

    /// Overrides the usage index.
    pub const fn usage_index(mut self, index: u8) -> Self {
        self.usage_index = index;
        self
    }

    /// Overrides the opaque type code.
    pub const fn type_code(mut self, code: u32) -> Self {
        self.type_code = code;
        self
    }

    /// The 12 bytes this element occupies on disk.
    pub fn to_bytes(self) -> Vec<u8> {
        let mut out = Vec::with_capacity(12);
        out.extend_from_slice(&self.stream.to_le_bytes());
        out.extend_from_slice(&self.offset.to_le_bytes());
        out.push(self.decl_type);
        out.push(self.decl_method);
        out.push(self.decl_usage);
        out.push(self.usage_index);
        push_u32(&mut out, self.type_code);
        out
    }
}

/// One index buffer to write.
#[derive(Debug, Clone)]
pub struct IndexBuffer {
    pub prim_type: u32,
    pub index_count: u32,
    pub index_bits: u32,
    pub bytes: Vec<u8>,
    /// Overrides the written payload-size word without changing the payload.
    pub size_word: Option<u32>,
}

impl IndexBuffer {
    /// A triangle list of `u16` indices.
    pub fn triangles(indices: &[u32]) -> Self {
        let mut bytes = Vec::with_capacity(indices.len() * 2);
        for index in indices {
            bytes.extend_from_slice(&(*index as u16).to_le_bytes());
        }
        Self {
            prim_type: 4,
            index_count: indices.len() as u32,
            index_bits: 16,
            bytes,
            size_word: None,
        }
    }

    /// A triangle list of `u32` indices (the deliberately rejected 32-bit path).
    pub fn triangles32(indices: &[u32]) -> Self {
        let mut bytes = Vec::with_capacity(indices.len() * 4);
        for index in indices {
            bytes.extend_from_slice(&index.to_le_bytes());
        }
        Self {
            prim_type: 4,
            index_count: indices.len() as u32,
            index_bits: 32,
            bytes,
            size_word: None,
        }
    }

    /// Overrides the primitive type.
    pub fn prim(mut self, prim_type: u32) -> Self {
        self.prim_type = prim_type;
        self
    }

    /// Overrides the declared index count, leaving the payload alone.
    pub fn declared_count(mut self, count: u32) -> Self {
        self.index_count = count;
        self
    }

    /// Overrides the declared index width, leaving the payload alone.
    pub fn width(mut self, bits: u32) -> Self {
        self.index_bits = bits;
        self
    }

    /// Lies about the payload length.
    pub fn size_word(mut self, size: u32) -> Self {
        self.size_word = Some(size);
        self
    }
}

/// One vertex buffer to write.
#[derive(Debug, Clone)]
pub struct VertexBufferSpec {
    pub desc_index: u32,
    pub vertex_count: u32,
    pub payload: Vec<u8>,
    /// Overrides the written payload-size word without changing the payload.
    pub size_word: Option<u32>,
}

impl VertexBufferSpec {
    /// A buffer of `payload` bytes holding `vertex_count` vertices.
    pub fn new(desc_index: u32, vertex_count: u32, payload: Vec<u8>) -> Self {
        Self {
            desc_index,
            vertex_count,
            payload,
            size_word: None,
        }
    }

    /// Overrides the declared vertex count, leaving the payload alone.
    pub fn declared_count(mut self, count: u32) -> Self {
        self.vertex_count = count;
        self
    }

    /// Lies about the payload length.
    pub fn size_word(mut self, size: u32) -> Self {
        self.size_word = Some(size);
        self
    }
}

/// One texture-set entry to write.
#[derive(Debug, Clone, Copy)]
pub struct TextureEntry {
    pub sampler: u32,
    pub opaque: [u8; 12],
    pub instance_id: u32,
    pub group_id: u32,
}

impl TextureEntry {
    /// An entry with a sampler id and 12 opaque zero bytes.
    pub const fn new(sampler: u32, instance_id: u32, group_id: u32) -> Self {
        Self {
            sampler,
            opaque: [0; 12],
            instance_id,
            group_id,
        }
    }

    /// Overwrites the 12 opaque bytes.
    pub fn opaque(mut self, opaque: [u8; 12]) -> Self {
        self.opaque = opaque;
        self
    }
}

/// One material-info entry.
#[derive(Debug, Clone)]
pub enum MaterialEntry {
    /// A shader-data id: the id word plus its documented payload, which the
    /// decoder skips by length. An id with no documented size writes no payload,
    /// which is exactly the record a decoder must refuse.
    Shader(u32),
    /// A shader-data id with a deliberately short payload, so the skip runs past
    /// the record end.
    ShortShader {
        /// The shader-data id.
        id: u32,
        /// Payload bytes actually written.
        payload_len: u32,
    },
    /// A `0x20D` texture set, decoded rather than skipped.
    TextureSet {
        /// The entries actually written.
        textures: Vec<TextureEntry>,
        /// Overrides the written entry count, for the truncation tests.
        declared_count: Option<u32>,
    },
}

impl MaterialEntry {
    /// A texture set whose count word matches its entries.
    pub fn textures(textures: Vec<TextureEntry>) -> Self {
        Self::TextureSet {
            textures,
            declared_count: None,
        }
    }

    /// A texture set whose count word is a lie.
    pub fn textures_declared(declared_count: u32, textures: Vec<TextureEntry>) -> Self {
        Self::TextureSet {
            textures,
            declared_count: Some(declared_count),
        }
    }
}

/// What follows the material info.
#[derive(Debug, Clone)]
pub enum Trailer {
    /// A well-formed trailer: bone ranges, anim data, trailing key.
    Full {
        /// `(start, count)` bone ranges.
        bone_ranges: Vec<(u32, u32)>,
        /// `(baked-deform count, payload)` per anim-data record.
        anim_datas: Vec<(u32, Vec<u32>)>,
        /// The trailing three-word key.
        unknown_key: [u32; 3],
    },
    /// Nothing at all after the material info.
    None,
    /// `n` filler bytes after the material info and no trailer.
    Filler(usize),
}

impl Default for Trailer {
    fn default() -> Self {
        Self::Full {
            bone_ranges: Vec::new(),
            anim_datas: Vec::new(),
            unknown_key: [0, 0, 0],
        }
    }
}

/// A forward description of a gmdl record.
///
/// Every section is a field, written in record order by [`Builder::build`]. The
/// defaults describe the minimal valid record: version 8, no referenced files,
/// one triangle-list mesh of three positions, no material info and an empty
/// trailer.
#[derive(Debug, Clone, Default)]
pub struct Builder {
    pub version: u32,
    /// Referenced-file keys in **on-disk** order: instance, group, type.
    pub refs: Vec<[u32; 3]>,
    /// `None` means "as many meshes as in `meshes`".
    pub mesh_count: Option<u32>,
    pub bounds_min: [f32; 3],
    pub bounds_max: [f32; 3],
    pub radius: f32,
    pub index_buffers: Vec<IndexBuffer>,
    /// Overrides the written index-buffer count word.
    pub index_buffers_count: Option<u32>,
    pub descriptors: Vec<Vec<Element>>,
    /// Overrides the written descriptor count word.
    pub descriptors_count: Option<u32>,
    /// Overrides the written element count of *every* descriptor.
    pub elements_count: Option<u32>,
    pub vertex_buffers: Vec<VertexBufferSpec>,
    /// Overrides the written vertex-buffer count word.
    pub vertex_buffers_count: Option<u32>,
    /// `(index buffer, vertex buffer)` per mesh.
    pub meshes: Vec<(u32, u32)>,
    pub material_ids: Vec<u32>,
    /// The observed zero word between the material ids and the material info.
    pub zero_word: u32,
    /// One entry list per material block.
    pub material_info: Vec<Vec<MaterialEntry>>,
    /// Overrides the written material-info count word.
    pub material_info_count: Option<u32>,
    /// Overrides the written entry count of every material block.
    pub material_entries_count: Option<u32>,
    pub trailer: Trailer,
}

impl Builder {
    /// The minimal valid record: version 8, zero refs, one triangle-list mesh of
    /// three positions in the z = 0 plane, one material id and an empty trailer.
    pub fn minimal() -> Self {
        let positions: [[f32; 3]; 3] = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 1.0, 0.0]];
        let mut payload = Vec::new();
        for position in positions {
            for value in position {
                payload.extend_from_slice(&value.to_le_bytes());
            }
        }
        Self {
            version: 8,
            refs: Vec::new(),
            mesh_count: None,
            bounds_min: [0.0, 0.0, 0.0],
            bounds_max: [1.0, 1.0, 0.0],
            radius: std::f32::consts::FRAC_1_SQRT_2,
            index_buffers: vec![IndexBuffer::triangles(&[0, 1, 2])],
            index_buffers_count: None,
            descriptors: vec![vec![Element::new(0, 2, 0)]],
            descriptors_count: None,
            elements_count: None,
            vertex_buffers: vec![VertexBufferSpec::new(0, 3, payload)],
            vertex_buffers_count: None,
            meshes: vec![(0, 0)],
            material_ids: vec![0x1234_5678],
            zero_word: 0,
            material_info: Vec::new(),
            material_info_count: None,
            material_entries_count: None,
            trailer: Trailer::default(),
        }
    }

    /// Overrides the version word.
    pub fn version(mut self, version: u32) -> Self {
        self.version = version;
        self
    }

    /// A record with `meshCount` 0 and no buffers, descriptors, meshes, material
    /// ids or material info: the shape of the real record captured in
    /// `tests/expected/real_gmdl_1006.json`.
    pub fn empty_mesh() -> Self {
        Self {
            version: 8,
            mesh_count: Some(0),
            ..Self::default()
        }
    }

    /// Sets the `meshCount` word independently of the mesh table.
    pub fn mesh_count(mut self, count: u32) -> Self {
        self.mesh_count = Some(count);
        self
    }

    /// Sets the record's own bounding box and radius.
    pub fn bounds(mut self, min: [f32; 3], max: [f32; 3], radius: f32) -> Self {
        self.bounds_min = min;
        self.bounds_max = max;
        self.radius = radius;
        self
    }

    /// Sets the referenced-file keys, in on-disk `{instance, group, type}` order.
    pub fn refs(mut self, keys: Vec<[u32; 3]>) -> Self {
        self.refs = keys;
        self
    }

    /// Sets the material ids, one per mesh.
    pub fn material_ids(mut self, ids: Vec<u32>) -> Self {
        self.material_ids = ids;
        self
    }

    /// Sets the index buffers.
    pub fn index_buffers(mut self, buffers: Vec<IndexBuffer>) -> Self {
        self.index_buffers = buffers;
        self
    }

    /// Sets the descriptors.
    pub fn descriptors(mut self, descriptors: Vec<Vec<Element>>) -> Self {
        self.descriptors = descriptors;
        self
    }

    /// Sets the vertex buffers.
    pub fn vertex_buffers(mut self, buffers: Vec<VertexBufferSpec>) -> Self {
        self.vertex_buffers = buffers;
        self
    }

    /// Sets the mesh table.
    pub fn meshes(mut self, meshes: Vec<(u32, u32)>) -> Self {
        self.meshes = meshes;
        self
    }

    /// Lies about the index-buffer count word.
    pub fn index_buffers_count(mut self, count: u32) -> Self {
        self.index_buffers_count = Some(count);
        self
    }

    /// Lies about the descriptor count word.
    pub fn descriptors_count(mut self, count: u32) -> Self {
        self.descriptors_count = Some(count);
        self
    }

    /// Lies about every descriptor's element count word.
    pub fn elements_count(mut self, count: u32) -> Self {
        self.elements_count = Some(count);
        self
    }

    /// Lies about the vertex-buffer count word.
    pub fn vertex_buffers_count(mut self, count: u32) -> Self {
        self.vertex_buffers_count = Some(count);
        self
    }

    /// Lies about the material-info count word.
    pub fn material_info_count(mut self, count: u32) -> Self {
        self.material_info_count = Some(count);
        self
    }

    /// Lies about every material block's entry count word.
    pub fn material_entries_count(mut self, count: u32) -> Self {
        self.material_entries_count = Some(count);
        self
    }

    /// Sets the material-info blocks.
    pub fn material_info(mut self, blocks: Vec<Vec<MaterialEntry>>) -> Self {
        self.material_info = blocks;
        self
    }

    /// Replaces the trailer.
    pub fn trailer(mut self, trailer: Trailer) -> Self {
        self.trailer = trailer;
        self
    }

    /// Emits the record bytes.
    pub fn build(&self) -> Vec<u8> {
        let mut out = Vec::new();
        push_u32(&mut out, self.version);
        // The one big-endian *run* in the format: the count and all three words
        // of every key behind it.
        out.extend_from_slice(&(self.refs.len() as u32).to_be_bytes());
        for key in &self.refs {
            // On-disk order: instance, group, type — each big-endian.
            for word in key {
                out.extend_from_slice(&word.to_be_bytes());
            }
        }
        push_u32(
            &mut out,
            self.mesh_count.unwrap_or(self.meshes.len() as u32),
        );
        for value in self.bounds_min {
            out.extend_from_slice(&value.to_le_bytes());
        }
        for value in self.bounds_max {
            out.extend_from_slice(&value.to_le_bytes());
        }
        out.extend_from_slice(&self.radius.to_le_bytes());

        push_u32(
            &mut out,
            self.index_buffers_count
                .unwrap_or(self.index_buffers.len() as u32),
        );
        for buffer in &self.index_buffers {
            push_u32(&mut out, buffer.prim_type);
            push_u32(&mut out, buffer.index_count);
            push_u32(&mut out, buffer.index_bits);
            push_u32(
                &mut out,
                buffer.size_word.unwrap_or(buffer.bytes.len() as u32),
            );
            out.extend_from_slice(&buffer.bytes);
        }

        push_u32(
            &mut out,
            self.descriptors_count
                .unwrap_or(self.descriptors.len() as u32),
        );
        for descriptor in &self.descriptors {
            push_u32(
                &mut out,
                self.elements_count.unwrap_or(descriptor.len() as u32),
            );
            for element in descriptor {
                out.extend_from_slice(&element.to_bytes());
            }
        }

        push_u32(
            &mut out,
            self.vertex_buffers_count
                .unwrap_or(self.vertex_buffers.len() as u32),
        );
        for buffer in &self.vertex_buffers {
            push_u32(&mut out, buffer.desc_index);
            push_u32(&mut out, buffer.vertex_count);
            push_u32(
                &mut out,
                buffer.size_word.unwrap_or(buffer.payload.len() as u32),
            );
            out.extend_from_slice(&buffer.payload);
        }

        for (index_buffer, vertex_buffer) in &self.meshes {
            push_u32(&mut out, *index_buffer);
            push_u32(&mut out, *vertex_buffer);
        }
        for material_id in &self.material_ids {
            push_u32(&mut out, *material_id);
        }
        push_u32(&mut out, self.zero_word);

        push_u32(
            &mut out,
            self.material_info_count
                .unwrap_or(self.material_info.len() as u32),
        );
        for entries in &self.material_info {
            push_u32(
                &mut out,
                self.material_entries_count.unwrap_or(entries.len() as u32),
            );
            for entry in entries {
                match entry {
                    MaterialEntry::Shader(id) => {
                        push_u32(&mut out, *id);
                        // The payload the decoder will skip. Undocumented ids get
                        // no payload: the record is meant to be refused.
                        let size = spore_gmdl::shader_data_size(*id).unwrap_or(0);
                        out.extend(std::iter::repeat_n(0u8, size as usize));
                    }
                    MaterialEntry::ShortShader { id, payload_len } => {
                        push_u32(&mut out, *id);
                        out.extend(std::iter::repeat_n(0u8, *payload_len as usize));
                    }
                    MaterialEntry::TextureSet {
                        textures,
                        declared_count,
                    } => {
                        push_u32(&mut out, 0x20D);
                        push_u32(&mut out, declared_count.unwrap_or(textures.len() as u32));
                        for texture in textures {
                            push_u32(&mut out, texture.sampler);
                            out.extend_from_slice(&texture.opaque);
                            push_u32(&mut out, texture.instance_id);
                            push_u32(&mut out, texture.group_id);
                        }
                    }
                }
            }
        }

        match &self.trailer {
            Trailer::Full {
                bone_ranges,
                anim_datas,
                unknown_key,
            } => {
                push_u32(&mut out, bone_ranges.len() as u32);
                for (start, count) in bone_ranges {
                    push_u32(&mut out, *start);
                    push_u32(&mut out, *count);
                }
                push_u32(&mut out, anim_datas.len() as u32);
                for (baked_count, baked) in anim_datas {
                    out.extend(std::iter::repeat_n(0u8, (64 * 2 + 4 + 12) as usize));
                    push_u32(&mut out, *baked_count);
                    for value in baked {
                        push_u32(&mut out, *value);
                    }
                }
                for word in unknown_key {
                    push_u32(&mut out, *word);
                }
            }
            Trailer::None => {}
            Trailer::Filler(count) => out.extend(std::iter::repeat_n(0xA5u8, *count)),
        }
        out
    }
}

/// Appends a little-endian `u32`.
pub fn push_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

/// Reads a little-endian `u32` at `offset`, or `None`.
pub fn u32_at(data: &[u8], offset: usize) -> Option<u32> {
    let end = offset.checked_add(4)?;
    let window = data.get(offset..end)?;
    Some(u32::from_le_bytes([
        window[0], window[1], window[2], window[3],
    ]))
}

/// Reads a little-endian `f32` at `offset`, or `None`.
pub fn f32_at(data: &[u8], offset: usize) -> Option<f32> {
    Some(f32::from_bits(u32_at(data, offset)?))
}

/// Offset of the first little-endian `u32` equal to `needle`, or `None`.
pub fn find_u32(data: &[u8], needle: u32) -> Option<usize> {
    (0..data.len().saturating_sub(3)).find(|&offset| u32_at(data, offset) == Some(needle))
}

/// Offset of the first occurrence of `needle`, or `None`.
pub fn find_bytes(data: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || needle.len() > data.len() {
        return None;
    }
    (0..=data.len() - needle.len())
        .find(|&offset| data.get(offset..offset + needle.len()) == Some(needle))
}

/// Overwrites the first little-endian `u32` equal to `needle` with `replacement`,
/// reporting whether it was found.
pub fn patch_first_u32(data: &mut [u8], needle: u32, replacement: u32) -> bool {
    if needle == replacement {
        return false;
    }
    let Some(offset) = find_u32(data, needle) else {
        return false;
    };
    let bytes = replacement.to_le_bytes();
    if let Some(slot) = data.get_mut(offset..offset + 4) {
        slot.copy_from_slice(&bytes);
    }
    true
}

/// Overwrites every little-endian `u32` equal to `needle`, returning the count.
pub fn patch_all_u32(data: &mut [u8], needle: u32, replacement: u32) -> usize {
    let mut hits = 0;
    for offset in 0..data.len().saturating_sub(3) {
        if u32_at(data, offset) == Some(needle) {
            let bytes = replacement.to_le_bytes();
            if let Some(slot) = data.get_mut(offset..offset + 4) {
                slot.copy_from_slice(&bytes);
            }
            hits += 1;
        }
    }
    hits
}

/// Builds a `POSITION/FLOAT3`-only vertex payload: three floats per vertex, no
/// padding, so the descriptor's stride is exactly 12.
pub fn positions_payload(positions: &[[f32; 3]]) -> Vec<u8> {
    let mut out = Vec::with_capacity(positions.len() * 12);
    for position in positions {
        for value in position {
            out.extend_from_slice(&value.to_le_bytes());
        }
    }
    out
}

/// Builds an interleaved vertex payload for a descriptor of `POSITION/FLOAT3` at
/// byte 0, `NORMAL/UBYTE4` at `normal_offset` and `TEXCOORD/FLOAT2` at
/// `uv_offset`, padding the gaps with zeros.
pub fn interleaved_vertices(
    positions: &[[f32; 3]],
    normals: &[[u8; 3]],
    uvs: &[[f32; 2]],
    normal_offset: u16,
    uv_offset: u16,
) -> Vec<u8> {
    // The stride a descriptor with exactly these elements implies: the largest
    // element end offset. Computing it here means a payload and the descriptor
    // written in a test cannot disagree about the row length.
    let mut stride = 12usize;
    if !normals.is_empty() {
        stride = stride.max(usize::from(normal_offset) + 4);
    }
    if !uvs.is_empty() {
        stride = stride.max(usize::from(uv_offset) + 8);
    }
    let mut out = Vec::with_capacity(positions.len() * stride);
    for index in 0..positions.len() {
        let mut row: Vec<u8> = Vec::with_capacity(stride);
        for value in positions.get(index).copied().unwrap_or([0.0; 3]) {
            row.extend_from_slice(&value.to_le_bytes());
        }
        if let Some(normal) = normals.get(index) {
            while row.len() < usize::from(normal_offset) {
                row.push(0);
            }
            row.extend_from_slice(normal);
            row.push(0xFF);
        }
        if let Some(uv) = uvs.get(index) {
            while row.len() < usize::from(uv_offset) {
                row.push(0);
            }
            row.extend_from_slice(&uv[0].to_le_bytes());
            row.extend_from_slice(&uv[1].to_le_bytes());
        }
        while row.len() < stride {
            row.push(0);
        }
        out.extend_from_slice(&row);
    }
    out
}
