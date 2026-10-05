//! The gmdl record walk: header, referenced files, bounds, index buffers,
//! vertex descriptors, vertex buffers, mesh refs, materials, material info.
//!
//! # The `refCount` table is big-endian — the count *and* every key word
//!
//! **Every other word in this format is little-endian. The 4-byte count and all
//! 36 bytes of each `{instance, group, type}` key behind it are big-endian.**
//!
//! Two separate bugs live in those 4 bytes, and they are independent: fixing the
//! count and leaving the keys little-endian produces a record that parses and
//! carries keys that address nothing at all.
//!
//! *Count.* A little-endian read of a record with `N` referenced files yields
//! `N << 24` — 16 777 216 for a single reference — and the walk then asks the
//! record for `N << 24 * 12` bytes of file keys, which runs straight off the
//! end and either truncates mid-record or, worse, decodes plausible nonsense
//! out of geometry data. This is the single most expensive bug in this format's
//! history. It was found in this repository by reading a 175 KB record whose
//! little-endian count came out as `0x05000000` (`docs/CELLSTAGE-RECON.md` §2)
//! and it explained 1 510 of the 4 209 gmdl walk failures in
//! `Spore_Content.package` — every record with at least one reference.
//!
//! *Keys.* Over `Spore_Content.package`, reading the three words of every key
//! big-endian resolves **7 989 of 7 989** declared keys to real DBPF index rows;
//! reading them little-endian resolves **0 of 7 989**. The count word was
//! already correct, which is exactly why this survived: the walk consumed the
//! right number of bytes and produced plausible `ResourceKey`s that named
//! nothing. All six orderings of the three words were tried; exactly one —
//! `(instance, group, type)`, the order this crate documents — resolves, and
//! only big-endian. See `docs/ANIMATION.md` §2.
//!
//! The reader keeps `read_u32` and `read_u32_be` as separate methods precisely
//! so that getting this wrong requires typing the exception. Pinning:
//! `tests/refcount_be.rs` (synthetic) and `tests/corpus_animation.rs`
//! (index resolution against the installed package, `#[ignore]`d).
//!
//! # Strict, then best-effort
//!
//! Everything from the version word through the end of the material info is
//! strict: a short buffer is a hard error. What follows — bone ranges, anim
//! data, baked deforms, the trailing key — is walked best-effort and never
//! fails a parse. That asymmetry is not laziness; it reflects what the records
//! actually contain. The trailer framing is verified on the `mini.gmdl` /
//! `CellImages` family, while the real cell-stage records at groups
//! `0x40616201`/`0x40616202` carry a larger opaque baked-deform trailer with a
//! different framing that overruns the known walk. Refusing those records would
//! throw away perfectly decodable geometry, so the tail is reported instead of
//! enforced: [`GmdlModel::strict_consumed`] and [`GmdlModel::consumed`] say
//! exactly how far each claim reaches, and [`GmdlModel::trailer`] says whether
//! the tail was walked at all.

use spore_core::ResourceKey;

use crate::error::GmdlError;
use crate::reader::Reader;
use crate::SUPPORTED_VERSION;

/// Bytes one referenced-file key occupies: `{instance, group, type}`.
///
/// The on-disk order is *not* the canonical `T:G:I` order of
/// [`ResourceKey`]; see [`Walk::read_referenced_files`].
const REF_KEY_SIZE: u64 = 12;

/// Bytes one index-buffer header occupies:
/// `{prim_type, index_count, index_bits, buf_size}`.
const INDEX_BUFFER_HEADER_SIZE: u64 = 16;

/// Bytes one vertex-buffer header occupies:
/// `{desc_index, vertex_count, buf_size}`.
const VERTEX_BUFFER_HEADER_SIZE: u64 = 12;

/// Bytes one vertex-element record occupies:
/// `{u16 stream, u16 offset, u8 decl_type, u8 decl_method, u8 decl_usage,
/// u8 usage_index, u32 type_code}`.
const VERTEX_ELEMENT_SIZE: u64 = 12;

/// Bytes one texture-set entry occupies: 16 skipped bytes (`{sampler, opaque}`)
/// plus `u32 instance` plus `u32 group`.
const TEXTURE_ENTRY_SIZE: u64 = 24;

/// Bytes one anim-data record occupies before its baked-deform count: two baked
/// transforms (64 bytes each), a flags word, and a 12-byte resource key.
const ANIM_DATA_PREFIX_SIZE: u64 = 64 * 2 + 4 + 12;

/// Bytes one bone range occupies: `{start, count}`.
const BONE_RANGE_SIZE: u64 = 8;

/// Bytes one baked-deform entry occupies.
const BAKED_DEFORM_SIZE: u64 = 4;

