//! Synthetic fixtures: a DBPF image, raster records, and gmdl records.
//!
//! **No committed raster or gmdl fixture is usable here.** The real records live
//! in the git-ignored `SPORE/` tree, so every byte below is built in the test.
//! The committed `tests/fixtures/mini.gmdl` belongs to `spore-gmdl`'s own
//! integration tests and says nothing about a *model* that carries texture
//! references, which is the only thing this crate consumes.
//!
//! # Written from the format, not from the readers
//!
//! Each builder is written from the layout as documented in the reader's own
//! source — `spore-dbpf`'s `index.rs`, `spore-texture`'s `raster.rs`,
//! `spore-gmdl`'s `parse.rs` — and not from a call into the decoder. A builder
//! that shared code with the reader would round-trip perfectly while both were
//! wrong together, which is the failure mode `spore-gmdl`'s own test support is
//! written to avoid.
//!
//! # The realism that matters
//!
//! The gmdl builder defaults to the *documented asset*'s material section,
//! measured with `osptool` on `SPORE/Data/Spore_Content.package` (2026-10-04):
//! one material id `0x407DFDDB`, three texture references with
//! `instance_id = 0x067a07f0` and groups `0x40632900`, `0x40632901`,
//! `0x40632902` — the third of which is a `0x15` luminance record this build
//! refuses.

#![allow(dead_code)]

use spore_assets::{ContentStore, Package};
use spore_core::ResourceKey;
use spore_gmdl::GmdlTextureRef;
use spore_texture::{DXT5_FOURCC, RASTER_TYPE};

/// The fourcc of the luminance family this build refuses.
///
/// The observed value is `0x00000015`, whose high byte is `0x00`; the family is
/// identified by `spore-texture` as `0x15xx`, so a synthetic record uses the
/// exact observed word rather than a tidier invention.
pub const LUMINANCE_FOURCC: u32 = 0x0000_0015;

/// The type id of a raw-PNG record.
///
/// Verified on a real record: `0x2F7D0004:0x40666200:0xd1b4bb56` begins
/// `89 50 4E 47 0D 0A 1A 0A` (`\x89PNG\r\n\x1a\n`) and continues into `IHDR`.
pub const PNG_TYPE: u32 = 0x2F7D_0004;

/// The type id of a RenderWare container.
///
/// Verified on a real record: `0x2F4E681B:0x41606100:0xcb032b45` begins
/// `52 57 34 77 33 32 00` (`RW4w32\0`).
pub const RW4_TYPE: u32 = 0x2F4E_681B;

/// The documented asset's material id.
pub const DOCUMENTED_MATERIAL_ID: u32 = 0x407D_FDDB;

/// The documented asset's instance id, shared by all three texture references.
pub const DOCUMENTED_INSTANCE_ID: u32 = 0x067A_07F0;

/// The documented asset's model key.
pub fn documented_model_key() -> ResourceKey {
    ResourceKey::new(spore_gmdl::GMDL_TYPE, 0x4063_7E03, DOCUMENTED_INSTANCE_ID)
}

/// The documented asset's three texture groups, in encounter order.
pub const DOCUMENTED_TEXTURE_GROUPS: [u32; 3] = [0x4063_2900, 0x4063_2901, 0x4063_2902];

/// The documented asset's three texture references, in encounter order.
pub fn documented_texture_refs() -> Vec<GmdlTextureRef> {
    DOCUMENTED_TEXTURE_GROUPS
        .iter()
        .map(|group_id| GmdlTextureRef {
            instance_id: DOCUMENTED_INSTANCE_ID,
            group_id: *group_id,
        })
        .collect()
}

/// Appends a little-endian `u32`.
pub fn push_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

// ---------------------------------------------------------------------------
// DBPF
// ---------------------------------------------------------------------------

/// Bytes in the fixed DBPF v3 header.
const HEADER_SIZE: usize = 96;

/// Byte offset of the index entry count inside the header.
const OFF_INDEX_COUNT: usize = 0x24;

/// Byte offset of the index block offset inside the header.
const OFF_INDEX_OFFSET: usize = 0x40;

/// Row stride with neither the type nor the group shared: 28 bytes.
const ROW_SIZE_PLAIN: usize = 28;

