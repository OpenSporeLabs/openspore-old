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
//! source — `spore-dbpf`'s `index.rs`, `spore-texture`'s `framing.rs` and
//! `dxt5.rs`, `spore-gmdl`'s `parse.rs` — and not from a call into the decoder.
//! A builder that shared code with the reader would round-trip perfectly while
//! both were wrong together, which is the failure mode `spore-gmdl`'s own test
//! support is written to avoid.
//!
//! # The raster fixtures are BC3, and one chain
//!
//! **This section was rewritten on 2026-10-05.** The previous DXT5 fixture built
//! **8-byte blocks with 16-byte layer headers**, which is exactly the layout the
//! corpus refutes:
//!
//! ```text
//! record_size == 32 + base_level_bytes
//!                     + SUM over m in 2..mip_count of (4 + level_m_bytes)
//! ```
//!
//! 16 bytes per 4x4 block, `field_18` = the base level's own byte count, a `u32`
//! length before every level from **2** on, **no** layer headers, and **one**
//! chain. 2 954 of 2 954 records in `Spore_Content.package` satisfy exactly that
//! identity, and the old fixture satisfied none of it.
//!
//! A fixture built from the refuted layout cannot test anything downstream of
//! the refutation: it produced records the real decoder refuses, so every role
//! assertion in this crate was failing for the wrong reason.
//!
//! # The realism that matters
//!
//! The gmdl builder defaults to the *documented asset*'s material section,
//! measured with `osptool` on `SPORE/Data/Spore_Content.package` (2026-10-04):
//! one material id `0x407DFDDB`, three texture references with
//! `instance_id = 0x067a07f0` and groups `0x40632900`, `0x40632901`,
//! `0x40632902` — all three of which **decode**.

#![allow(dead_code)]

use spore_assets::{ContentStore, Package};
use spore_core::ResourceKey;
use spore_gmdl::GmdlTextureRef;
use spore_texture::{DXT5_FOURCC, RASTER_TYPE};

/// The fourcc of the uncompressed normal-map family.
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
/// The `Spore_Content.package` of the installed game, or `None` when it is not
/// there.
///
/// The path is resolved against the workspace root rather than the crate
/// directory: `cargo test` runs a test with its own crate as the working
/// directory, so a bare relative path is a coin flip.
pub fn content_store() -> Option<ContentStore> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../SPORE/Data/Spore_Content.package");
    if !path.exists() {
        println!("SKIP: {} is not installed", path.display());
        return None;
    }
    let mut store = ContentStore::new();
    store.push(Package::open("Spore_Content", path).expect("Spore_Content.package opens"));
    Some(store)
}

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

/// Bytes in the raster envelope of the two block formats.
const ENVELOPE_SIZE: usize = 32;

/// Bytes in one 4x4 **BC3** block.
const BLOCK_BYTES: usize = 16;

/// The first mip level the block formats prefix with its own `u32` byte count.
///
/// Levels 0 and 1 are written bare. This is `spore-texture`'s measured rule, not
/// a choice: a `u32` where a level-1 prefix would sit in a DXT5 record holds
/// block data. **Unknown** why the `0x15` family differs (it prefixes level 1) —
/// see `spore_texture::claims::FIRST_PREFIXED_LEVEL`.
const FIRST_PREFIXED_LEVEL: u32 = 2;

/// Texels in one 4x4 block.
const TEXELS_PER_BLOCK: usize = 16;

/// Packs an 8-bit RGB triple into a `R5G6B5` colour word.
///
/// Red in bits 11–15, green in bits 5–10, blue in bits 0–4, all little-endian on
/// disk. Quantisation is `(v * n_max + 127) / 255`, i.e. round-to-nearest of the
/// channel's own width; the *decoder* expands with bit replication, which is a
/// different function, and that asymmetry is exactly why the fixture stores the
/// words and lets the decoder expand them rather than predicting texels here.
pub fn r5g6b5(r: u8, g: u8, b: u8) -> u16 {
    let scale = |value: u8, max: u16| (u16::from(value) * max + 127) / 255;
    // Red 5 bits at 11..15, green 6 at 5..10, blue 5 at 0..4. The maximum is
    // `0xF800 | 0x07E0 | 0x001F = 0xFFFF`, so the shifts cannot overflow.
    (scale(r, 31) << 11) | (scale(g, 63) << 5) | scale(b, 31)
}