/// The trailer stage the best-effort walk stopped in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TrailerStage {
    /// The bone-range count word.
    BoneRangeCount,
    /// The bone-range array itself.
    BoneRanges,
    /// The anim-data count word.
    AnimDataCount,
    /// An anim-data record's fixed-size prefix (transforms, flags, key).
    AnimData,
    /// An anim-data record's baked-deform array.
    BakedDeform,
    /// The trailing three-word key.
    UnknownKey,
}

/// How far the best-effort trailer walk got.
///
/// This exists so that an empty [`GmdlModel::bone_ranges`] is not ambiguous.
/// "The walk reached the end and the record has no bone ranges" and "the walk
/// never got to the bone-range word" produce the same empty vector, and
/// collapsing them is how a decoder turns "we did not look" into "there is
/// nothing there".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TrailerWalk {
    /// The trailer was walked to the end of the record, so
    /// `strict_consumed == consumed` and every byte of the record was read.
    Complete,
    /// The walk ran out of record inside the trailer. `strict_consumed` is where
    /// it stopped; the bytes after it were **not** validated.
    Truncated {
        /// Which stage ran out.
        at: TrailerStage,
    },
}

impl TrailerWalk {
    /// True when the walk consumed the trailer in bounds.
    pub const fn is_complete(self) -> bool {
        matches!(self, TrailerWalk::Complete)
    }

    /// True when the walk ran out of record before the end of the trailer.
    pub const fn is_truncated(self) -> bool {
        matches!(self, TrailerWalk::Truncated { .. })
    }

    /// Where the walk stopped, if it stopped short.
    pub const fn stopped_at(self) -> Option<TrailerStage> {
        match self {
            TrailerWalk::Complete => None,
            TrailerWalk::Truncated { at } => Some(at),
        }
    }
}

/// One `D3DVERTEXELEMENT`-shaped vertex element (12 bytes on disk).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GmdlVertexElement {
    /// Stream the element belongs to. Only stream 0 is read.
    pub stream: u16,
    /// Byte offset of the element within the interleaved vertex.
    pub offset: u16,
    /// `D3DDECLTYPE` code; see [`crate::DeclType`].
    pub decl_type: u8,
    /// Per-vertex transform method. Only 0 (default) is read.
    pub decl_method: u8,
    /// `D3DDECLUSAGE` code; see [`crate::DeclUsage`].
    pub decl_usage: u8,
    /// Which texcoord/normal set this element belongs to.
    pub usage_index: u8,
    /// Opaque type code, carried verbatim and never interpreted.
    pub type_code: u32,
}

/// One index buffer: a 16-byte header plus its raw payload.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct GmdlIndexBuffer {
    /// RenderWare `PRIM` primitive type. 4 is a triangle list.
    pub prim_type: u32,
    /// Number of indices the buffer declares.
    pub index_count: u32,
    /// Bits per index. 16 on every record this build has decoded.
    pub index_bits: u32,
    /// Raw little-endian index payload.
    pub bytes: Vec<u8>,
}

/// One vertex buffer: a 12-byte header plus its raw payload.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct GmdlVertexBuffer {
    /// Index into [`GmdlModel::descriptors`].
    pub desc_index: u32,
    /// Number of interleaved vertices the buffer declares.
    pub vertex_count: u32,
    /// Raw interleaved vertex payload.
    pub bytes: Vec<u8>,
}

/// One mesh: the pair of buffers it draws from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GmdlMeshRef {
    /// Index into [`GmdlModel::index_buffers`].
    pub index_buffer: u32,
    /// Index into [`GmdlModel::vertex_buffers`].
    pub vertex_buffer: u32,
}

/// One texture reference from a `0x20D` texture set.
///
/// The on-disk entry carries no type word, so only the instance/group pair is
/// recoverable; the referenced record's type comes from context, not from here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GmdlTextureRef {
    /// Instance id of the referenced texture record.
    pub instance_id: u32,
    /// Group id of the referenced texture record.
    pub group_id: u32,
}

/// A decoded gmdl record.
#[derive(Debug, Clone, PartialEq)]
pub struct GmdlModel {
    /// Record version. Always [`SUPPORTED_VERSION`] after a successful parse.
    pub version: u32,

    /// Keys of the files this model references, in record order.
    ///
    /// The three words are stored on disk as `{instance, group, type}` and are
    /// reordered here into [`ResourceKey`]'s `{type_id, group_id, instance_id}`
    /// so that every record identity in this workspace is spelled one way. The
    /// reorder is observed layout; what these files *are* (materials? shared
    /// geometry?) is not known — see the `referenced_files_role` claim.
    pub referenced_files: Vec<ResourceKey>,

    /// Number of meshes the record declares.
    pub mesh_count: u32,

    /// Lower corner of the record's own bounding box.
    pub bounds_min: [f32; 3],
    /// Upper corner of the record's own bounding box.
    pub bounds_max: [f32; 3],
    /// Bounding radius as written in the record.
    pub radius: f32,