/// The stored-size flag bit, masked off by the reader.
const SIZE_MASK: u32 = 0x7FFF_FFFF;

/// Builds a DBPF v3 image holding `records`, uncompressed.
///
/// Every row carries its own type and group ids (`flags == 0`, the plain 28-byte
/// layout) so the reader's shared-id branches are not in play, and every stored
/// size has bit 31 set — which is how `tests/fixtures/gen_fixtures.py` writes a
/// row and what proves the reader masks the flag off.
pub fn dbpf_package(records: &[(ResourceKey, Vec<u8>)]) -> Vec<u8> {
    let index_offset = HEADER_SIZE;
    // flags word + one row per record, all before the first payload.
    let payloads_at = index_offset + 4 + records.len() * ROW_SIZE_PLAIN;

    let mut image = vec![0u8; index_offset];
    image[0..4].copy_from_slice(b"DBPF");
    image[4..8].copy_from_slice(&3u32.to_le_bytes()); // major: never validated
    image[OFF_INDEX_COUNT..OFF_INDEX_COUNT + 4]
        .copy_from_slice(&(records.len() as u32).to_le_bytes());
    image[OFF_INDEX_OFFSET..OFF_INDEX_OFFSET + 4]
        .copy_from_slice(&(index_offset as u32).to_le_bytes());
    push_u32(&mut image, 0); // index flags: nothing shared

    // The whole index is written before any payload, so a record's extent can
    // never overlap the rows that describe it.
    let mut offset = payloads_at;
    for (key, payload) in records {
        push_u32(&mut image, key.type_id);
        push_u32(&mut image, key.group_id);
        push_u32(&mut image, key.instance_id);
        push_u32(&mut image, offset as u32);
        push_u32(&mut image, payload.len() as u32 | !SIZE_MASK);
        push_u32(&mut image, payload.len() as u32); // memory size == stored size
        image.extend_from_slice(&0u16.to_le_bytes()); // compression: stored
        image.push(0); // saved
        image.push(0xAA); // pad, whose value must not matter
                          // Payloads are word-aligned so the next record's extent stays inside the
                          // image; the format does not require the alignment.
        offset += (payload.len() + 3) & !3;
    }
    debug_assert_eq!(
        image.len(),
        payloads_at,
        "the index must end exactly where the first payload begins"
    );
    image.resize(offset, 0);

    let mut at = payloads_at;
    for (_, payload) in records {
        image
            .get_mut(at..at + payload.len())
            .expect("payload extent inside the image")
            .copy_from_slice(payload);
        at += (payload.len() + 3) & !3;
    }
    image
}

/// A one-package [`ContentStore`] over `records`.
pub fn store_over(records: &[(ResourceKey, Vec<u8>)]) -> ContentStore {
    let mut store = ContentStore::new();
    store.push(Package::from_vec("synthetic", dbpf_package(records)).expect("synthetic DBPF"));
    store
}

/// The raster record key for a group and instance.
pub fn raster_key(group_id: u32, instance_id: u32) -> ResourceKey {
    ResourceKey::new(RASTER_TYPE, group_id, instance_id)
}

// ---------------------------------------------------------------------------
// Raster records
// ---------------------------------------------------------------------------

/// Bytes in the raster envelope.
const ENVELOPE_SIZE: usize = 32;

/// Bytes in one layer header, which is skipped and never interpreted.
const LAYER_HEADER_SIZE: usize = 16;

/// Bytes in one 4x4 block.
const BLOCK_BYTES: usize = 8;

/// The block `addressing_block` builds for `k`, as one 8-byte DXT5 block.
///
/// `a0 = 0 < a1 = 255` (unreversed) and `c0 = 0x20 > c1 = 0x00`, so the
/// two-colour palette is `[(8,0,0), (0,0,0)]`: code 0 gives `(8,0,0)` with alpha
/// `round(2*255/8) = 64`, code 1 gives `(0,0,0)` with alpha 0. Sixteen distinct
/// blocks exist, one per bit pattern, which is what makes them usable as an
/// addressing probe.
pub fn addressing_block(k: u8) -> [u8; BLOCK_BYTES] {
    let mut bytes = [0u8; BLOCK_BYTES];
    bytes[0] = 0; // alpha0
    bytes[1] = 255; // alpha1
    bytes[2] = 0x20; // colour lo
    bytes[3] = 0x00; // colour hi
    for (texel, code) in codes_for(k).into_iter().enumerate() {
        bytes[4 + texel / 4] |= (code & 0x3) << (2 * (texel % 4));
    }
    bytes
}