/// Builds one 16-byte BC3 block from its two endpoint pairs and its index
/// streams.
///
/// ```text
/// b[0]      alpha0     u8
/// b[1]      alpha1     u8
/// b[2..8]   16 three-bit alpha indices, texel i at bits 3*i of a 48-bit LE word
/// b[8..10]  colour0    R5G6B5 little-endian
/// b[10..12] colour1    R5G6B5 little-endian
/// b[12..16] 16 two-bit colour indices, texel i at bits 2*i of a 32-bit LE word
/// ```
///
/// The two index streams are **independent** — one structural difference from
/// the 8-byte "Spore block" codec this fixture replaces, which drove both
/// channels from a single 2-bit stream.
pub fn bc3_block(
    alpha0: u8,
    alpha1: u8,
    colour0: u16,
    colour1: u16,
    alpha_codes: [u8; TEXELS_PER_BLOCK],
    colour_codes: [u8; TEXELS_PER_BLOCK],
) -> [u8; BLOCK_BYTES] {
    let mut bytes = [0u8; BLOCK_BYTES];
    bytes[0] = alpha0;
    bytes[1] = alpha1;
    let mut alpha_word: u64 = 0;
    for (texel, code) in alpha_codes.iter().enumerate() {
        alpha_word |= u64::from(code & 0x7) << (3 * texel);
    }
    bytes[2..8].copy_from_slice(&alpha_word.to_le_bytes()[..6]);
    bytes[8..10].copy_from_slice(&colour0.to_le_bytes());
    bytes[10..12].copy_from_slice(&colour1.to_le_bytes());
    let mut colour_word: u32 = 0;
    for (texel, code) in colour_codes.iter().enumerate() {
        colour_word |= u32::from(code & 0x3) << (2 * texel);
    }
    bytes[12..16].copy_from_slice(&colour_word.to_le_bytes());
    bytes
}

/// The block `colour_block` builds for `k`: every channel moves as `k` moves.
///
/// This is what makes a synthetic `DXT5` record look like a real one **for the
/// role check**, and the reason is a corpus measurement rather than taste: over
/// all 1 747 `DXT5` records of `Spore_Content.package` each of lanes 0–2 takes
/// at least **32** distinct values and none takes fewer. A fixture whose colour
/// lanes were flat would be refused by the signature, and the resulting role
/// would be `Unidentified` for a reason that has nothing to do with the code
/// under test.
///
/// `alpha0` is **scrambled** across the block index rather than rising with it,
/// and the alpha indices walk the eight-level ramp inside each block. The
/// fourth plane is therefore not a monotone function of the colour, which is the
/// one axis on which the corpus is emphatic: over all 1 747 real records the mean
/// colour-to-alpha correlation has median **0.058** and maximum 0.539. It is still
/// *published* rather than constant — the 46 corpus records whose fourth plane is
/// constant are covered separately by [`flat_alpha_raster`].
pub fn colour_block(k: u8) -> [u8; BLOCK_BYTES] {
    let t = u32::from(k.min(15));
    // Every colour channel **rises** with the block index, and the three lanes
    // rise together with small fixed offsets between them. That is the axis on
    // which realism is load-bearing for a fixture: over the real corpus the mean
    // mutual correlation of the three colour lanes is **0.510**, which is the
    // measurement behind "these are one colour image". A fixture whose three
    // lanes moved independently would decode to something no Spore record
    // contains, and any statistic taken from it would be a statistic of the
    // fixture.
    //
    // The two colour endpoints are far apart so the two blend entries spread the
    // lanes: an 8x8 record is four blocks, and with endpoints four apart it would
    // take fewer than eight distinct values in a lane and be refused by the
    // signature for a reason that has nothing to do with the code under test.
    let channel = |v: u32| (v & 0xFF) as u8;
    let base = t * 15;
    bc3_block(
        channel(40 + ((t * 7) % 16) * 14),
        0,
        r5g6b5(channel(base), channel(base + 8), channel(base + 16)),
        r5g6b5(channel(base + 60), channel(base + 68), channel(base + 76)),
        core::array::from_fn(|i| (i as u8).wrapping_add(k) % 8),
        core::array::from_fn(|i| (i as u8).wrapping_add(k) & 3),
    )
}