    /// Index buffers, in record order.
    pub index_buffers: Vec<GmdlIndexBuffer>,
    /// Vertex descriptors, in record order. The outer vector is the descriptor
    /// table; each inner vector is one descriptor's element list.
    pub descriptors: Vec<Vec<GmdlVertexElement>>,
    /// Vertex buffers, in record order.
    pub vertex_buffers: Vec<GmdlVertexBuffer>,
    /// Mesh table, in record order.
    pub meshes: Vec<GmdlMeshRef>,
    /// One material id per mesh, in record order. Carried verbatim; the
    /// material-info section is what would interpret them.
    pub material_ids: Vec<u32>,

    /// Texture references in order of appearance across all texture sets.
    pub texture_refs: Vec<GmdlTextureRef>,

    /// Bone ranges as `(start, count)` pairs, as far as the best-effort walk
    /// read them. Read together with [`GmdlModel::trailer`]: an empty vector
    /// means "no bone ranges" only when the walk completed.
    pub bone_ranges: Vec<(u32, u32)>,

    /// The trailing three-word key, read verbatim. All three words are zero in
    /// `mini.gmdl`; the one real record captured in
    /// `tests/expected/real_gmdl_1006.json` reads `(0, 0xFFFFFFFF, 0)`, whose
    /// middle `0xFFFFFFFF` is exactly [`spore_core::WILDCARD`].
    ///
    /// Left as zeros when the trailer walk did not reach it — see
    /// [`GmdlModel::trailer`].
    pub unknown_key: [u32; 3],

    /// How far the best-effort trailer walk got.
    pub trailer: TrailerWalk,

    /// How far the walk reached: the end of the validated geometry and
    /// material-info sections, extended through the trailer whenever the
    /// trailer walk stayed in bounds.
    ///
    /// Equal to [`GmdlModel::consumed`] exactly when the whole record was read
    /// (see [`GmdlModel::fully_walked`]). When it is smaller, the bytes in
    /// between were **not** validated: the parse still succeeded, because the
    /// tail is best-effort by design, but nothing is claimed about it.
    pub strict_consumed: usize,

    /// Bytes consumed of the input. Always the input length: this crate never
    /// decodes a prefix of a record and calls it a record.
    pub consumed: usize,
}

impl GmdlModel {
    /// True when every byte of the input was consumed by the walk.
    ///
    /// This is a statement about *bytes*, not about meaning, and the two come
    /// apart: a record whose last trailer word claimed a structure extending past
    /// the end has still had all of its bytes read, and is still
    /// [`GmdlModel::trailer`]-truncated. Both facts are reported rather than
    /// collapsed into one flag, because "we read all of it" and "all of it was
    /// there" are different claims.
    pub fn fully_walked(&self) -> bool {
        self.strict_consumed == self.consumed
    }

    /// The interleaved stride implied by one vertex buffer's descriptor, or
    /// `None` when the buffer or its descriptor index is out of range.
    pub fn vertex_stride_of(&self, vertex_buffer: usize) -> Option<Result<u32, GmdlError>> {
        let buffer = self.vertex_buffers.get(vertex_buffer)?;
        let descriptor = self.descriptors.get(buffer.desc_index as usize)?;
        Some(crate::tables::vertex_stride(descriptor))
    }
}

/// Which part of a record a partial walk reached.
///
/// The ordering is the record's own order, and it matters: a walk that stops at
/// [`WalkStage::MaterialInfo`] has already validated everything a renderer
/// needs, while one that stops at [`WalkStage::Geometry`] has not.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum WalkStage {
    /// Version, references, bounds, index buffers, descriptors, vertex buffers,
    /// the mesh table and the per-mesh material ids.
    Geometry,
    /// The material-info table: texture sets and other shader-data entries.
    MaterialInfo,
    /// Bone ranges, anim data and the trailing key.
    Trailer,
}

impl WalkStage {
    /// The canonical spelling, matching the trailer-stage names already used.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Geometry => "geometry",
            Self::MaterialInfo => "material-info",
            Self::Trailer => "trailer",
        }
    }
}

/// Where a recovering walk stopped, and why.
#[derive(Debug, Clone, PartialEq)]
pub struct StopReason {
    /// The section that was being read.
    pub stage: WalkStage,
    /// The typed failure. Carried verbatim so a caller can match on it.
    pub error: GmdlError,
}