/// Texel `t` of [`addressing_block`] carries code `(k >> t) & 1`.
fn codes_for(k: u8) -> [u8; 16] {
    core::array::from_fn(|t| ((u32::from(k) >> t) & 1) as u8)
}

/// Bytes one mip of `width` x `height` occupies as DXT5 blocks.
///
/// Saturating for the same reason as [`chain_bytes`].
fn mip_block_bytes(width: u32, height: u32) -> usize {
    let blocks_wide = (width.max(1) as usize).div_ceil(4);
    let blocks_high = (height.max(1) as usize).div_ceil(4);
    blocks_wide
        .saturating_mul(blocks_high)
        .saturating_mul(BLOCK_BYTES)
}

/// The mip chain's block bytes for `mip_count` levels, matching
/// `spore_texture::dxt5_chain_size` for mip counts below 32.
///
/// Saturating, because the same expression must be computable for the absurd
/// headers a refusal test builds: an overflowing `usize` here would turn a test
/// into a panic, which is the one outcome these tests exist to rule out.
fn chain_bytes(width: u32, height: u32, mip_count: u32) -> usize {
    (0..mip_count).fold(0usize, |total, mip| {
        let w = (width >> mip).max(1);
        let h = (height >> mip).max(1);
        total.saturating_add(mip_block_bytes(w, h))
    })
}

/// A DXT5 block that decodes to a flat field: every code 0 with the full ramp's
/// first entry, so the decoded texel is `(8, 0, 0, 64)`.
fn flat_block() -> [u8; BLOCK_BYTES] {
    addressing_block(0)
}

/// The largest record [`raster_record`] will materialize.
///
/// A test-authoring bound, not a format rule: building a record whose declared
/// geometry is `0xFFFFFFFF` square would mean writing four billion blocks, so
/// the absurd-header tests use [`raster_header`] instead, which writes only the
/// bytes a decoder needs in order to *refuse* them.
pub const MAX_SYNTHETIC_RASTER_BYTES: u32 = 1 << 20;

/// Writes a 32-byte envelope plus `payload_blocks` blocks of `mip 0`.
///
/// Enough for the decoders to read every envelope word and then refuse the
/// record on its own terms — a wrong `fourcc`, a zero dimension, a chain that
/// does not divide — without materializing a payload the header claims. This is
/// how an absurd header is tested without building the absurd record.
pub fn raster_header(
    width: u32,
    height: u32,
    mip_count: u32,
    fourcc: u32,
    payload_blocks: usize,
) -> Vec<u8> {
    let mut out = Vec::new();
    push_u32(&mut out, 1);
    push_u32(&mut out, width);
    push_u32(&mut out, height);
    push_u32(&mut out, mip_count);
    push_u32(&mut out, 8);
    push_u32(&mut out, fourcc);
    push_u32(&mut out, 0x0004_0000);
    push_u32(&mut out, 0x0000_FFFF);
    // One layer header, which is skipped and never interpreted.
    out.extend_from_slice(b"L000");
    push_u32(&mut out, 0x1111_0000);
    push_u32(&mut out, 0x2222_0000);
    push_u32(&mut out, 0x3333_0000);
    for index in 0..payload_blocks {
        out.extend_from_slice(&addressing_block((index % 16) as u8));
    }
    out
}