/// A `DXT5` record whose **every** block decodes to a constant, all-zero fourth
/// plane, over a moving colour field.
///
/// The measured shape of the 46 records of `Spore_Content.package` that hold a
/// constant fourth plane. It exists so the role signature is tested against the
/// case that must NOT be refused: a requirement of "the mask plane varies" would
/// refuse all 46, and this is the fixture that proves it does not.
pub fn flat_alpha_raster(width: u32, height: u32, mip_count: u32) -> Vec<u8> {
    let mut out = raster_envelope(width, height, mip_count, DXT5_FOURCC);
    for mip in 0..mip_count {
        if mip >= FIRST_PREFIXED_LEVEL {
            push_u32(
                &mut out,
                u32::try_from(level_block_bytes(width, height, mip)).expect("a level fits in u32"),
            );
        }
        for index in 0..block_count(width, height, mip) {
            // alpha0 == alpha1 == 0: the BC4 ramp is then all zeros for every
            // index including the six the specification leaves undefined, so the
            // fourth lane is constant whatever the index stream says.
            let mut block = colour_block(index as u8);
            block[0] = 0;
            block[1] = 0;
            out.extend_from_slice(&block);
        }
    }
    out
}

/// The block `addressing_block` builds for `k`, as one 16-byte BC3 block.
///
/// **An addressing probe, not a realistic texture.** Texel `t` carries colour
/// index `(k >> t) & 1` and alpha index `(k >> t) & 1`, so the sixteen blocks
/// `k = 0..16` are distinguishable by value and a decode that read the wrong
/// level, chain or block index produces visibly wrong texels rather than
/// plausible ones. The colours are the extremes of the word so the two palette
/// entries are far apart.
///
/// Used by [`raster_header`], whose whole job is to be refused. The realistic
/// path is [`colour_block`].
pub fn addressing_block(k: u8) -> [u8; BLOCK_BYTES] {
    bc3_block(
        255,
        0,
        r5g6b5(255, 255, 255),
        r5g6b5(0, 0, 0),
        core::array::from_fn(|t| addressing_code(k, t)),
        core::array::from_fn(|t| addressing_code(k, t)),
    )
}

/// Texel `t` of [`addressing_block`] carries colour code `(k >> t) & 1`.
///
/// The shift is done on a `u32`: `t` runs to 15 and shifting a `u8` by 8 or more
/// is an overflow the debug build refuses, which would turn the absurd-header
/// tests into a panic instead of the typed refusal they exist to produce.
const fn addressing_code(k: u8, t: usize) -> u8 {
    ((k as u32 >> (t as u32)) & 1) as u8
}

/// Bytes one mip of `width` x `height` occupies as BC3 blocks.
///
/// Saturating, because the absurd-header fixtures declare geometries no builder
/// could materialize and an overflowing `usize` here would turn a refusal test
/// into a panic.
fn level_block_bytes(width: u32, height: u32, mip: u32) -> usize {
    block_count(width, height, mip).saturating_mul(BLOCK_BYTES)
}

/// How many 4x4 blocks one mip of `width` x `height` occupies.
fn block_count(width: u32, height: u32, mip: u32) -> usize {
    let w = (width >> mip).max(1);
    let h = (height >> mip).max(1);
    (w as usize)
        .div_ceil(4)
        .saturating_mul((h as usize).div_ceil(4))
}

/// The largest record [`raster_record`] will materialize.
///
/// A test-authoring bound, not a format rule: building a record whose declared
/// geometry is `0xFFFFFFFF` square would mean writing four billion blocks, so
/// the absurd-header tests use [`raster_header`] instead, which writes only the
/// bytes a decoder needs in order to *refuse* them.
pub const MAX_SYNTHETIC_RASTER_BYTES: u32 = 1 << 20;

/// Writes a 32-byte envelope for `width` x `height` x `mip_count`.
///
/// `field_18` is written as the base level's own byte count where that fits in a
/// `u32`, because the decoder cross-checks it. It is saturated rather than
/// refused so an absurd header is still materializable by [`raster_header`].
fn raster_envelope(width: u32, height: u32, mip_count: u32, fourcc: u32) -> Vec<u8> {
    let mut out = Vec::with_capacity(ENVELOPE_SIZE);
    push_u32(&mut out, 1); // version: observed 1 in every record, never validated
    push_u32(&mut out, width);
    push_u32(&mut out, height);
    push_u32(&mut out, mip_count);
    push_u32(&mut out, 8); // field_10: observed constant, never read by any decoder
    push_u32(&mut out, fourcc);
    let base = u32::try_from(level_block_bytes(width, height, 0)).unwrap_or(u32::MAX);
    push_u32(&mut out, base); // field_18: the base level's own byte count
    push_u32(&mut out, 0x0000_FFFF); // field_1c: read in DXT5/DXT1, payload in 0x15
    debug_assert_eq!(out.len(), ENVELOPE_SIZE);
    out
}