/// A gmdl record decoded as far as the format is understood.
///
/// # Why this exists
///
/// A real corpus contains gmdl records that name a shader-data id this crate
/// has no size for -- `0x218`, on 619 records of `Spore_Content` including
/// **every** record of the creature-model group `0x40627100`. Because the
/// material-info table sits *after* the mesh table, those records already have
/// complete, self-validated geometry by the time the walk stops: index buffers,
/// vertex descriptors, vertex buffers, the mesh table and the per-mesh material
/// ids are all behind it and all verified.
///
/// [`parse`] refuses them, and that is correct for verification: a record whose
/// walk does not close has not been fully decoded, and a differential harness
/// that accepted it would be comparing two incomplete things. But a *renderer*
/// has no use for the tail of a creature model, and throwing away 1.3 MB of
/// validated geometry because a material entry is unmeasured trades something
/// known for something unknown.
///
/// So there are two entry points with deliberately different contracts:
///
/// | function | contract | used by |
/// |---|---|---|
/// | [`parse`] | the whole record, or an error. No partial model. | verification, `osptool describe`, the differential harness |
/// | [`parse_recovering`] | geometry-complete-or-error, plus an explicit [`StopReason`] | the renderer |
///
/// `parse_recovering` **never** invents a size, a count or a value. It stops,
/// and it says where and why. A caller that ignores [`PartialGmdl::stop`] and
/// treats the result as a complete record has discarded the only information
/// that made the difference visible, which is why the report line and the
/// material layer both read it.
#[derive(Debug, Clone, PartialEq)]
pub struct PartialGmdl {
    /// The model as far as it was decoded.
    pub model: GmdlModel,
    /// Why the walk stopped, or `None` when it reached the record end.
    ///
    /// `None` is the strong claim: the walk closed exactly on the record end.
    /// `Some` means the geometry is intact and something after the mesh table
    /// is not understood.
    pub stop: Option<StopReason>,
}

impl PartialGmdl {
    /// Whether the walk closed exactly on the record end.
    pub const fn is_complete(&self) -> bool {
        self.stop.is_none()
    }

    /// Whether the geometry a renderer needs is fully decoded.
    ///
    /// True for a complete record and for one that stopped at or after the
    /// material-info table. False is never returned -- a walk that stopped
    /// inside the geometry is an error, not a `PartialGmdl`.
    pub const fn geometry_complete(&self) -> bool {
        true
    }
}

/// Decodes a gmdl record, keeping the geometry when the tail is unknown.
///
/// Returns [`GmdlError`] when the failure is at or before the mesh table,
/// because then the geometry is *not* decoded and a `PartialGmdl` would be a
/// lie. See [`PartialGmdl`] for the full contract.
pub fn parse_recovering(data: &[u8]) -> Result<PartialGmdl, GmdlError> {
    if data.is_empty() {
        return Err(GmdlError::EmptyInput);
    }
    let mut walk = Walk::new(data);
    // Everything a renderer needs, strictly. A failure here is a real failure.
    walk.read_version()?;
    walk.read_referenced_files()?;
    walk.read_mesh_count_and_bounds()?;
    walk.read_index_buffers()?;
    walk.read_descriptors()?;
    walk.read_vertex_buffers()?;
    walk.read_mesh_table()?;

    // From here on, stopping is recorded rather than propagated.
    let stop = match walk.read_material_info() {
        Ok(()) => None,
        Err(error) => Some(StopReason {
            stage: WalkStage::MaterialInfo,
            error,
        }),
    };
    if stop.is_none() {
        // `read_trailer` already swallows its own failures into
        // `TrailerWalk::Truncated`; the model reports that itself.
        walk.read_trailer();
    }
    Ok(PartialGmdl {
        model: walk.finish(data.len()),
        stop,
    })
}

/// Decodes a whole gmdl record.
///
/// On success the model is complete: `consumed == data.len()`, and
/// `strict_consumed <= consumed` (equal whenever the trailer walk also landed
/// exactly on the record end). On failure nothing is returned — there is no
/// partially populated model to mistake for a decoded one.
pub fn parse(data: &[u8]) -> Result<GmdlModel, GmdlError> {
    if data.is_empty() {
        return Err(GmdlError::EmptyInput);
    }
    let mut walk = Walk::new(data);
    walk.strict_sections()?;
    walk.read_trailer();
    Ok(walk.finish(data.len()))
}

/// The bytes that follow the validated geometry and material-info sections.
///
/// This is the **animation trailer**: the bone-range table, the `GameModelAnimData`
/// array and the trailing resource key — the part of the record
/// [`GmdlModel::bone_ranges`] is read out of, and the part
/// [`GmdlModel::strict_consumed`] may stop in the middle of because its framing
/// is best-effort.
///
/// # Why this is a function and not a field on [`GmdlModel`]
///
/// A caller measuring that region needs the trailer's **start**, and
/// `strict_consumed` is the walk's **stop** — after a truncated walk those differ
/// by an unknown amount, so `data[strict_consumed..]` is the un-validated
/// remainder, not the trailer. Adding a field would break every crate that builds
/// a `GmdlModel` literal; this walks the record instead and hands back a slice,
/// which is additive.
///
/// `None` means the strict sections did not validate, so the record has no
/// located trailer — not "an empty trailer". A record whose material info names a
/// shader-data id this crate has no size for (the `0x218` family, including every
/// creature-model record) returns `None` here even though it decodes perfectly
/// far enough to render.
pub fn trailer_region(data: &[u8]) -> Option<&[u8]> {
    if data.is_empty() {
        return None;
    }
    let mut walk = Walk::new(data);
    walk.strict_sections().ok()?;
    Some(&data[walk.r.position()..])
}