/// Builds a raster record with `layers` layers of `width` x `height` and
/// `mip_count` mips each.
///
/// The layout is the format's own: 32-byte envelope, **all** layer headers, then
/// **all** layer payloads. Every payload block is distinct per layer and per
/// block index, so a decoder that read a layer from the wrong offset would
/// produce visibly wrong texels rather than plausible ones.
///
/// # Panics
///
/// Above [`MAX_SYNTHETIC_RASTER_BYTES`]. See that constant for why.
pub fn raster_record(
    width: u32,
    height: u32,
    mip_count: u32,
    fourcc: u32,
    layers: usize,
) -> Vec<u8> {
    let chain = chain_bytes(width, height, mip_count) as u64;
    let total = 32 + layers as u64 * (LAYER_HEADER_SIZE as u64 + chain);
    assert!(
        total <= u64::from(MAX_SYNTHETIC_RASTER_BYTES),
        "a {width}x{height}x{mip_count} record would be {total} bytes; \
         use raster_header for a header that is only meant to be refused"
    );
    let mut out = Vec::new();
    push_u32(&mut out, 1); // version: observed 1 in every record, never validated
    push_u32(&mut out, width);
    push_u32(&mut out, height);
    push_u32(&mut out, mip_count);
    push_u32(&mut out, 8); // field_10: observed constant, never read
    push_u32(&mut out, fourcc);
    push_u32(&mut out, 0x0004_0000); // field_18: observed constant, never read
    push_u32(&mut out, 0x0000_FFFF); // field_1c

    for layer in 0..layers {
        // A 4-byte name plus three words, all of them never interpreted. The
        // values differ per layer so a decoder that read them cannot look right
        // by accident.
        out.extend_from_slice(format!("L{layer:03}").as_bytes());
        push_u32(&mut out, 0x1111_0000 | layer as u32);
        push_u32(&mut out, 0x2222_0000 | layer as u32);
        push_u32(&mut out, 0x3333_0000 | layer as u32);
    }
    debug_assert_eq!(out.len(), ENVELOPE_SIZE + layers * LAYER_HEADER_SIZE);

    for layer in 0..layers {
        for mip in 0..mip_count {
            let w = (width >> mip).max(1);
            let h = (height >> mip).max(1);
            let block_count = mip_block_bytes(w, h) / BLOCK_BYTES;
            for index in 0..block_count {
                // Distinct per (layer, mip, block) so a mis-offset decode is
                // observable by value.
                let k = ((layer as u32 + 1) * 37 + (mip + 1) * 11 + index as u32) % 16;
                let block = if index == 0 {
                    flat_block()
                } else {
                    addressing_block(k as u8)
                };
                out.extend_from_slice(&block);
            }
        }
    }
    debug_assert_eq!(
        out.len() - ENVELOPE_SIZE - layers * LAYER_HEADER_SIZE,
        layers * chain_bytes(width, height, mip_count),
        "the payload must be exactly the layer stride the reader divides by"
    );
    out
}

/// A valid two-layer 8x8 DXT5 record with two mips, the smallest fixture this
/// crate's happy path uses.
pub fn two_layer_raster() -> Vec<u8> {
    raster_record(8, 8, 2, DXT5_FOURCC, 2)
}

/// A **real** raster record of the `0x15` family.
///
/// This used to be built with the DXT5 framing so that the decoder's refusal
/// could be asserted. That was wrong in a way the corpus corrected:
/// `spore-texture` established (901/901 records in `Spore_Content`) that the
/// `0x15` family has its own framing, and the tests were asserting a
/// specification that the real data refutes.
///
/// ```text
/// record_size == 28 + w*h*4 + sum_{m>=1}(4 + max(1,w>>m)*max(1,h>>m)*4)
/// ```
///
/// A 28-byte header, **no** layer headers, a `u32` byte-count prefix before
/// every mip past the base, and uncompressed 4-bytes-per-texel RGBA. So it is
/// built here from that identity rather than from the DXT5 layer walk.
pub fn luminance_raster(width: u32, height: u32) -> Vec<u8> {
    luminance_raster_with_mips(width, height, 2)
}