/// Writes a 32-byte envelope plus `payload_blocks` BC3 blocks of the base level.
///
/// Enough for the decoders to read every envelope word and then refuse the
/// record on its own terms — a wrong `fourcc`, a zero dimension, a record length
/// its own geometry does not imply — without materializing a payload the header
/// claims. This is how an absurd header is tested without building the absurd
/// record.
pub fn raster_header(
    width: u32,
    height: u32,
    mip_count: u32,
    fourcc: u32,
    payload_blocks: usize,
) -> Vec<u8> {
    let mut out = raster_envelope(width, height, mip_count, fourcc);
    for index in 0..payload_blocks {
        out.extend_from_slice(&addressing_block((index % 16) as u8));
    }
    out
}

/// Builds a raster record of `width` x `height` with `mip_count` mips, in the
/// one chain every raster record in the corpus holds.
///
/// The layout is the measured framing identity, not the refuted one: a 32-byte
/// envelope whose `field_18` is the base level's own byte count, then level 0
/// **unprefixed**, level 1 **unprefixed**, and a little-endian `u32` byte count
/// in front of every level from [`FIRST_PREFIXED_LEVEL`] on. There are **no**
/// layer headers and there is exactly **one** chain — a claim this crate records
/// as a refutation, under the old subject, in `claims::LAYER_COUNT_IS_DERIVED`.
///
/// Every block is distinct per block index, so a decode that read the wrong level
/// or block would produce visibly wrong texels rather than plausible ones.
///
/// # Panics
///
/// Above [`MAX_SYNTHETIC_RASTER_BYTES`]. See that constant for why.
pub fn raster_record(width: u32, height: u32, mip_count: u32, fourcc: u32) -> Vec<u8> {
    let total = record_size(width, height, mip_count);
    assert!(
        total <= u64::from(MAX_SYNTHETIC_RASTER_BYTES),
        "a {width}x{height}x{mip_count} record would be {total} bytes; \
         use raster_header for a header that is only meant to be refused"
    );
    let mut out = raster_envelope(width, height, mip_count, fourcc);
    for mip in 0..mip_count {
        let bytes = level_block_bytes(width, height, mip);
        if mip >= FIRST_PREFIXED_LEVEL {
            push_u32(
                &mut out,
                u32::try_from(bytes).expect("a level below MAX_SYNTHETIC_RASTER_BYTES fits u32"),
            );
        }
        for index in 0..(bytes / BLOCK_BYTES) {
            // Distinct per (mip, block) so a mis-offset decode is observable by
            // value. Block 0 of level 0 is the only one reused, and it is reused
            // to give the base mip a predictable first texel.
            let block = if index == 0 && mip == 0 {
                colour_block(0)
            } else {
                colour_block((((mip + 1) * 11 + index as u32) % 16) as u8)
            };
            out.extend_from_slice(&block);
        }
    }
    debug_assert_eq!(
        out.len() as u64,
        record_size(width, height, mip_count),
        "the fixture must satisfy the framing identity the decoder verifies"
    );
    out
}

/// The record length the framing identity implies, saturating.
fn record_size(width: u32, height: u32, mip_count: u32) -> u64 {
    let mut total = ENVELOPE_SIZE as u64;
    for mip in 0..mip_count {
        if mip >= FIRST_PREFIXED_LEVEL {
            total = total.saturating_add(4);
        }
        total = total.saturating_add(level_block_bytes(width, height, mip) as u64);
    }
    total
}

/// A valid 16x16 DXT5 record with two mips, the smallest fixture this crate's
/// happy path uses.
///
/// **Renamed on 2026-10-05** from `two_layer_raster`, because there is one chain.
/// The old name asserted the refuted model and every call site would have kept
/// asserting it in prose.
pub fn dxt5_raster() -> Vec<u8> {
    raster_record(16, 16, 2, DXT5_FOURCC)
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
    let mut out = gmdl_through_material_ids(meshes, material_ids);
    push_u32(&mut out, 1); // material-info block count
    push_u32(&mut out, 1); // entries in this block
    match block {
        MaterialBlock::TextureSet(textures) => {
            push_texture_set(&mut out, &textures);
        }
        MaterialBlock::UndocumentedShader(id) => {
            // No payload: an id with no documented size has none to skip.
            push_u32(&mut out, id);
        }
    }
    push_complete_trailer(&mut out);
    out
}