/// The walk itself, split into one method per record section so that the order
/// of the source is the order of the walk.
struct Walk<'a> {
    r: Reader<'a>,
    model: GmdlModel,
    trailer: TrailerWalk,
}

impl<'a> Walk<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self {
            r: Reader::new(data),
            model: GmdlModel {
                version: 0,
                referenced_files: Vec::new(),
                mesh_count: 0,
                bounds_min: [0.0; 3],
                bounds_max: [0.0; 3],
                radius: 0.0,
                index_buffers: Vec::new(),
                descriptors: Vec::new(),
                vertex_buffers: Vec::new(),
                meshes: Vec::new(),
                material_ids: Vec::new(),
                texture_refs: Vec::new(),
                bone_ranges: Vec::new(),
                unknown_key: [0; 3],
                trailer: TrailerWalk::Complete,
                strict_consumed: 0,
                consumed: 0,
            },
            trailer: TrailerWalk::Complete,
        }
    }

    /// Every section that must validate, in record order.
    fn strict_sections(&mut self) -> Result<(), GmdlError> {
        self.read_version()?;
        self.read_referenced_files()?;
        self.read_mesh_count_and_bounds()?;
        self.read_index_buffers()?;
        self.read_descriptors()?;
        self.read_vertex_buffers()?;
        self.read_mesh_table()?;
        self.read_material_info()
    }

    fn read_version(&mut self) -> Result<(), GmdlError> {
        let version = self.r.read_u32().ok_or(GmdlError::TruncatedHeader {
            got: self.r.remaining(),
        })?;
        if version != SUPPORTED_VERSION {
            // Version 9 changes the material-info framing (its per-material
            // block carries no entry-count word) and no v9 record has been
            // validated against this walk, so it is named and refused rather
            // than attempted.
            return Err(GmdlError::UnsupportedVersion { version });
        }
        self.model.version = version;
        Ok(())
    }

    fn read_referenced_files(&mut self) -> Result<(), GmdlError> {
        // ===================================================================
        // BIG-ENDIAN — the count AND all three words of every key behind it.
        // See the module docs; this is 52 bytes in the same record read two
        // different ways.
        // ===================================================================
        let count = self.r.read_u32_be().ok_or(GmdlError::TruncatedRefCount)?;
        let needed = u64::from(count) * REF_KEY_SIZE;
        let available = self.r.remaining();
        if needed > available as u64 {
            return Err(GmdlError::TruncatedReferencedFiles {
                count,
                needed,
                available,
            });
        }
        let truncated = GmdlError::TruncatedReferencedFiles {
            count,
            needed,
            available,
        };
        for _ in 0..count {
            // On-disk order is instance, group, type; ResourceKey is
            // type, group, instance. The reorder is the whole reason this read
            // is three separate words instead of a slice copy — and every one
            // of the three is big-endian too.
            let instance = self.r.read_u32_be().ok_or_else(|| truncated.clone())?;
            let group = self.r.read_u32_be().ok_or_else(|| truncated.clone())?;
            let type_id = self.r.read_u32_be().ok_or_else(|| truncated.clone())?;
            self.model
                .referenced_files
                .push(ResourceKey::new(type_id, group, instance));
        }
        Ok(())
    }

    fn read_mesh_count_and_bounds(&mut self) -> Result<(), GmdlError> {
        // `meshCount` shares the truncation error with the bounds: the record
        // layout puts them in one run of words, and the C++ reference reports a
        // short read there as a bounds failure too.
        let available = self.r.remaining();
        let mesh_count = self
            .r
            .read_u32()
            .ok_or(GmdlError::TruncatedBounds { available })?;
        self.model.mesh_count = mesh_count;

        for component in 0..3 {
            let value = self
                .r
                .read_f32()
                .ok_or(GmdlError::TruncatedBounds { available })?;
            if !value.is_finite() {
                return Err(GmdlError::NonFiniteBounds {
                    component: bounds_component(0, component),
                    value,
                });
            }
            set_component(&mut self.model.bounds_min, component, value);
        }
        for component in 0..3 {
            let value = self
                .r
                .read_f32()
                .ok_or(GmdlError::TruncatedBounds { available })?;
            if !value.is_finite() {
                return Err(GmdlError::NonFiniteBounds {
                    component: bounds_component(1, component),
                    value,
                });
            }
            set_component(&mut self.model.bounds_max, component, value);
        }
        let radius = self
            .r
            .read_f32()
            .ok_or(GmdlError::TruncatedBounds { available })?;
        if !radius.is_finite() {
            return Err(GmdlError::InvalidBounds { radius });
        }
        self.model.radius = radius;
        Ok(())
    }

    fn read_index_buffers(&mut self) -> Result<(), GmdlError> {
        let available = self.r.remaining();
        let count = self
            .r
            .read_u32()
            .ok_or(GmdlError::TruncatedIndexBufferTable {
                // The count word itself is missing: nothing was declared, and the
                // four bytes it needed are what is missing.
                count: 0,
                needed: 4,
                available,
            })?;
        let needed = u64::from(count) * INDEX_BUFFER_HEADER_SIZE;
        let available = self.r.remaining();
        if needed > available as u64 {
            return Err(GmdlError::TruncatedIndexBufferTable {
                count,
                needed,
                available,
            });
        }
        for index in 0..count as usize {
            let short = GmdlError::TruncatedIndexBuffer { index, size: 0 };
            let prim_type = self.r.read_u32().ok_or_else(|| short.clone())?;
            let index_count = self.r.read_u32().ok_or_else(|| short.clone())?;
            let index_bits = self.r.read_u32().ok_or_else(|| short.clone())?;
            let buf_size = self.r.read_u32().ok_or_else(|| short.clone())?;
            let bytes = self
                .r
                .take(buf_size as usize)
                .ok_or(GmdlError::TruncatedIndexBuffer {
                    index,
                    size: buf_size,
                })?
                .to_vec();
            self.model.index_buffers.push(GmdlIndexBuffer {
                prim_type,
                index_count,
                index_bits,
                bytes,
            });
        }
        Ok(())
    }

    fn read_descriptors(&mut self) -> Result<(), GmdlError> {
        let available = self.r.remaining();
        let count = self
            .r
            .read_u32()
            .ok_or(GmdlError::TruncatedVertexDescriptorTable {
                // The count word itself is missing: nothing was declared, and the
                // four bytes it needed are what is missing.
                count: 0,
                needed: 4,
                available,
            })?;
        let needed = u64::from(count) * 4;
        let available = self.r.remaining();
        if needed > available as u64 {
            return Err(GmdlError::TruncatedVertexDescriptorTable {
                count,
                needed,
                available,
            });
        }
        for index in 0..count as usize {
            let elements = self
                .r
                .read_u32()
                .ok_or(GmdlError::TruncatedVertexDescriptor { index, count: 0 })?;
            let available = self.r.remaining();
            if u64::from(elements) * VERTEX_ELEMENT_SIZE > available as u64 {
                return Err(GmdlError::TruncatedVertexDescriptor {
                    index,
                    count: elements,
                });
            }
            let short = || GmdlError::TruncatedVertexDescriptor {
                index,
                count: elements,
            };
            let mut descriptor = Vec::with_capacity(elements as usize);
            for _ in 0..elements {
                let stream = self.r.read_u16().ok_or_else(short)?;
                let offset = self.r.read_u16().ok_or_else(short)?;
                let decl_type = self.r.read_u8().ok_or_else(short)?;
                let decl_method = self.r.read_u8().ok_or_else(short)?;
                let decl_usage = self.r.read_u8().ok_or_else(short)?;
                let usage_index = self.r.read_u8().ok_or_else(short)?;
                let type_code = self.r.read_u32().ok_or_else(short)?;
                // An undocumented declType has no size, so the descriptor has no
                // stride, so every vertex after the first would be read from the
                // wrong offset. Refuse the record instead.
                if crate::DeclType::from_code(decl_type).is_none() {
                    return Err(GmdlError::UndocumentedDeclType {
                        code: decl_type,
                        descriptor_index: Some(index),
                    });
                }
                descriptor.push(GmdlVertexElement {
                    stream,
                    offset,
                    decl_type,
                    decl_method,
                    decl_usage,
                    usage_index,
                    type_code,
                });
            }
            self.model.descriptors.push(descriptor);
        }
        Ok(())
    }

    fn read_vertex_buffers(&mut self) -> Result<(), GmdlError> {
        let available = self.r.remaining();
        let count = self
            .r
            .read_u32()
            .ok_or(GmdlError::TruncatedVertexBufferTable {
                // The count word itself is missing: nothing was declared, and the
                // four bytes it needed are what is missing.
                count: 0,
                needed: 4,
                available,
            })?;
        let needed = u64::from(count) * VERTEX_BUFFER_HEADER_SIZE;
        let available = self.r.remaining();
        if needed > available as u64 {
            return Err(GmdlError::TruncatedVertexBufferTable {
                count,
                needed,
                available,
            });
        }
        for index in 0..count as usize {
            let short = |size: u32| GmdlError::TruncatedVertexBuffer { index, size };
            let desc_index = self.r.read_u32().ok_or_else(|| short(0))?;
            let vertex_count = self.r.read_u32().ok_or_else(|| short(0))?;
            let buf_size = self.r.read_u32().ok_or_else(|| short(0))?;
            let bytes = self
                .r
                .take(buf_size as usize)
                .ok_or(short(buf_size))?
                .to_vec();
            // Checked before the buffer is stored, so a parsed model never holds
            // a vertex buffer that cannot be located in the descriptor table.
            if desc_index as usize >= self.model.descriptors.len() {
                return Err(GmdlError::BadDescriptorIndex {
                    index,
                    desc_index,
                    desc_count: self.model.descriptors.len(),
                });
            }
            self.model.vertex_buffers.push(GmdlVertexBuffer {
                desc_index,
                vertex_count,
                bytes,
            });
        }
        Ok(())
    }

    fn read_mesh_table(&mut self) -> Result<(), GmdlError> {
        // Bytes this section cannot do without: 12 per mesh (an 8-byte buffer
        // reference and a 4-byte material id), plus the 4-byte observed zero word
        // that follows them, plus the 4-byte material-info count word - without
        // which the *next* section cannot even start, so it belongs here rather
        // than in a second, later failure.
        //
        // The C++ reference checks `meshCount * 16`, which is 12 per mesh plus one
        // zero word. The two agree for every record whose trailer is complete (one
        // needs at least `12 * mesh_count + 12` bytes here) and differ only for a
        // record whose trailer was cut short: the reference's threshold is the
        // higher of the two, so it refuses such a record outright as a mesh-table
        // failure where this walk accepts the geometry and reports the short tail
        // through `trailer`. That is the strict/best-effort boundary this crate
        // documents, applied consistently.
        let mesh_count = self.model.mesh_count;
        let needed = u64::from(mesh_count) * 12 + 8;
        let available = self.r.remaining();
        if needed > available as u64 {
            return Err(GmdlError::TruncatedMeshTable {
                mesh_count,
                needed,
                available,
            });
        }
        let short = || GmdlError::TruncatedMeshTable {
            mesh_count,
            needed,
            available,
        };
        for mesh in 0..mesh_count as usize {
            let index_buffer = self.r.read_u32().ok_or_else(short)?;
            let vertex_buffer = self.r.read_u32().ok_or_else(short)?;
            if index_buffer as usize >= self.model.index_buffers.len()
                || vertex_buffer as usize >= self.model.vertex_buffers.len()
            {
                return Err(GmdlError::MeshReferencesMissingBuffer {
                    mesh,
                    index_buffer,
                    vertex_buffer,
                    index_buffers: self.model.index_buffers.len(),
                    vertex_buffers: self.model.vertex_buffers.len(),
                });
            }
            self.model.meshes.push(GmdlMeshRef {
                index_buffer,
                vertex_buffer,
            });
        }
        for _ in 0..mesh_count {
            let material_id = self.r.read_u32().ok_or_else(short)?;
            self.model.material_ids.push(material_id);
        }
        // A four-byte word observed to be zero on every record this build has
        // decoded. It is skipped, not read, because its meaning is unknown and a
        // zero is not evidence of a meaning.
        if !self.r.skip(4) {
            return Err(short());
        }
        Ok(())
    }

    fn read_material_info(&mut self) -> Result<(), GmdlError> {
        let available = self.r.remaining();
        let count = self
            .r
            .read_u32()
            .ok_or(GmdlError::TruncatedMaterialInfoTable {
                // The count word itself is missing: nothing was declared, and the
                // four bytes it needed are what is missing.
                count: 0,
                needed: 4,
                available,
            })?;
        let needed = u64::from(count) * 4;
        let available = self.r.remaining();
        if needed > available as u64 {
            return Err(GmdlError::TruncatedMaterialInfoTable {
                count,
                needed,
                available,
            });
        }
        for index in 0..count as usize {
            let entries = self
                .r
                .read_u32()
                .ok_or(GmdlError::TruncatedMaterialInfo { index, count: 0 })?;
            let available = self.r.remaining();
            if u64::from(entries) * 4 > available as u64 {
                return Err(GmdlError::TruncatedMaterialInfo {
                    index,
                    count: entries,
                });
            }
            let short = || GmdlError::TruncatedMaterialInfo {
                index,
                count: entries,
            };
            for _ in 0..entries {
                let id = self.r.read_u32().ok_or_else(short)?;
                if id == crate::TEXTURE_SET_ID {
                    self.read_texture_set()?;
                } else {
                    // No documented size means no way to skip: stopping is the
                    // only option that cannot desynchronise the sections after
                    // this one.
                    let size = crate::tables::shader_data_size(id)
                        .ok_or(GmdlError::UndocumentedShaderDataId { id })?;
                    self.r
                        .take(size as usize)
                        .ok_or(GmdlError::TruncatedShaderData { id, size })?;
                }
            }
        }
        let available = self.r.remaining();
        if available < 4 {
            return Err(GmdlError::TruncatedTrailer { available });
        }
        Ok(())
    }

    /// A `0x20D` texture set: a count, then that many 24-byte entries. Each entry
    /// is 16 bytes of `{sampler, opaque}` followed by the referenced texture's
    /// `instance` and `group`.
    fn read_texture_set(&mut self) -> Result<(), GmdlError> {
        let count = self.r.read_u32().ok_or(GmdlError::TruncatedTextureSet {
            count: 0,
            available: self.r.remaining(),
        })?;
        let available = self.r.remaining();
        if u64::from(count) * TEXTURE_ENTRY_SIZE > available as u64 {
            return Err(GmdlError::TruncatedTextureSet { count, available });
        }
        let short = || GmdlError::TruncatedTextureSet { count, available };
        for _ in 0..count {
            // `{sampler, opaque}`: the sampler id would let a material be bound
            // to a specific sampler stage, and the 12 opaque bytes are not
            // understood. Both are skipped rather than guessed at.
            if !self.r.skip(16) {
                return Err(short());
            }
            let instance_id = self.r.read_u32().ok_or_else(short)?;
            let group_id = self.r.read_u32().ok_or_else(short)?;
            self.model.texture_refs.push(GmdlTextureRef {
                instance_id,
                group_id,
            });
        }
        Ok(())
    }

    /// The best-effort tail: bone ranges, anim data, baked deforms, trailing key.
    ///
    /// Nothing in here can fail the parse. Every step is bounds-checked, and the
    /// first one that runs out of record stops the walk and is recorded in
    /// [`Walk::trailer`] so a caller can tell a short tail from an absent one.
    fn read_trailer(&mut self) {
        // Bone ranges: `{start, count}` pairs.
        let Some(bone_count) = self.r.read_u32() else {
            self.stop_trailer(TrailerStage::BoneRangeCount);
            return;
        };
        if u64::from(bone_count) * BONE_RANGE_SIZE > self.r.remaining() as u64 {
            self.stop_trailer(TrailerStage::BoneRanges);
            return;
        }
        for _ in 0..bone_count {
            let (Some(start), Some(count)) = (self.r.read_u32(), self.r.read_u32()) else {
                self.stop_trailer(TrailerStage::BoneRanges);
                return;
            };
            self.model.bone_ranges.push((start, count));
        }

        // Anim data: two baked transforms, a flags word and a resource key, then
        // a baked-deform array.
        let Some(anim_count) = self.r.read_u32() else {
            self.stop_trailer(TrailerStage::AnimDataCount);
            return;
        };
        for _ in 0..anim_count {
            if !self.r.skip(ANIM_DATA_PREFIX_SIZE as usize) {
                self.stop_trailer(TrailerStage::AnimData);
                return;
            }
            let Some(baked) = self.r.read_u32() else {
                self.stop_trailer(TrailerStage::BakedDeform);
                return;
            };
            if u64::from(baked) * BAKED_DEFORM_SIZE > self.r.remaining() as u64 {
                self.stop_trailer(TrailerStage::BakedDeform);
                return;
            }
            if !self.r.skip(baked as usize * BAKED_DEFORM_SIZE as usize) {
                self.stop_trailer(TrailerStage::BakedDeform);
                return;
            }
        }

        // The trailing key. Assigned only when all three words were read, so a
        // zero here never looks like an observed zero.
        let mut key = [0u32; 3];
        for slot in &mut key {
            let Some(word) = self.r.read_u32() else {
                self.stop_trailer(TrailerStage::UnknownKey);
                return;
            };
            *slot = word;
        }
        self.model.unknown_key = key;
    }

    fn stop_trailer(&mut self, at: TrailerStage) {
        self.trailer = TrailerWalk::Truncated { at };
    }

    fn finish(mut self, input_len: usize) -> GmdlModel {
        self.model.trailer = self.trailer;
        self.model.strict_consumed = self.r.position();
        // `consumed` is the input length, unconditionally. This crate does not
        // decode a prefix of a record and hand it back as if it were the record;
        // what it did or did not *validate* is `strict_consumed` and `trailer`.
        self.model.consumed = input_len;
        self.model
    }
}

/// Names the bounding-box component a read failed on, for the error message.
fn bounds_component(box_index: usize, component: usize) -> &'static str {
    match (box_index, component) {
        (0, 0) => "bboxMin[0]",
        (0, 1) => "bboxMin[1]",
        (0, 2) => "bboxMin[2]",
        (1, 0) => "bboxMax[0]",
        (1, 1) => "bboxMax[1]",
        (1, 2) => "bboxMax[2]",
        // Unreachable: both box indices are 0 or 1 and both components are
        // 0..3. Named explicitly rather than indexed so the compiler checks it.
        _ => "bounds",
    }
}

/// Writes one component of a 3-vector.
fn set_component(target: &mut [f32; 3], component: usize, value: f32) {
    if let Some(slot) = target.get_mut(component) {
        *slot = value;
    }
}