/// As [`luminance_raster`], with an explicit mip count.
pub fn luminance_raster_with_mips(width: u32, height: u32, mip_count: u32) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&1u32.to_le_bytes()); // 0x00 version
    out.extend_from_slice(&width.to_le_bytes()); // 0x04 width
    out.extend_from_slice(&height.to_le_bytes()); // 0x08 height
    out.extend_from_slice(&mip_count.to_le_bytes()); // 0x0c mip_count
    out.extend_from_slice(&8u32.to_le_bytes()); // 0x10 field_10
    out.extend_from_slice(&LUMINANCE_FOURCC.to_le_bytes()); // 0x14 fourcc = 0x15
                                                            // 0x18 field_18 is the BASE level's byte count (w*h*4), format-specific.
    out.extend_from_slice(&(width * height * 4).to_le_bytes());
    // 0x1c onwards is payload, not constants: the first texel begins here.
    for mip in 0..mip_count {
        let w = (width >> mip).max(1);
        let h = (height >> mip).max(1);
        let bytes = w * h * 4;
        if mip > 0 {
            out.extend_from_slice(&bytes.to_le_bytes());
        }
        // A flat tangent-space normal, Z FIRST: lane 0 = 0xFF, 1 = 0x80, 2 = 0x80.
        // Distinct per mip so a mis-offset decode is observable by value.
        let (r, g, b) = match mip {
            0 => (0xFFu8, 0x80, 0x80),
            1 => (0xC0u8, 0x90, 0xA0),
            _ => (0x11u8 * mip as u8, 0x22, 0x33),
        };
        for i in 0..(w * h) {
            let t = (i % 7) as u8;
            out.extend_from_slice(&[r.saturating_add(t), g, b, 0xFF]);
        }
    }
    debug_assert_eq!(
        out.len() as u64,
        28 + (width * height * 4) as u64 + {
            let mut acc = 0u64;
            for m in 1..mip_count {
                let w = (width >> m).max(1);
                let h = (height >> m).max(1);
                acc += 4 + (w * h * 4) as u64;
            }
            acc
        },
        "the fixture must satisfy the framing identity the decoder verifies"
    );
    out
}

/// A `0x15` record whose payload does NOT satisfy the framing identity, which
/// the decoder must refuse rather than pad, round or truncate.
pub fn malformed_luminance_raster(width: u32, height: u32) -> Vec<u8> {
    let mut bytes = luminance_raster(width, height);
    bytes.truncate(bytes.len() - 64); // drop a chunk of the last mip
    bytes
}

// ---------------------------------------------------------------------------
// gmdl records
// ---------------------------------------------------------------------------

/// The `0x20D` texture-set shader-data id.
const TEXTURE_SET_ID: u32 = 0x20D;

/// A shader-data id with **no** documented size: `spore-gmdl`'s table has a gap
/// there, so a record carrying it cannot be walked past.
const UNDOCUMENTED_SHADER_ID: u32 = 0x218;

/// What a gmdl material-info block holds.
#[derive(Debug, Clone)]
pub enum MaterialBlock {
    /// A `0x20D` texture set with these entries, in order.
    TextureSet(Vec<GmdlTextureRef>),
    /// A shader-data id with no documented payload size.
    UndocumentedShader(u32),
}

/// Writes the 16 skipped bytes of one texture-set entry, plus `{instance, group}`.
///
/// The 16 bytes are written with **distinct** values per entry so that a reader
/// which wrongly interpreted them could not agree with one that skips them, and
/// so a test can prove the instance/group pair was read from the right offset.
fn push_texture_entry(out: &mut Vec<u8>, index: u32, reference: GmdlTextureRef) {
    push_u32(out, 0x0100_0000 | index); // sampler: skipped, never interpreted
    for word in 0..3 {
        push_u32(out, 0xC0DE_0000 | (index << 8) | word);
    }
    push_u32(out, reference.instance_id);
    push_u32(out, reference.group_id);
}