/// Everything up to and including the per-mesh material ids: the version word,
/// the empty big-endian reference table, the bounds, one empty triangle-list
/// index buffer, one descriptor, one empty vertex buffer, the mesh table and the
/// material ids.
fn gmdl_through_material_ids(meshes: usize, material_ids: &[u32]) -> Vec<u8> {
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
    out
}

/// Writes one `0x20D` texture set: the id, the count, then that many entries.
fn push_texture_set(out: &mut Vec<u8>, textures: &[GmdlTextureRef]) {
    push_u32(out, TEXTURE_SET_ID);
    push_u32(out, textures.len() as u32);
    for (index, reference) in textures.iter().enumerate() {
        push_texture_entry(out, index as u32, *reference);
    }
}

/// The trailer a complete record ends with: no bone ranges, no anim data, the
/// three-word trailing key.
///
/// **Five** words, which is exactly what `spore_gmdl`'s trailer walk reads:
/// `bone_range_count`, `anim_data_count`, and the three-word key. A sixth word
/// leaves four bytes the walk never reads, which makes
/// `GmdlModel::fully_walked()` false on a record that in fact decoded
/// completely — the byte-level statement and the meaning-level one coming apart,
/// which is the trap this comment exists to stop a future edit from walking into.
fn push_complete_trailer(out: &mut Vec<u8>) {
    for _ in 0..5 {
        push_u32(out, 0);
    }
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
    push_complete_trailer(&mut out);
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

/// The shader-data id that opens every creature-model material-info block, and
/// its documented payload length.
const RENDER_PARAMS_ID: u32 = 0x210;
const RENDER_PARAMS_SIZE: usize = 20;

/// A gmdl record shaped like the documented creature
/// `0x00e6bce5:0x40627100:0x067c79d2`: `meshes` meshes, **`declared_blocks`
/// declared material-info blocks**, and a block 0 that is `0x210` (20 bytes) then
/// a `0x20D` texture set of `block0_refs` references then the undocumented
/// `0x218`.
///
/// That last entry is what makes this the shape the fix exists for: the walk
/// reads block 0's texture set, collects its references, and then **stops** —
/// so `declared_blocks - 1` blocks are never read and `model.texture_refs` is a
/// strict prefix of the record's texture references. With `meshes == 2` and two
/// references the division still comes out even, which is exactly the trap.
pub fn truncated_material_gmdl(
    meshes: usize,
    declared_blocks: usize,
    block0_refs: Vec<GmdlTextureRef>,
) -> Vec<u8> {
    let mut out = gmdl_through_material_ids(meshes, &vec![DOCUMENTED_MATERIAL_ID; meshes]);
    push_u32(&mut out, declared_blocks as u32);
    // Block 0: render parameters, the texture set, then the id that stops the
    // walk. The real record's order, verified against the package.
    push_u32(&mut out, 3); // entries in this block
    push_u32(&mut out, RENDER_PARAMS_ID);
    for word in 0..RENDER_PARAMS_SIZE / 4 {
        push_u32(&mut out, 0x2644_5C07 + word as u32);
    }
    push_texture_set(&mut out, &block0_refs);
    push_u32(&mut out, UNDOCUMENTED_SHADER_ID);
    // Blocks 1.. are never reached, but the record must be long enough that the
    // stop is the ONLY reason the walk ends: 4 bytes of trailer follow, so a
    // reader that somehow skipped `0x218` would find the record end rather than
    // running out of it.
    push_complete_trailer(&mut out);
    out
}

/// The two references the documented creature's block 0 contributes, with the
/// group ids it carries: low bytes `0` and `1`, which is what makes the partial
/// list's own ordinals contradict a division by two.
pub fn creature_block0_refs() -> Vec<GmdlTextureRef> {
    vec![
        GmdlTextureRef {
            instance_id: 0x067C_79D2,
            group_id: 0x4062_2900,
        },
        GmdlTextureRef {
            instance_id: 0x067C_79D2,
            group_id: 0x4062_2901,
        },
    ]
}