/// Builds a gmdl record with `meshes` meshes (one material id each), one
/// material-info block holding `block`, and a complete trailer.
///
/// The geometry is the minimum `spore-gmdl` accepts: one empty triangle-list
/// index buffer, one descriptor with a single `POSITION/FLOAT3` element, and one
/// empty vertex buffer, all shared by every mesh.
pub fn gmdl_record(meshes: usize, material_ids: &[u32], block: MaterialBlock) -> Vec<u8> {
    let mut out = Vec::new();
    push_u32(&mut out, spore_gmdl::SUPPORTED_VERSION);
    // The one big-endian word in the format.
    out.extend_from_slice(&0u32.to_be_bytes()); // referenced-file count
    push_u32(&mut out, meshes as u32);
    for value in [0.0f32, 0.0, 0.0, 1.0, 1.0, 1.0] {
        out.extend_from_slice(&value.to_le_bytes());
    }
    out.extend_from_slice(&0.0f32.to_le_bytes()); // radius

    // Index buffers: one, prim = TRIANGLE_LIST, no indices, no payload.
    push_u32(&mut out, 1);
    push_u32(&mut out, spore_gmdl::PRIM_TRIANGLE_LIST);
    push_u32(&mut out, 0); // index count
    push_u32(&mut out, 16); // bits
    push_u32(&mut out, 0); // payload size

    // Descriptors: one, with a single POSITION/FLOAT3 element.
    push_u32(&mut out, 1);
    push_u32(&mut out, 1); // element count
    out.extend_from_slice(&0u16.to_le_bytes()); // stream
    out.extend_from_slice(&0u16.to_le_bytes()); // offset
    out.push(2); // decl type: Float3
    out.push(0); // method
    out.push(0); // usage: POSITION
    out.push(0); // usage index
    push_u32(&mut out, 0); // type code

    // Vertex buffers: one, empty.
    push_u32(&mut out, 1);
    push_u32(&mut out, 0); // descriptor index
    push_u32(&mut out, 0); // vertex count
    push_u32(&mut out, 0); // payload size

    for _ in 0..meshes {
        push_u32(&mut out, 0); // index buffer
        push_u32(&mut out, 0); // vertex buffer
    }
    for material_id in material_ids {
        push_u32(&mut out, *material_id);
    }
    push_u32(&mut out, 0); // the observed zero word

    push_u32(&mut out, 1); // material-info block count
    push_u32(&mut out, 1); // entries in this block
    match block {
        MaterialBlock::TextureSet(textures) => {
            push_u32(&mut out, TEXTURE_SET_ID);
            push_u32(&mut out, textures.len() as u32);
            for (index, reference) in textures.iter().enumerate() {
                push_texture_entry(&mut out, index as u32, *reference);
            }
        }
        MaterialBlock::UndocumentedShader(id) => {
            // No payload: an id with no documented size has none to skip.
            push_u32(&mut out, id);
        }
    }

    // Trailer: no bone ranges, no anim data, the three-word trailing key.
    push_u32(&mut out, 0);
    push_u32(&mut out, 0);
    push_u32(&mut out, 0);
    push_u32(&mut out, 0);
    push_u32(&mut out, 0);
    push_u32(&mut out, 0);
    out
}

/// A gmdl record shaped like the documented asset: one mesh, material id
/// `0x407DFDDB`, and a texture set of three references.
pub fn documented_gmdl() -> Vec<u8> {
    gmdl_record(
        1,
        &[DOCUMENTED_MATERIAL_ID],
        MaterialBlock::TextureSet(documented_texture_refs()),
    )
}

/// A gmdl record with `meshCount` 0: no buffers, no meshes, no material ids and
/// no material info.
///
/// The shape of the real record captured in
/// `tests/expected/real_gmdl_1006.json`. Three absences, none of them an error.
pub fn meshless_gmdl() -> Vec<u8> {
    let mut out = Vec::new();
    push_u32(&mut out, spore_gmdl::SUPPORTED_VERSION);
    out.extend_from_slice(&0u32.to_be_bytes()); // referenced-file count
    push_u32(&mut out, 0); // mesh count
    for value in [0.0f32, 0.0, 0.0, 1.0, 1.0, 1.0] {
        out.extend_from_slice(&value.to_le_bytes());
    }
    out.extend_from_slice(&0.0f32.to_le_bytes()); // radius
    push_u32(&mut out, 0); // index buffers
    push_u32(&mut out, 0); // descriptors
    push_u32(&mut out, 0); // vertex buffers
    push_u32(&mut out, 0); // the observed zero word
    push_u32(&mut out, 0); // material-info block count
    for _ in 0..6 {
        push_u32(&mut out, 0); // trailer: no bone ranges, no anim data, the key
    }
    out
}

/// A gmdl record whose material info names shader-data id `0x218`, which has no
/// documented payload size — so the walk stops before any texture reference is
/// read.
pub fn undocumented_shader_gmdl() -> Vec<u8> {
    gmdl_record(
        1,
        &[DOCUMENTED_MATERIAL_ID],
        MaterialBlock::UndocumentedShader(UNDOCUMENTED_SHADER_ID),
    )
}
