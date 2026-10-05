//! An **independent** re-derivation of the raster framing, for all three
//! formats, and the decoder held against it.
//!
//! # Why this file exists at all
//!
//! The framing itself is now **measured**: a header, the base level bare, a
//! little-endian `u32` byte count in front of every level past the family's
//! first prefixed one, and a record exactly as long as that implies. It holds on
//! 4386 of 4386 raster records in the two content packages
//! (`docs/RASTER-FRAMING.md` §2). The per-format differences that remain are
//! three numbers and one unknown:
//!
//! | `fourcc` | header | bytes per 4x4 block | first prefixed level |
//! |---|---|---|---|
//! | `DXT5` | 32 | 16 (BC3) | 2 |
//! | `DXT1` | 32 | 8 (BC1) | 2 |
//! | `0x15` | **28** | 4 per *texel* | **1** |
//!
//! *Why* the block formats leave level 1 bare is **UNKNOWN** and is recorded as
//! such rather than guessed; what is asserted here is the measurement.
//!
//! A measured framing still needs a second transcription of it, because
//! `tools/spore/raster/raster.py` — the differential reference — still derives a
//! layer count by dividing a payload by an **8-byte** block stride, which is the
//! rule the corpus refuted. So there is no Python left to check the Rust
//! against, and this file is where the missing one lives: the `ref_*` functions
//! below re-derive the framing from the measured table, in a file that never
//! calls `spore_texture::framing`, `luminance_record_size`,
//! `luminance_mip_size` or `luminance_dimension`.
//!
//! # What the corpus refuted, and what happened to the assertions
//!
//! The previous version of this file asserted a **two-layer** reading: a 16-byte
//! layer header in front of every layer, a derived layer count, `DXT1` refused
//! by name, and a payload that had to divide by `16 + chain`. Two errors in it
//! cancelled exactly — a 16-byte block charged at 8 bytes per block is two
//! half-pictures, and two of them make one picture — so it "worked" on 1374
//! records and refused 373. Every test below that touched layers now asserts the
//! **new** truth and says, in its own comment, which measurement replaced it.
//!
//! # The properties, which did not change
//!
//! The reason these tests exist is not the layer count. It is that a decoder
//! which is wrong *quietly* is worse than one that is wrong loudly, and the ways
//! to be wrong quietly are enumerable. All of them are still here:
//!
//! 1. **Any successful decode is consistent with the independently derived
//!    layout.** Every level is the exact bytes of the window this file gives it,
//!    the walk consumes the record to the byte, and the record is exactly as
//!    long as the identity says. (For the block formats the codec intervenes, so
//!    the pixels are checked as RGBA8 output of the window — which is still a
//!    claim about *which* bytes were read.)
//! 2. **Refusal is total.** Empty input, every truncation offset, every extension
//!    offset, every single-byte flip in the header classified per word, every
//!    corrupted length prefix, and absurd counts all return `Err` — never a
//!    panic, never a hang, never a partial image.
//! 3. **No format accepts another format's bytes.** DXT5 is 16 bytes per block
//!    under a 32-byte header, DXT1 is 8 under the same header, and `0x15` is 4
//!    per texel under a 28-byte one. Relabelling a record's format word must be
//!    refused, because the alternative is *plausible garbage*.
//! 4. **A record that is not exactly the framed size is refused with both
//!    numbers**, never rounded into a partial image and never "one layer, plus a
//!    remainder".
//!
//! # What is deliberately *not* asserted here
//!
//! * What the block bytes **mean**. `spore-texture` grades BC3 and BC1
//!   `Inferred`; a framing test that asserted a codec would be re-asserting the
//!   implementation.
//! * What the four lanes of the `0x15` family mean. That is `spore-material`'s
//!   measurement.
//! * That a level past 31 is walked correctly. It is not, and
//!   [`the_walk_past_level_32_disagrees_with_the_identity_it_reports`] pins that
//!   as a defect in the crate under test rather than hiding it.
//!
//! Hermetic: synthetic records only, no `SPORE/`, no Python, no network.

use spore_texture::{
    decode_raster, frame_for, layer_count_for, luminance_record_size, parse_envelope, FrameFormat,
    MipImage, RasterImage, TextureError, DXT1_FOURCC, DXT5_FOURCC, LUMINANCE_FOURCC,
};

// ---------------------------------------------------------------- the oracle

/// Bytes per texel of the uncompressed family.
const TEXEL: usize = 4;
/// Bytes per output texel.
const OUT_TEXEL: usize = 4;
/// Bytes in one 4x4 block.
const BLOCK: usize = 16;
/// Bytes in one 4x4 BC1 block.
const BC1_BLOCK: usize = 8;
/// Bytes in the block formats' envelope.
const DXT_HEADER: usize = 32;
/// Bytes in the `0x15` family's envelope.
const LUM_HEADER: usize = 28;
/// Bytes in the length prefix before a prefixed level.
const PREFIX: usize = 4;
/// The mip cut: every level at or beyond this index is 1x1.
const MIP_FLOOR: u32 = 32;

/// The three measured frames, written out here rather than imported.
///
/// `spore_texture::framing::FrameFormat` holds exactly these numbers, and
/// `the_crates_frame_table_equals_the_measured_one` asserts that it does. Taking
/// them from the crate instead would make every other assertion in this file a
/// comparison of the crate with itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RefFrame {
    /// The `fourcc` this frame describes.
    fourcc: u32,
    /// Bytes of header before the base level's payload.
    header: usize,
    /// Bytes per 4x4 block, or `None` for the family that has no blocks.
    block: Option<usize>,
    /// First level preceded by its own `u32` byte count.
    first_prefixed: u32,
}

/// `DXT5`: 32-byte header, 16-byte blocks, level 2 onwards prefixed.
const REF_DXT5: RefFrame = RefFrame {
    fourcc: DXT5_FOURCC,
    header: DXT_HEADER,
    block: Some(BLOCK),
    first_prefixed: 2,
};

/// `DXT1`: 32-byte header, 8-byte blocks, level 2 onwards prefixed.
const REF_DXT1: RefFrame = RefFrame {
    fourcc: DXT1_FOURCC,
    header: DXT_HEADER,
    block: Some(BC1_BLOCK),
    first_prefixed: 2,
};

/// `0x15`: 28-byte header, four bytes per texel, level 1 onwards prefixed.
const REF_LUM: RefFrame = RefFrame {
    fourcc: LUMINANCE_FOURCC,
    header: LUM_HEADER,
    block: None,
    first_prefixed: 1,
};

/// The three frames the corpus measures, in the order the tests use them.
const REF_FRAMES: [RefFrame; 3] = [REF_DXT5, REF_DXT1, REF_LUM];

/// A readable name, for failure messages.
fn frame_name(frame: RefFrame) -> String {
    match frame.fourcc {
        DXT5_FOURCC => "DXT5".to_owned(),
        DXT1_FOURCC => "DXT1".to_owned(),
        LUMINANCE_FOURCC => "0x15".to_owned(),
        other => format!("0x{other:08x}"),
    }
}

/// The mip cut, `max(1, base >> mip)`, written out rather than shared with the
/// crate, which is the whole point of this file.
fn ref_dim(base: u32, mip: u32) -> u32 {
    if mip >= MIP_FLOOR {
        1
    } else {
        (base >> mip).max(1)
    }
}

/// `ceil(n / 4)`, the block count along one axis.
fn ref_ceil4(n: u32) -> u32 {
    n / 4 + if n % 4 != 0 { 1 } else { 0 }
}

/// Payload bytes of one level, checked rather than saturating.
///
/// `None` means the geometry is not addressable on this machine, which is a
/// different answer from "a few bytes" and must not be confused with one.
fn ref_level_bytes(frame: RefFrame, width: u32, height: u32, mip: u32) -> Option<usize> {
    let w = ref_dim(width, mip) as usize;
    let h = ref_dim(height, mip) as usize;
    match frame.block {
        None => w.checked_mul(h)?.checked_mul(TEXEL),
        Some(block) => (ref_ceil4(ref_dim(width, mip)) as usize)
            .checked_mul(ref_ceil4(ref_dim(height, mip)) as usize)?
            .checked_mul(block),
    }
}

/// The record size the measured framing implies.
///
/// ```text
/// record_size == header + level0
///                   + SUM over m in first_prefixed..mip_count of (4 + level_m)
/// ```
///
/// There is no division anywhere in it. That is the whole difference from the
/// rule this file used to assert: the old identity *divided* a payload by
/// `16 + chain` and required the remainder to vanish.
fn ref_record_size(frame: RefFrame, width: u32, height: u32, mip_count: u32) -> Option<usize> {
    let mut total = frame.header;
    if mip_count == 0 {
        return Some(total);
    }
    let bounded = mip_count.min(MIP_FLOOR);
    for mip in 0..bounded {
        if mip >= frame.first_prefixed {
            total = total.checked_add(PREFIX)?;
        }
        total = total.checked_add(ref_level_bytes(frame, width, height, mip)?)?;
    }
    let extra = mip_count.saturating_sub(bounded) as usize;
    let each = PREFIX.checked_add(ref_level_bytes(frame, width, height, MIP_FLOOR)?)?;
    total.checked_add(extra.checked_mul(each)?)
}

/// `(offset, length)` of every level's payload, derived here.
fn ref_windows(frame: RefFrame, width: u32, height: u32, mip_count: u32) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut at = frame.header;
    for mip in 0..mip_count {
        if mip >= frame.first_prefixed {
            at += PREFIX;
        }
        let len = ref_level_bytes(frame, width, height, mip).unwrap_or(0);
        out.push((at, len));
        at += len;
    }
    out
}

/// Little-endian `u32` at `at`, or `None` past the end. The test's own header
/// reader: the crate's `parse_envelope` is the thing under test.
fn ref_u32(data: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(data.get(at..at + 4)?.try_into().ok()?))
}

/// The header as this file reads it.
fn ref_header(data: &[u8]) -> Option<(u32, u32, u32, u32)> {
    Some((
        ref_u32(data, 0x04)?, // width
        ref_u32(data, 0x08)?, // height
        ref_u32(data, 0x0C)?, // mip_count
        ref_u32(data, 0x14)?, // fourcc
    ))
}

// ------------------------------------------------------------- the builders

/// Pushes `count` bytes whose value is a function of the absolute offset, so
/// every window in the record is uniquely identifiable and a decoder that read
/// the wrong bytes cannot produce a matching image.
fn fill(out: &mut Vec<u8>, count: usize) {
    let start = out.len();
    for index in 0..count {
        out.push(((start + index) % 251) as u8);
    }
}

/// A well-formed record of `frame`'s shape.
///
/// `field_18` is the base level's own byte count — the cross-check the crate now
/// reads — and it is computed here from *this file's* level arithmetic, so a
/// builder that agreed with the crate by construction would prove nothing.
fn build(frame: RefFrame, width: u32, height: u32, mip_count: u32) -> Vec<u8> {
    let size = ref_record_size(frame, width, height, mip_count).expect("addressable shape");
    let mut out = Vec::with_capacity(size);
    out.extend_from_slice(&1u32.to_le_bytes()); // version
    out.extend_from_slice(&width.to_le_bytes());
    out.extend_from_slice(&height.to_le_bytes());
    out.extend_from_slice(&mip_count.to_le_bytes());
    out.extend_from_slice(&8u32.to_le_bytes()); // field_10: unresolved
    out.extend_from_slice(&frame.fourcc.to_le_bytes());
    let base = ref_level_bytes(frame, width, height, 0).expect("addressable base") as u32;
    out.extend_from_slice(&base.to_le_bytes()); // field_18
    if frame.header == DXT_HEADER {
        // 0x1c is a header word for the block formats and reads 0x0000FFFF on
        // every record of both families. Nothing reads it, which is the point of
        // the header-flip sweep.
        out.extend_from_slice(&0x0000_FFFFu32.to_le_bytes());
    }
    assert_eq!(
        out.len(),
        frame.header,
        "{}: the header must be {} bytes",
        frame_name(frame),
        frame.header
    );
    for mip in 0..mip_count {
        let len = ref_level_bytes(frame, width, height, mip).expect("addressable level");
        if mip >= frame.first_prefixed {
            out.extend_from_slice(&(len as u32).to_le_bytes());
        }
        fill(&mut out, len);
    }
    assert_eq!(
        out.len(),
        size,
        "{}: the builder must produce the identity's size",
        frame_name(frame)
    );
    out
}

/// A well-formed `0x15` record.
fn build_luminance(width: u32, height: u32, mip_count: u32) -> Vec<u8> {
    build(REF_LUM, width, height, mip_count)
}

/// A well-formed `DXT5` record under the measured framing.
///
/// The previous builder here wrote 16 bytes of "layer header" per layer and
/// repeated the chain; that shape is no longer a record of anything, and a
/// builder for it would make the sweep below test a format that does not exist.
fn build_dxt5(width: u32, height: u32, mip_count: u32) -> Vec<u8> {
    build(REF_DXT5, width, height, mip_count)
}

/// A well-formed `DXT1` record.
fn build_dxt1(width: u32, height: u32, mip_count: u32) -> Vec<u8> {
    build(REF_DXT1, width, height, mip_count)
}

// -------------------------------------------------- property 1: consistency

/// Holds a successful `0x15` decode to the independently derived framing.
///
/// This is the anti-silent-acceptance property, and it is deliberately written
/// against **this file's** offsets rather than the crate's: a decoder that read
/// a level from the wrong place, or that returned a short image, or that left
/// bytes unconsumed, cannot satisfy it. The family's codec is a copy, so the
/// pixels must equal the window byte for byte.
fn assert_luminance_decode_matches_the_independent_layout(record: &[u8], image: &RasterImage) {
    let (width, height, mip_count, fourcc) = ref_header(record).expect("a readable header");
    assert_eq!(fourcc, LUMINANCE_FOURCC);

    assert_eq!(
        record.len(),
        ref_record_size(REF_LUM, width, height, mip_count).expect("addressable"),
        "a decoded record must be exactly as long as the identity says"
    );
    assert_eq!(
        image.layers.len(),
        1,
        "the 0x15 family has no layer headers, so there is exactly one chain"
    );
    let windows = ref_windows(REF_LUM, width, height, mip_count);
    assert_eq!(
        image.layers[0].len(),
        windows.len(),
        "one decoded level per declared level"
    );
    assert_eq!(
        windows.last().map(|(at, len)| at + len),
        Some(record.len()),
        "the walk must end at the record end, with nothing unconsumed"
    );

    for (mip, (offset, len)) in windows.iter().enumerate() {
        let decoded: &MipImage = &image.layers[0][mip];
        assert_eq!(decoded.width, ref_dim(width, mip as u32), "mip {mip} width");
        assert_eq!(
            decoded.height,
            ref_dim(height, mip as u32),
            "mip {mip} height"
        );
        assert_eq!(decoded.pixels.len(), *len, "mip {mip} length");
        assert_eq!(
            &decoded.pixels,
            &record[*offset..*offset + *len],
            "mip {mip} must be the exact bytes at its own window; there is no codec"
        );
    }
}

/// The same property for a blocked format, where the codec intervenes.
///
/// What can still be asserted about *framing* is everything except the byte
/// equality: the record is exactly as long as the identity says, there is one
/// decoded level per declared level, each level has the dimensions and the
/// `width * height * 4` output length the geometry requires, and the walk ends
/// at the record end. Which bytes were read is asserted separately, by flipping
/// one byte inside a level's window and requiring texels to move.
fn assert_block_decode_matches_the_independent_layout(
    frame: RefFrame,
    record: &[u8],
    image: &RasterImage,
) {
    let (width, height, mip_count, fourcc) = ref_header(record).expect("a readable header");
    assert_eq!(fourcc, frame.fourcc);

    assert_eq!(
        record.len(),
        ref_record_size(frame, width, height, mip_count).expect("addressable"),
        "{}: a decoded record must be exactly as long as the identity says",
        frame_name(frame)
    );
    assert_eq!(
        image.layers.len(),
        1,
        "{}: exactly one chain",
        frame_name(frame)
    );
    let windows = ref_windows(frame, width, height, mip_count);
    assert_eq!(
        image.layers[0].len(),
        windows.len(),
        "{}: one decoded level per declared level",
        frame_name(frame)
    );
    assert_eq!(
        windows.last().map(|(at, len)| at + len),
        Some(record.len()),
        "{}: the walk must end at the record end, with nothing unconsumed",
        frame_name(frame)
    );

    for (mip, (offset, len)) in windows.iter().enumerate() {
        let decoded: &MipImage = &image.layers[0][mip];
        let level_width = ref_dim(width, mip as u32);
        let level_height = ref_dim(height, mip as u32);
        assert_eq!(
            decoded.width,
            level_width,
            "{} mip {mip} width",
            frame_name(frame)
        );
        assert_eq!(
            decoded.height,
            level_height,
            "{} mip {mip} height",
            frame_name(frame)
        );
        assert_eq!(
            decoded.pixels.len(),
            level_width as usize * level_height as usize * OUT_TEXEL,
            "{} mip {mip}: RGBA8 output is exactly width * height * 4",
            frame_name(frame)
        );
        // The window is inside the record, at the offset this file derived.
        assert!(
            *len == ref_level_bytes(frame, width, height, mip as u32).expect("addressable"),
            "{} mip {mip}: window length must be the geometry's, not the decoder's",
            frame_name(frame)
        );
        assert!(
            *offset + *len <= record.len(),
            "{} mip {mip}: window must lie inside the record",
            frame_name(frame)
        );
    }
}

/// Holds a successful decode to the independent layout, dispatching on format.
fn assert_decode_matches_the_independent_layout(
    frame: RefFrame,
    record: &[u8],
    image: &RasterImage,
) {
    if frame.block.is_none() {
        assert_luminance_decode_matches_the_independent_layout(record, image);
    } else {
        assert_block_decode_matches_the_independent_layout(frame, record, image);
    }
}

/// Requires that flipping bytes inside `level`'s window moves texels, which is
/// the only way to show the decode *read* those bytes rather than reproducing a
/// constant.
///
/// Two families, two strengths, and the difference is the codec:
///
/// * The `0x15` family's texels **are** the stored bytes, so any flip changes
///   them and the check is unconditional.
/// * A blocked level is decoded, so a flip can land on a bit the block does not
///   use for that level's texels -- on a 1x1 level, one block decides one texel
///   and several of its sixteen bytes do not reach it at all. So the requirement
///   is the weaker but still falsifiable one: of four flips spread across the
///   window, **at least one** must change the level, for any level wider than
///   one texel. A 1x1 level is skipped with the reason stated, rather than
///   asserted with a tolerance that would prove nothing.
fn assert_the_level_is_read(frame: RefFrame, record: &[u8], level: u32) {
    let (width, height, mip_count, _) = ref_header(record).expect("a readable header");
    let windows = ref_windows(frame, width, height, mip_count);
    let (offset, len) = windows[level as usize];
    assert!(len > 0, "level {level} has no bytes to flip");
    let before = decode_raster(record).expect("the unflipped record decodes");

    let probes: Vec<usize> = if frame.block.is_none() {
        vec![offset]
    } else {
        [0, len / 4, len / 2, len - 1]
            .into_iter()
            .map(|at| offset + at)
            .collect()
    };
    let mut moved = 0usize;
    for at in probes {
        let mut flipped = record.to_vec();
        flipped[at] ^= 0xFF;
        let after = decode_raster(&flipped).expect("a payload flip keeps the framing valid");
        if after.layers[0][level as usize] != before.layers[0][level as usize] {
            moved += 1;
        }
    }
    let texels = ref_dim(width, level) as usize * ref_dim(height, level) as usize;
    if texels == 1 && frame.block.is_some() {
        // One block, one texel: a BC3 block's bytes do not all reach it, so
        // "this level is read" is not a statement this flip can make. The
        // framing property it would have supported is asserted for every level
        // by the offset checks in `assert_block_decode_matches_...`.
        assert!(
            len >= 16 || len >= 8,
            "level {level} must hold at least one whole block"
        );
        return;
    }
    assert!(
        moved > 0,
        "{}: none of the flips inside level {level}'s own window (offset {offset}, {len} bytes, \
         {texels} texel(s)) changed a single texel -- a decode that ignored the payload would \
         pass every other check in this file",
        frame_name(frame)
    );
}

// --------------------------------------------------- property 2: total refusal

/// Every one of these must be a refusal, and none may panic.
///
/// The sweep is what makes "refuses" a claim about the whole input space around
/// a valid record rather than about three hand-picked inputs.
fn assert_all_refused(cases: &[(String, Vec<u8>)]) {
    for (what, bytes) in cases {
        let outcome = decode_raster(bytes);
        assert!(
            outcome.is_err(),
            "{what}: a {}-byte record decoded as Ok, which is silent acceptance",
            bytes.len()
        );
    }
}

// ------------------------------------------------------------------ the tests

/// Every shape the corpus measures, plus shapes that break the naive
/// assumptions: non-square, a single texel, and a mip count at and past the
/// point where `w >> mip` reaches zero.
fn shapes() -> Vec<(u32, u32, u32)> {
    let mut out = vec![
        // The 0x15 shapes measured in Spore_Content.package.
        (32, 32, 6),
        (256, 256, 9),
        (512, 512, 10),
        // The DXT5 and DXT1 shapes measured in the same package.
        (64, 64, 7),
    ];
    for (w, h) in [
        (1u32, 1u32),
        (1, 8),
        (8, 1),
        (2, 3),
        (3, 2),
        (4, 4),
        (5, 7),
        (7, 5),
    ] {
        for mips in [1u32, 2, 3, 4, 8, 12, 32] {
            out.push((w, h, mips));
        }
    }
    for mips in [1u32, 2, 5, 9, 10, 11, 32] {
        out.push((64, 64, mips));
        out.push((128, 64, mips));
        out.push((64, 128, mips));
    }
    out
}

/// Shapes whose `mip_count` is far enough past the shared `MIP_FLOOR` for the
/// walk's cap to cost it a whole level.
///
/// `MIP_FLOOR` is 32 and the cap only bites from `mip_count == 34` onwards: at
/// 33 the last level (32) still lands on its own window, because placing level
/// `m` needs the accumulation of levels `0..m`, and `min(33, 32) == 32` covers
/// levels 0..31 — exactly enough. One more level and level 32's own prefix and
/// 1x1 payload are missing from the sum.
///
/// No record in the corpus has more than ten levels, so these are not measured
/// shapes; they are the arithmetic's tail. They are handled by
/// [`the_walk_past_level_32_disagrees_with_the_identity_it_reports`] and kept
/// out of every "must be exact" sweep on purpose.
fn beyond_the_floor_shapes() -> Vec<(u32, u32, u32)> {
    let mut out = Vec::new();
    for (w, h) in [(1u32, 1u32), (4, 4), (8, 8)] {
        for mips in [34u32, 40] {
            out.push((w, h, mips));
        }
    }
    out
}

#[test]
fn the_crates_frame_table_equals_the_measured_one() {
    // The four numbers per format that the whole corpus turns on. Taking them
    // from the crate instead would make the rest of this file self-agreeing.
    for (reference, expected) in [
        (REF_DXT5, (DXT_HEADER, Some(BLOCK), 2u32)),
        (REF_DXT1, (DXT_HEADER, Some(BC1_BLOCK), 2)),
        (REF_LUM, (LUM_HEADER, None, 1)),
    ] {
        let frame: FrameFormat =
            frame_for(reference.fourcc).unwrap_or_else(|| panic!("no frame for the format"));
        assert_eq!(
            (
                frame.header_size,
                (frame.block_bytes != 0).then_some(frame.block_bytes),
                frame.first_prefixed_level
            ),
            expected,
            "{}: the crate's frame table must be the measured one",
            frame_name(reference)
        );
    }
    assert_eq!(frame_for(0xDEAD_BEEF), None, "absence is not a rule");
}

#[test]
fn the_crates_record_size_equals_an_independently_written_identity() {
    for frame in REF_FRAMES {
        for (w, h, mips) in shapes() {
            let mine = ref_record_size(frame, w, h, mips).expect("addressable shape");
            let theirs = spore_texture::framing::record_size(
                frame_for(frame.fourcc).expect("a frame"),
                w,
                h,
                mips,
            );
            assert_eq!(
                theirs,
                mine,
                "{} {w}x{h} mips={mips}: the crate's record size must equal the identity written \
                 out in this file",
                frame_name(frame)
            );
            if frame.fourcc == LUMINANCE_FOURCC {
                // The family keeps its own spelling of the same identity, and a
                // caller may use either.
                assert_eq!(
                    luminance_record_size(w, h, mips),
                    mine,
                    "0x15 {w}x{h} mips={mips}: luminance_record_size must agree with it too"
                );
            }
        }
    }
}

#[test]
fn an_unaddressable_geometry_saturates_rather_than_wrapping_to_a_small_number() {
    // The failure this guards is a wrapped multiply returning "a few bytes",
    // which a caller cannot distinguish from a small texture. Every one of these
    // geometries overflows `usize` on a 64-bit target.
    // (u32::MAX, 1) is deliberately NOT here: 4294967295 * 4 fits in a 64-bit
    // `usize`, so it is addressable and belongs to the case above, not here.
    for (w, h, mips) in [
        (u32::MAX, u32::MAX, 1u32),
        (u32::MAX, u32::MAX, 40),
        (0x8000_0000, 0x8000_0000, 2),
        (0x8000_0000, 0x8000_0000, 1),
    ] {
        assert_eq!(
            ref_record_size(REF_LUM, w, h, mips),
            None,
            "{w}x{h} mips={mips} must be unaddressable here, or this test proves nothing"
        );
        assert_eq!(
            luminance_record_size(w, h, mips),
            usize::MAX,
            "{w}x{h} mips={mips}: the crate must saturate, not wrap"
        );
    }
    // And a geometry that *is* addressable must not be mistaken for one that is
    // not: the boundary case is real data, not an error.
    assert!(ref_record_size(REF_LUM, 4096, 4096, 1).is_some());
}

#[test]
fn a_well_formed_record_decodes_to_exactly_the_bytes_that_were_written() {
    // The name is the property and it now covers all three formats. For the
    // `0x15` family it is literal (the codec is a copy); for the block formats
    // it is the framing plus the RGBA8 output shape plus, per level, proof that
    // the level's own bytes are the ones that were read.
    for frame in REF_FRAMES {
        for (w, h, mips) in shapes() {
            let record = build(frame, w, h, mips);
            let label = format!("{} {w}x{h} mips={mips}", frame_name(frame));
            let image = decode_raster(&record)
                .unwrap_or_else(|error| panic!("{label} must decode: {error}"));
            assert_eq!(image.envelope.mip_count, mips, "{label}");
            assert_eq!(image.envelope.width, w, "{label}");
            assert_eq!(image.envelope.height, h, "{label}");
            assert_eq!(image.envelope.fourcc, frame.fourcc, "{label}");
            assert_eq!(image.envelope.version, 1, "{label}");
            assert_eq!(image.envelope.field_10, 8, "{label}");
            assert_eq!(
                image.envelope.field_18,
                ref_level_bytes(frame, w, h, 0).expect("addressable") as u32,
                "{label}: field_18 restates the base level's size, and it is now READ as a \
                 cross-check rather than carried unread"
            );
            assert_eq!(image.envelope.mip_count as usize, mips as usize, "{label}");
            assert_eq!(
                layer_count_for(record.len(), &image.envelope),
                Ok(1),
                "{label}"
            );
            assert_decode_matches_the_independent_layout(frame, &record, &image);
            // Every level is read, at every mip that has bytes.
            for level in 0..mips {
                assert_the_level_is_read(frame, &record, level);
            }
        }
    }
}

#[test]
fn the_first_four_bytes_after_the_header_are_the_first_texel_not_a_field() {
    // The 28-byte header is one of the two structural differences from the block
    // formats, and `RasterEnvelope::field_1c` deliberately reports bytes that are
    // *not* a field of this format. If a future edit made the header 32 bytes,
    // the envelope would start reporting a padding word and this would still
    // pass — so the assertion is on what those four bytes ARE.
    let record = build_luminance(4, 4, 2);
    let image = decode_raster(&record).expect("a valid record decodes");
    let envelope_word = image.envelope.field_1c;
    assert_eq!(
        envelope_word,
        u32::from_le_bytes([record[0x1C], record[0x1D], record[0x1E], record[0x1F]]),
        "field_1c reports the bytes that are there"
    );
    let first_texel = &image.layers[0][0].pixels[..OUT_TEXEL];
    assert_eq!(
        first_texel,
        &record[LUM_HEADER..LUM_HEADER + OUT_TEXEL],
        "and those bytes are the first texel of the base level, not a header word"
    );
    // The first texel is a distinguishable pattern, so this is not a comparison
    // of two zeros.
    assert_ne!(first_texel, &[0, 0, 0, 0]);
}

#[test]
fn truncation_at_every_offset_is_refused_and_never_partially_accepted() {
    // 8x8 / 4 mips, 0x15: 28 + 256 + (4+64) + (4+16) + (4+4) = 380 bytes.
    let luminance = build_luminance(8, 8, 4);
    assert_eq!(luminance.len(), 380);
    assert!(decode_raster(&luminance).is_ok());

    // DXT5 8x8 / 4 mips: 32 + 64 + 16 + (4+16) + (4+16) + (4+16) = 152 bytes.
    // Two of the four levels are 16 bytes, one block each.
    let dxt5 = build_dxt5(8, 8, 4);
    assert_eq!(dxt5.len(), 152);
    assert!(decode_raster(&dxt5).is_ok());

    // DXT1 8x8 / 4 mips: 32 + 32 + 8 + (4+8) + (4+8) + (4+8) = 96 bytes -- the
    // same shape as the DXT5 record with every block half the size.
    let dxt1 = build_dxt1(8, 8, 4);
    assert_eq!(dxt1.len(), 96);
    assert!(decode_raster(&dxt1).is_ok());

    for (what, record) in [("0x15", &luminance), ("DXT5", &dxt5), ("DXT1", &dxt1)] {
        let cases: Vec<(String, Vec<u8>)> = (0..record.len())
            .map(|n| (format!("{what} truncated to {n}"), record[..n].to_vec()))
            .collect();
        assert_all_refused(&cases);
    }
}

#[test]
fn padding_a_record_is_refused_because_nothing_unconsumed_is_allowed() {
    // The mirror of the truncation sweep and the more dangerous half: a decoder
    // that only checked "enough bytes" would accept every one of these and hand
    // the caller an image built from bytes the record does not claim. Under the
    // old two-layer rule a padded record was the *interesting* case — the
    // payload had to divide by `16 + chain` — so this is the property that
    // replaced "the division refuses a remainder": the identity refuses ANY
    // difference, with both numbers named.
    for frame in REF_FRAMES {
        let record = build(frame, 8, 8, 4);
        let mut cases: Vec<(String, Vec<u8>)> = Vec::new();
        for extra in 1..=64usize {
            let mut padded = record.clone();
            padded.extend(std::iter::repeat_n(0u8, extra));
            cases.push((format!("{} padded by {extra}", frame_name(frame)), padded));
        }
        // And padding that looks like another level's prefix, which is the shape
        // a decoder that derived its own offsets would accept.
        for extra in [4usize, 8, 68, 72] {
            let mut padded = record.clone();
            padded.extend_from_slice(&256u32.to_le_bytes());
            padded.resize(record.len() + extra, 0);
            cases.push((
                format!("{} padded by a plausible {extra}", frame_name(frame)),
                padded,
            ));
        }
        assert_all_refused(&cases);
    }
}

#[test]
fn every_single_byte_flip_in_the_header_is_either_refused_or_changes_nothing_read() {
    // 8x8 / 4 mips, so every header word carries a value that a flip changes.
    // The classification per word is the assertion:
    //
    // | word        | flips to                       | because                          |
    // |-------------|--------------------------------|----------------------------------|
    // | `version`   | decodes, no texel moves        | nothing reads it                 |
    // | `field_10`  | decodes, no texel moves        | nothing reads it                 |
    // | width / height / mip_count | refused by name, exact-size | the record then fails its own length check |
    // | `fourcc`    | refused by name                | it selects the frame             |
    // | `field_18`  | **refused by name**            | it is READ as a cross-check      |
    // | `field_1c` (block formats) | decodes, no texel moves | nothing reads it        |
    //
    // The `field_18` row changed, and it is the row worth having: the old table
    // said "`field_18` decodes, no texel moves — the crate's rule is 'derived,
    // never read'", which was true then and is **false** now. A byte count over
    // 4386 of 4386 records established that `field_18` is the base level's own
    // size in all three formats, so the crate reads it and refuses a record whose
    // header contradicts its geometry. That is a *stronger* refusal than the old
    // one, and the sweep is what proves it cannot be mistaken for a silently
    // ignored word.
    for frame in REF_FRAMES {
        let (w, h, mips) = (8u32, 8u32, 4u32);
        let record = build(frame, w, h, mips);
        let baseline = decode_raster(&record).expect("the unflipped record decodes");
        let base_pixels = baseline.layers[0][0].pixels.clone();

        // 0x00 version, 0x04 width, 0x08 height, 0x0c mip_count, 0x10
        // field_10, 0x14 fourcc, 0x18 field_18, and for the block formats
        // 0x1c field_1c.
        const NEVER_READ: [usize; 3] = [0, 4, 7];
        const GEOMETRY: [usize; 3] = [1, 2, 3];
        const FORMAT_WORD: usize = 5;
        const BASE_SIZE_WORD: usize = 6;
        let words = frame.header / 4;

        let mut flips_per_word = vec![0usize; words];
        for byte in 0..frame.header {
            for pattern in [0x01u8, 0x80, 0xFF] {
                let mut flipped = record.clone();
                flipped[byte] ^= pattern;
                let word = byte / 4;
                flips_per_word[word] += 1;
                match decode_raster(&flipped) {
                    Err(error) => {
                        if word == FORMAT_WORD {
                            assert!(
                                matches!(error, TextureError::UnsupportedFourcc { .. }),
                                "{}: a flipped format word must be refused by name, got {error}",
                                frame_name(frame)
                            );
                        } else if word == BASE_SIZE_WORD {
                            assert!(
                                matches!(error, TextureError::Field18Mismatch { .. }),
                                "{}: field_18 is read as the base level's byte count, so a flip \
                                 there must be refused BY NAME as a disagreement between the \
                                 header and the geometry, got {error}",
                                frame_name(frame)
                            );
                        } else {
                            assert!(
                                GEOMETRY.contains(&word),
                                "{}: header word {word} (offset 0x{:02x}) must never be the reason \
                                 a record is refused, got {error}",
                                frame_name(frame),
                                word * 4
                            );
                            // Two named refusals are correct for a flipped
                            // geometry word, and which one fires depends on the
                            // flipped value rather than on which byte it was.
                            //
                            // A `width`/`height` flip can make the declared
                            // `mip_count` longer than the chain the dimensions
                            // reach, which is now `MipChainBeyondFloor` — the
                            // record asks this build to place more levels than it
                            // knows how to place, and that is refused before the
                            // size comparison rather than after it. A `mip_count`
                            // flip very far up is the same thing. Everything else
                            // fails the exact-size check, which is the check that
                            // a record is exactly as long as its own envelope
                            // implies.
                            assert!(
                                matches!(
                                    error,
                                    TextureError::RecordSizeMismatch { .. }
                                        | TextureError::MipChainBeyondFloor { .. }
                                ),
                                "{}: a flipped geometry word must fail the exact-size check or the \
                                 chain-length ceiling, got {error}",
                                frame_name(frame)
                            );
                        }
                    }
                    Ok(image) => {
                        assert!(
                            NEVER_READ.contains(&word),
                            "{}: header word {word} (offset 0x{:02x}) decoded, but only the \
                             never-read words may: it either selects the frame or is checked \
                             against it",
                            frame_name(frame),
                            word * 4
                        );
                        assert_decode_matches_the_independent_layout(frame, &flipped, &image);
                        assert_eq!(
                            image.layers[0][0].pixels,
                            base_pixels,
                            "{}: offset 0x{:02x} is never read, so a flip there must not move a \
                             texel",
                            frame_name(frame),
                            word * 4
                        );
                    }
                }
            }
        }
        for (word, count) in flips_per_word.iter().enumerate() {
            assert!(
                *count > 0,
                "{}: header word {word} was never flipped",
                frame_name(frame)
            );
        }

        // The four bytes just past a 28-byte header are the first texel, so a
        // flip there MUST move pixels. Without this the "28-byte header" claim
        // has no test. The block formats have no such window: their 32-byte
        // header is eight words, and word 7 was covered above.
        if frame.header == LUM_HEADER {
            for byte in LUM_HEADER..LUM_HEADER + 4 {
                let mut flipped = record.clone();
                flipped[byte] ^= 0xFF;
                let image = decode_raster(&flipped).expect("a texel flip keeps the framing valid");
                assert_decode_matches_the_independent_layout(frame, &flipped, &image);
                assert_ne!(
                    image.layers[0][0].pixels, base_pixels,
                    "offset 0x{byte:02x} is inside the base level's payload, so a flip must move \
                     pixels"
                );
            }
        }
    }
}

#[test]
fn every_length_prefix_must_state_its_own_mips_size() {
    // The prefix is the record's own statement about its framing past the base
    // level, so a decoder that ignored it would be discarding the only check
    // available. Each prefix is corrupted on its own, and each must be refused
    // **by name at its own mip**.
    //
    // Which levels carry a prefix is per-format and measured: from level 1 in
    // the `0x15` family, from level 2 in the block formats. The old version of
    // this file swept only `0x15`, so it never saw the block formats' bare
    // level 1 — see
    // `the_bytes_where_a_level_1_prefix_would_sit_are_block_data`.
    for frame in REF_FRAMES {
        let (w, h, mips) = (8u32, 8u32, 5u32);
        let record = build(frame, w, h, mips);
        let windows = ref_windows(frame, w, h, mips);
        assert_eq!(windows.len(), 5);

        let mut prefixed = 0usize;
        for (mip, &(payload_at, _)) in windows.iter().enumerate().skip(1) {
            if (mip as u32) < frame.first_prefixed {
                continue;
            }
            prefixed += 1;
            let prefix_at = payload_at - PREFIX;
            let correct = ref_level_bytes(frame, w, h, mip as u32).expect("addressable") as u32;
            for declared in [0u32, 1, correct.wrapping_sub(1), correct + 4, u32::MAX] {
                let mut broken = record.clone();
                broken[prefix_at..prefix_at + PREFIX].copy_from_slice(&declared.to_le_bytes());
                match decode_raster(&broken) {
                    Ok(image) => panic!(
                        "{}: mip {mip} prefix {declared} (correct {correct}) decoded as Ok; \
                         first level {} bytes",
                        frame_name(frame),
                        image.layers[0][0].pixels.len()
                    ),
                    Err(TextureError::LevelLengthPrefix { mip: reported, .. }) => assert_eq!(
                        reported,
                        mip as u32,
                        "{}: the refusal must name the mip whose prefix disagrees",
                        frame_name(frame)
                    ),
                    Err(other) => panic!(
                        "{}: mip {mip} prefix {declared}: expected LevelLengthPrefix, got {other}",
                        frame_name(frame)
                    ),
                }
            }
        }
        assert!(prefixed > 0, "{}: no level was prefixed", frame_name(frame));
    }
}

#[test]
fn the_bytes_where_a_level_1_prefix_would_sit_are_block_data() {
    // The per-format asymmetry the corpus settled and the reason for is
    // UNKNOWN: the block formats leave level 1 bare, and the `u32` sitting where
    // a level-1 prefix would sit is block data. Asserting *that* is measured;
    // asserting *why* would be a guess.
    //
    // This is also the reason the prefix sweep above starts where it does. If a
    // decoder ever grew a level-1 prefix for the block formats, this test would
    // fail with a `LevelLengthPrefix` at mip 1 rather than passing quietly.
    for frame in [REF_DXT5, REF_DXT1] {
        let (w, h, mips) = (8u32, 8u32, 5u32);
        let record = build(frame, w, h, mips);
        let windows = ref_windows(frame, w, h, mips);
        let level1_at = windows[1].0;
        let would_be = ref_level_bytes(frame, w, h, 1).expect("addressable") as u32;
        let there = ref_u32(&record, level1_at).expect("in range");
        assert_ne!(
            there,
            would_be,
            "{}: the u32 at level 1's payload offset must NOT be level 1's size, or this \
             builder is not writing the measured framing",
            frame_name(frame)
        );
        // And corrupting those four bytes is a payload edit: it changes texels
        // and it is not refused as a prefix disagreement.
        let mut flipped = record.clone();
        flipped[level1_at..level1_at + PREFIX].copy_from_slice(&u32::MAX.to_le_bytes());
        let image = decode_raster(&flipped).unwrap_or_else(|error| {
            panic!(
                "{}: a payload edit must not be refused: {error}",
                frame_name(frame)
            )
        });
        assert_ne!(
            image.layers[0][1].pixels,
            decode_raster(&record).expect("decodes").layers[0][1].pixels,
            "{}: corrupting level 1's payload must change level 1's texels",
            frame_name(frame)
        );
        // The `0x15` family is the opposite case, and it is why the two sweeps
        // above start at different levels.
        assert_eq!(
            REF_LUM.first_prefixed, 1,
            "the uncompressed family prefixes from level 1, unlike the block formats"
        );
    }
}

#[test]
fn a_prefix_that_agrees_cannot_rescue_a_record_that_is_too_short() {
    // The two checks are independent and both must hold. A decoder that trusted
    // the prefix over the geometry would decode this; one that trusted the
    // geometry over the prefix would decode the previous test's case.
    for frame in REF_FRAMES {
        let (w, h, mips) = (8u32, 8u32, 4u32);
        let record = build(frame, w, h, mips);
        let windows = ref_windows(frame, w, h, mips);
        let last = windows[mips as usize - 1];
        let mut short = record[..last.0 + last.1 - 1].to_vec();
        // Make the last prefix agree with what is left, so only the size check
        // can catch it.
        let prefix_at = last.0 - PREFIX;
        short[prefix_at..prefix_at + PREFIX].copy_from_slice(&3u32.to_le_bytes());
        assert!(
            matches!(
                decode_raster(&short),
                Err(TextureError::RecordSizeMismatch { .. })
            ),
            "{}: a short record with an agreeing prefix must still fail the exact-size check",
            frame_name(frame)
        );
    }
}

#[test]
fn absurd_counts_are_refused_before_anything_is_allocated_or_walked() {
    // `mip_count = 0xFFFFFFFF` is the dangerous one: the walk is
    // `for mip in 0..mip_count`, so a decoder that reached the walk before
    // checking the framing would spin for four billion iterations. The exact-size
    // check is what makes this cheap, and this test is the only thing that says
    // so. If it ever hangs here, that check moved.
    let mut cases: Vec<(String, Vec<u8>)> = Vec::new();
    for (w, h, mips) in [
        (u32::MAX, u32::MAX, 1u32),
        (u32::MAX, u32::MAX, u32::MAX),
        (1, 1, u32::MAX),
        (8, 8, u32::MAX),
        (0x8000_0000, 8, u32::MAX),
    ] {
        for frame in REF_FRAMES {
            let mut record = build(frame, 8, 8, 1);
            record[0x04..0x08].copy_from_slice(&w.to_le_bytes());
            record[0x08..0x0C].copy_from_slice(&h.to_le_bytes());
            record[0x0C..0x10].copy_from_slice(&mips.to_le_bytes());
            cases.push((format!("{} {w}x{h} mips={mips}", frame_name(frame)), record));
        }
    }
    // Zero dimensions and a zero mip count, each refused by name.
    for frame in REF_FRAMES {
        for (what, patch) in [
            ("width 0", (0x04usize, 0u32)),
            ("height 0", (0x08, 0)),
            ("mips 0", (0x0C, 0)),
        ] {
            let mut record = build(frame, 8, 8, 4);
            record[patch.0..patch.0 + 4].copy_from_slice(&patch.1.to_le_bytes());
            cases.push((format!("{} {what}", frame_name(frame)), record));
        }
    }
    assert_all_refused(&cases);

    // And the refusals that must be *named*, so a caller can tell a malformed
    // header from an absent decoder.
    let mut zero_width = build_luminance(8, 8, 4);
    zero_width[0x04..0x08].copy_from_slice(&0u32.to_le_bytes());
    assert!(matches!(
        decode_raster(&zero_width),
        Err(TextureError::ZeroDimension {
            width: 0,
            height: 8
        })
    ));
    let mut zero_mips = build_luminance(8, 8, 4);
    zero_mips[0x0C..0x10].copy_from_slice(&0u32.to_le_bytes());
    assert!(matches!(
        decode_raster(&zero_mips),
        Err(TextureError::ZeroMipCount)
    ));
}

#[test]
fn a_mutation_sweep_never_produces_an_inconsistent_success() {
    // The generalisation of the sweeps above: deterministic mutations of a valid
    // record, and the single rule that every one of them must obey. Mutations
    // that leave the record decodable must decode to the independently derived
    // layout; mutations that do not must be refused. There is no third
    // possibility, and no assertion about *which* mutations land where, because
    // that is not a property worth pinning.
    let mut state = 0x2545_F491_4F6C_DD1Du64;
    let mut next = move || {
        // xorshift64*, so the sweep is identical on every machine and run.
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };

    let mut total_successes = 0usize;
    let mut total_refusals = 0usize;
    for frame in REF_FRAMES {
        let record = build(frame, 16, 16, 5);
        let mut successes = 0usize;
        let mut refusals = 0usize;
        for _ in 0..3_000 {
            let mut mutated = record.clone();
            match next() % 4 {
                0 => {
                    // Truncate.
                    let cut = (next() as usize) % mutated.len();
                    mutated.truncate(cut);
                }
                1 => {
                    // Extend.
                    let extra = (next() as usize) % 17;
                    mutated.extend(std::iter::repeat_n(0u8, extra));
                }
                2 => {
                    // Flip a byte anywhere, header included.
                    let at = (next() as usize) % mutated.len();
                    mutated[at] ^= (next() % 255 + 1) as u8;
                }
                _ => {
                    // Flip between one and four bytes, which is what a
                    // decompressor that lost sync actually produces.
                    let count = 1 + (next() % 4) as usize;
                    for _ in 0..count {
                        let at = (next() as usize) % mutated.len();
                        mutated[at] ^= (next() % 255 + 1) as u8;
                    }
                }
            }
            match decode_raster(&mutated) {
                Ok(image) => {
                    successes += 1;
                    let (_, _, _, fourcc) = ref_header(&mutated)
                        .expect("a record that decoded must carry a readable header");
                    assert_eq!(
                        fourcc,
                        frame.fourcc,
                        "{}: a record of another format decoded",
                        frame_name(frame)
                    );
                    assert_decode_matches_the_independent_layout(frame, &mutated, &image);
                }
                Err(_) => refusals += 1,
            }
        }
        // The sweep is only meaningful if it exercised both arms.
        assert!(
            successes > 0,
            "{}: no mutation decoded; the sweep proved nothing",
            frame_name(frame)
        );
        assert!(
            refusals > 0,
            "{}: no mutation was refused; the sweep proved nothing",
            frame_name(frame)
        );
        println!(
            "{}: mutation sweep: {successes} decoded, {refusals} refused, 0 inconsistent",
            frame_name(frame)
        );
        total_successes += successes;
        total_refusals += refusals;
    }
    println!("total: {total_successes} decoded, {total_refusals} refused");
}

#[test]
fn the_formats_do_not_accept_each_others_bytes() {
    // The failure the format documentation names explicitly: three formats with
    // three different framings (32-byte header at 16 bytes per block, 32 at 8,
    // 28 at four per texel), so feeding one to another's decoder would produce
    // *plausible garbage* rather than an error. Only a test that relabels a
    // record's format word can tell the difference.
    //
    // The old version of this test paired `0x15` with `DXT5` and asserted that
    // `DXT5` was refused *by name as a format it does not decode*. Both halves
    // of that are now false: `DXT5` decodes, and what refuses these records is
    // the length identity, because relabelling does not move a byte.
    for (w, h, mips) in [(8u32, 8u32, 4u32), (64, 64, 7), (256, 256, 9)] {
        for frame in REF_FRAMES {
            let record = build(frame, w, h, mips);
            assert!(
                decode_raster(&record).is_ok(),
                "{} {w}x{h}: the builder must produce a record its own decoder accepts",
                frame_name(frame)
            );
            for other in REF_FRAMES {
                if other.fourcc == frame.fourcc {
                    continue;
                }
                let mut relabelled = record.clone();
                relabelled[0x14..0x18].copy_from_slice(&other.fourcc.to_le_bytes());
                // The two identities differ for every pair and every shape here,
                // which is what makes the refusal a *length* refusal rather than
                // a lucky accident.
                assert_ne!(
                    ref_record_size(frame, w, h, mips),
                    ref_record_size(other, w, h, mips),
                    "{w}x{h}: {} and {} must not share one record size, or this test is vacuous",
                    frame_name(frame),
                    frame_name(other)
                );
                match decode_raster(&relabelled) {
                    Ok(_) => panic!(
                        "{w}x{h} mips={mips}: a {} record relabelled {} decoded",
                        frame_name(frame),
                        frame_name(other)
                    ),
                    Err(error) => assert!(
                        matches!(
                            error,
                            TextureError::RecordSizeMismatch { .. }
                                | TextureError::Field18Mismatch { .. }
                        ),
                        "{w}x{h} mips={mips}: {} bytes labelled {} must be refused by the length \
                         identity or the field_18 cross-check, got {error}",
                        frame_name(frame),
                        frame_name(other)
                    ),
                }
            }
        }
    }
}

#[test]
fn dxt1_decodes_and_every_other_format_word_is_refused_by_name() {
    // **This test used to assert the opposite.** `0x31545844` is present in the
    // corpus (306 records of `Spore_Content.package`) and was deliberately not
    // decoded, on the grounds that "a decoder that accepted any fourcc it did
    // not recognise would silently mis-frame them". The corpus refuted the
    // reason: a `DXT1` record frames exactly like a `DXT5` record with 8 bytes
    // per block instead of 16, all 306 decode, and `field_18` agrees with the
    // base level's byte count in every one of them. What the old test actually
    // protected against — a mis-framed record decoding as plausible garbage —
    // is now protected by the length identity, which refuses a `DXT1` record
    // labelled `DXT5` and vice versa
    // (`the_formats_do_not_accept_each_others_bytes`).
    let record = build_dxt1(8, 8, 4);
    let image = decode_raster(&record).expect("a well-framed DXT1 record decodes");
    assert_eq!(image.envelope.fourcc, DXT1_FOURCC);
    assert_eq!(image.layers.len(), 1);
    assert_eq!(image.layers[0].len(), 4, "one level per declared level");
    assert_eq!(
        image.layers[0][0].pixels.len(),
        8 * 8 * OUT_TEXEL,
        "BC1 still produces RGBA8 output of width * height * 4"
    );
    // 8 bytes per block: an 8x8 base level is 2*2*8 = 32 bytes, and that is
    // what field_18 states. At the 8-byte-block stride that once refuted DXT5,
    // this record would be 32 bytes of base level; the DXT5 record of the same
    // shape is 64. One byte, twice.
    assert_eq!(record.len(), 96);
    assert_eq!(image.envelope.field_18, 32);

    // Everything else is still refused by name. The list includes the `0x15xx`
    // window the old test guessed at, which the corpus shows is not the family:
    // the real format word is the single value `0x00000015`.
    for fourcc in [
        0u32,
        1,
        0xDEAD_BEEF,
        0x1500,
        0x15FF,
        0x1500_0000,
        0x3554_5843,
    ] {
        let mut record = build_luminance(8, 8, 4);
        record[0x14..0x18].copy_from_slice(&fourcc.to_le_bytes());
        match decode_raster(&record) {
            Ok(_) => panic!("fourcc 0x{fourcc:08x} decoded"),
            Err(error) => assert!(
                matches!(error, TextureError::UnsupportedFourcc { .. }),
                "fourcc 0x{fourcc:08x}: expected UnsupportedFourcc, got {error}"
            ),
        }
        // And the envelope still parses, because reading a header is not
        // decoding: the refusal must be about the format, not about the bytes.
        let envelope = parse_envelope(&record).expect("32 bytes is a readable header");
        assert_eq!(envelope.fourcc, fourcc);
    }
}

#[test]
fn an_empty_or_tiny_input_is_refused_rather_than_read_past() {
    for length in 0..LUM_HEADER {
        assert!(
            decode_raster(&vec![0u8; length]).is_err(),
            "{length} bytes must be refused"
        );
        assert!(matches!(
            parse_envelope(&vec![0u8; length]),
            Err(TextureError::TruncatedEnvelope { .. })
        ));
    }
    // A header with a valid-looking geometry but no payload at all. Note which
    // refusal this is: `parse_envelope` reads eight words and therefore needs 32
    // bytes, so a 28-byte record is refused as a **truncated envelope** even
    // though this format's header is 28. The consequence is that the smallest
    // decodable `0x15` record is 32 bytes -- a 1x1 base level with one mip --
    // and that is a fact about the reader, not about the format.
    let mut header_only = build_luminance(8, 8, 4);
    header_only.truncate(LUM_HEADER);
    assert!(matches!(
        decode_raster(&header_only),
        Err(TextureError::TruncatedEnvelope { available: 28 })
    ));
    assert_eq!(
        build_luminance(1, 1, 1).len(),
        32,
        "a 1x1 record with one mip is exactly the 32 bytes the envelope reader needs"
    );
    assert!(decode_raster(&build_luminance(1, 1, 1)).is_ok());
    // And 31 bytes is one short of it, refused the same way.
    let mut one_short = build_luminance(1, 1, 1);
    one_short.truncate(31);
    assert!(matches!(
        decode_raster(&one_short),
        Err(TextureError::TruncatedEnvelope { available: 31 })
    ));
}

#[test]
fn every_raster_record_is_one_chain_and_no_count_is_derived_from_a_division() {
    // **This test used to assert the opposite**, and the opposite was the
    // refuted premise: a `DXT5` record of a given geometry derived TWO layers,
    // and the `0x15` family one, and the two counts came from the same division
    // `(record_size - 32) / (16 + chain)` run on different arithmetic. The corpus
    // says there is one chain in every record of all three formats, so the count
    // is 1 and is not derived from anything.
    //
    // The old evidence for "two" survives as a number worth keeping: at 8 bytes
    // per block the 512x512 DXT5 payload is 349 584 bytes and the old stride
    // 174 792, which divides exactly. That is the arithmetic coincidence that
    // made the refuted rule look right, and it is why 1374 records decoded under
    // it. It is asserted here as a *statement about the old rule*, not as a
    // property of any decoder.
    for (w, h, mips) in [(8u32, 8u32, 4u32), (64, 64, 7), (512, 512, 10)] {
        for frame in REF_FRAMES {
            let record = build(frame, w, h, mips);
            let envelope = parse_envelope(&record).expect("a built record has a readable header");
            let image = decode_raster(&record).expect("a built record decodes");
            assert_eq!(
                layer_count_for(record.len(), &envelope),
                Ok(1),
                "{} {w}x{h} mips={mips}: one chain, and the count is not a division",
                frame_name(frame)
            );
            assert_eq!(
                image.layers.len(),
                1,
                "{} {w}x{h} mips={mips}: one chain",
                frame_name(frame)
            );
        }
    }
    // What the old rule would have said about the real corpus shapes, computed
    // here from its own arithmetic so the refutation is legible rather than
    // asserted: (record - 32) / (16 + chain-at-8-bytes-per-block).
    let old_stride = 16 + ref_chain_at_eight_bytes_per_block(512, 512, 10);
    let payload = 349_616 - 32;
    assert_eq!(payload % old_stride, 0, "the old rule divided exactly here");
    assert_eq!(
        payload / old_stride,
        2,
        "which is how it reported two layers"
    );
    assert_ne!(
        ref_record_size(REF_DXT5, 512, 512, 10),
        Some(349_584),
        "the measured identity does NOT give the old stride arithmetic its second layer: the \
         real 512x512/10 DXT5 record is 349 616 bytes"
    );
}

/// The `0x15`-style chain total the refuted rule used: `sum over levels of
/// ceil4(w)*ceil4(h)*8`, prefixes excluded. Written out because it is the *old*
/// rule, kept so the refutation above is arithmetic rather than rhetoric.
fn ref_chain_at_eight_bytes_per_block(width: u32, height: u32, mip_count: u32) -> usize {
    let bounded = mip_count.min(MIP_FLOOR);
    let mut total = 0usize;
    for mip in 0..bounded {
        let w = ref_ceil4(ref_dim(width, mip)) as usize;
        let h = ref_ceil4(ref_dim(height, mip)) as usize;
        total += w * h * BC1_BLOCK;
    }
    total + mip_count.saturating_sub(bounded) as usize * BC1_BLOCK
}

#[test]
fn a_record_that_is_not_exactly_the_framed_size_is_refused_with_both_numbers() {
    // **This test used to assert that a layer count which does not divide is
    // refused rather than rounded.** That is still the property, but "does not
    // divide" was a statement about a payload and a stride; the identity has no
    // stride and no remainder, so the refusal now names the record's length AND
    // the length its own envelope implies. Both numbers, always: a refusal that
    // reported only one would leave a caller unable to tell a truncation from a
    // padding.
    //
    // The old error variants (`PayloadNotMultiple`, `PayloadTooSmall`) are
    // checked as *unreachable*, because leaving them reachable would be leaving
    // the refuted division in the decoder.
    for frame in REF_FRAMES {
        let (w, h, mips) = (8u32, 8u32, 4u32);
        let record = build(frame, w, h, mips);
        let envelope = parse_envelope(&record).expect("a readable header");
        let exact = ref_record_size(frame, w, h, mips).expect("addressable");
        for extra in 1..=8usize {
            let mut longer = record.clone();
            longer.extend(std::iter::repeat_n(0u8, extra));
            for (what, bytes) in [
                ("padded", longer.clone()),
                ("truncated", longer[..longer.len() - 2 * extra].to_vec()),
            ] {
                match decode_raster(&bytes) {
                    Err(TextureError::RecordSizeMismatch {
                        record_size,
                        expected,
                        ..
                    }) => {
                        assert_eq!(
                            expected,
                            exact,
                            "{}: the expected size is the identity",
                            frame_name(frame)
                        );
                        assert_ne!(
                            record_size,
                            expected,
                            "{}: the two numbers must differ, or the refusal is vacuous",
                            frame_name(frame)
                        );
                    }
                    Err(error) => panic!(
                        "{} {what} by {extra}: expected RecordSizeMismatch, got {error}",
                        frame_name(frame)
                    ),
                    Ok(image) => panic!(
                        "{} {what} by {extra} byte(s) decoded as Ok; first level {} bytes",
                        frame_name(frame),
                        image.layers[0][0].pixels.len()
                    ),
                }
                // The same refusal through the counting entry point.
                match layer_count_for(bytes.len(), &envelope) {
                    Err(TextureError::RecordSizeMismatch { expected, .. }) => {
                        assert_eq!(
                            expected,
                            exact,
                            "{}: expected is the identity",
                            frame_name(frame)
                        );
                    }
                    Err(other) => panic!(
                        "{} {what}: layer_count_for must refuse the same way, got {other}",
                        frame_name(frame)
                    ),
                    Ok(count) => panic!(
                        "{} {what} by {extra} byte(s): layer_count_for derived {count}",
                        frame_name(frame)
                    ),
                }
            }
        }
    }
    // The division-era variants are not produced by either entry point, anywhere
    // in the sweeps above. Stated once, over a record that the old rule would
    // have refused and the identity accepts.
    let record = build_dxt5(8, 8, 4);
    assert!(
        decode_raster(&record).is_ok(),
        "the record the old division refused now decodes"
    );
    let mut one_byte_more = record.clone();
    one_byte_more.push(0);
    let error = decode_raster(&one_byte_more).expect_err("one byte more must be refused");
    assert!(
        !matches!(
            error,
            TextureError::PayloadNotMultiple { .. } | TextureError::PayloadTooSmall { .. }
        ),
        "the refuted division's own error variants must be unreachable from decode_raster, got \
         {error}"
    );
}

#[test]
fn field_18_is_the_base_level_byte_count_in_every_format_and_is_read() {
    // The header word whose meaning the corpus settled, and the one that changed
    // this file's expectations most. It used to be documented as
    // *format-specific* — "0x40000 in every DXT5 record whatever the
    // dimensions" — on the strength of three 512x512 samples, and the crate
    // deliberately did not read it. It is the base level's own byte count in all
    // three formats, which is why the 512x512 constant was an artefact of
    // charging 8 bytes per block: at 8, field_18 read as two base levels.
    //
    // Three things are asserted, and they are different things:
    //
    // 1. the VALUE, per format and shape — the old "format-specific" reading
    //    cannot survive the 512x512/256x256 pair below;
    // 2. that a record whose field_18 disagrees is refused BY NAME, so a
    //    corrupted header word is never quietly obeyed;
    // 3. that obeying it would change the decode — which is why (2) is a
    //    refusal and not a silent truncation.
    for frame in REF_FRAMES {
        for (w, h) in [(64u32, 64u32), (256, 256), (512, 512)] {
            let base = ref_level_bytes(frame, w, h, 0).expect("addressable");
            let record = build(frame, w, h, 3);
            assert_eq!(
                ref_u32(&record, 0x18),
                Some(base as u32),
                "{} {w}x{h}: field_18 must state the base level's byte count",
                frame_name(frame)
            );
            let image = decode_raster(&record).expect("decodes");
            assert_eq!(image.envelope.field_18 as usize, base);
        }
    }
    // The pair that refuted "format-specific": same format, two sizes, two
    // values. Under the old rule both read 0x40000.
    assert_eq!(
        ref_u32(&build(REF_DXT5, 512, 512, 10), 0x18),
        Some(0x0004_0000),
        "512x512 DXT5 field_18 is 128*128*16"
    );
    assert_eq!(
        ref_u32(&build(REF_DXT5, 256, 256, 9), 0x18),
        Some(0x0001_0000),
        "256x256 DXT5 field_18 is 64*64*16, so the word is NOT the 512x512 constant"
    );
    assert_eq!(
        ref_u32(&build(REF_DXT1, 256, 256, 9), 0x18),
        Some(0x0000_8000),
        "256x256 DXT1 field_18 is 64*64*8 -- the same rule, a different stride"
    );

    // (2) and (3): a header that contradicts its own geometry is refused, and
    // the refusal names the word.
    for frame in REF_FRAMES {
        let record = build(frame, 8, 8, 4);
        let base = ref_level_bytes(frame, 8, 8, 0).expect("addressable") as u32;
        for declared in [0u32, base - 1, base + 1, u32::MAX] {
            let mut wrong = record.clone();
            wrong[0x18..0x1C].copy_from_slice(&declared.to_le_bytes());
            match decode_raster(&wrong) {
                Ok(_) => panic!(
                    "{}: field_18 {declared} against a {base}-byte base level decoded; a decoder \
                     that obeys the word instead of the geometry would splice two records together",
                    frame_name(frame)
                ),
                Err(error) => assert!(
                    matches!(error, TextureError::Field18Mismatch { .. }),
                    "{}: expected Field18Mismatch, got {error}",
                    frame_name(frame)
                ),
            }
        }
    }
}

/// A defect in the crate under test, found by this file's oracle, pinned here
/// rather than worked around — and now **fixed**, so this file pins the fix.
///
/// # What the defect was
///
/// `framing::record_size` charges every level past `MIP_FLOOR` a prefix plus a
/// 1x1 payload, and `chain_size` agrees with it — which is why this file's
/// `the_crates_record_size_equals_an_independently_written_identity` passes at 34
/// and 40 levels. `framing::level_offset`, which is what `decode_raster` walks
/// with, stopped its accumulation loop at `mip.min(MIP_FLOOR)`, so it omitted one
/// prefix and one 1x1 payload for every level past the floor, and the tail of a
/// long chain was read from the wrong offsets. Measured, not assumed: on a 34-level
/// 1x1 DXT5 record the sizing said **704** and the walk said **664**, forty bytes
/// apart.
///
/// Two faces, and only the first is silent: a chain whose tail is uniform (every
/// level past the geometry is 1x1, hence every prefix `4`) **decoded as `Ok`** with
/// another level's bytes, while any other shape was refused under a name that
/// pointed at the wrong level. The cap bit from `mip_count == 34`, not 33:
/// placing level `m` needs the accumulation of levels `0..m`, and level 32 is the
/// last index whose offset is still exactly what both functions compute.
///
/// # What the fix is
///
/// `level_offset` no longer extrapolates, and no longer clamps silently. It
/// **refuses**: `mip >= mip_count` is `LevelNotInChain`, `mip >= 33` is
/// `MipChainBeyondFloor`, and a zero dimension is `ZeroDimension`. A chain this
/// build cannot place is now refused by name at both whole-record entry points as
/// well, so `layer_count_for` can no longer answer `Ok(1)` for a record
/// `decode_raster` rejects.
///
/// # Why a refusal and not agreement with `record_size`
///
/// Because the past-the-floor rule is a recorded **approximation**, not a
/// measurement: `spore_texture::claims::MIPS_BEYOND_32` grades it `Approximation`
/// and says it "bounds an absurd header rather than describing how Spore writes a
/// 33rd level". The sizing functions keep it — they are cross-checked against the
/// record's own length and against every prefixed level's byte count, and
/// `spore-texture`'s own `a_sizing_question_is_still_answered_past_the_floor`
/// pins that they still answer. The *placement* function does not, because a
/// number only an approximation can produce is a fabricated offset. Nothing in the
/// corpus is affected either way: the largest chain in 4386 raster records is ten
/// levels (`crates/spore-differential/tests/framing_refusals.rs` re-measures it).
#[test]
fn the_walk_past_level_32_is_refused_rather_than_mis_placed() {
    // --- the sizing half is unchanged, and this file's identity still holds.
    for (w, h, mips) in beyond_the_floor_shapes() {
        let record = build_luminance(w, h, mips);
        let exact = ref_record_size(REF_LUM, w, h, mips).expect("addressable");
        let frame = frame_for(LUMINANCE_FOURCC).expect("a frame");
        assert_eq!(record.len(), exact, "{w}x{h} mips={mips}: fixture length");
        assert_eq!(
            spore_texture::framing::record_size(frame, w, h, mips),
            exact,
            "{w}x{h} mips={mips}: record_size must still include the levels past the floor -- it \
             is a recorded approximation, not a removal"
        );
    }

    // --- and the walking half now refuses, by name, for every level past the
    //     ceiling and for every level the envelope does not declare.
    let frame = frame_for(LUMINANCE_FOURCC).expect("a frame");
    let ceiling = spore_texture::framing::MAX_PLACEABLE_LEVELS;
    assert_eq!(ceiling, MIP_FLOOR + 1);

    for (w, h, mips) in beyond_the_floor_shapes() {
        // Both whole-record entry points, one variant, naming the count and the
        // ceiling. Before the fix the decode said `Ok` (wrong) or named a length
        // prefix at a level that was not the one at fault.
        let record = build_luminance(w, h, mips);
        let expected = TextureError::MipChainBeyondFloor {
            levels: mips,
            floor: ceiling,
        };
        assert_eq!(
            decode_raster(&record),
            Err(expected.clone()),
            "{w}x{h} mips={mips}: a chain this build cannot place must be refused by name, not \
             decoded"
        );
        assert_eq!(
            layer_count_for(record.len(), &parse_envelope(&record).expect("envelope")),
            Err(expected),
            "{w}x{h} mips={mips}: layer_count_for must not answer Ok(1) for a record decode_raster \
             refuses"
        );

        // And the walk itself: one refusal per unplaceable index, and no value.
        for mip in [MIP_FLOOR + 1, mips, mips + 1, u32::MAX] {
            let answer = spore_texture::framing::level_offset(frame, w, h, mip, mips);
            let placeable = mip < mips && mip < ceiling;
            assert_eq!(
                answer.is_ok(),
                placeable,
                "{w}x{h} mips={mips}: level {mip} {}",
                if placeable {
                    "is placeable"
                } else {
                    "must be refused"
                }
            );
            if let Err(error) = answer {
                assert!(
                    matches!(
                        error,
                        TextureError::MipChainBeyondFloor { .. }
                            | TextureError::LevelNotInChain { .. }
                    ),
                    "{w}x{h} mips={mips} level {mip}: an undeclared or unplaceable level must be \
                     refused by name, got {error}"
                );
            }
        }
    }

    // --- the levels that ARE placeable are placed exactly, right up to and
    //     including the last one, 32. Before the fix the sweep stopped early and
    //     the tail was mis-placed; now the tail is refused and the head is exact.
    for (w, h, mips) in [
        (1u32, 1u32, 33u32),
        (8, 8, 33),
        (64, 64, 33),
        (512, 512, 33),
    ] {
        let windows = ref_windows(REF_LUM, w, h, mips);
        assert_eq!(mips, ceiling, "33 is the longest chain this build places");
        assert_eq!(
            windows.last().map(|(at, len)| at + len),
            ref_record_size(REF_LUM, w, h, mips),
            "{w}x{h} mips={mips}: this file's own windows must still close on the record"
        );
        for (mip, (payload_at, _)) in windows.iter().enumerate() {
            let mut at = spore_texture::framing::level_offset(frame, w, h, mip as u32, mips)
                .unwrap_or_else(|error| panic!("{w}x{h}: level {mip} must be placeable: {error}"));
            if mip as u32 >= REF_LUM.first_prefixed {
                at += PREFIX;
            }
            assert_eq!(
                at, *payload_at,
                "{w}x{h} mips={mips}: level {mip} is at or below the ceiling and must be placed \
                 exactly"
            );
        }
    }
}

#[test]
fn the_builder_reproduces_the_record_sizes_the_corpus_measures() {
    // A builder that did not agree with the measured corpus would make every
    // test in this file vacuous, so the agreement is asserted directly with the
    // sizes the repository records. These are the numbers from
    // `docs/RASTER-FRAMING.md` sections 2 and 3, one per shape and format.
    for (frame, w, h, mips, size) in [
        (REF_LUM, 32u32, 32u32, 6u32, 5_508usize),
        (REF_LUM, 256, 256, 9, 349_584),
        (REF_LUM, 512, 512, 10, 1_398_164),
        (REF_DXT5, 64, 64, 7, 5_540),
        (REF_DXT5, 256, 256, 9, 87_468),
        (REF_DXT5, 512, 512, 10, 349_616),
        (REF_DXT1, 256, 256, 9, 43_764),
    ] {
        assert_eq!(
            ref_record_size(frame, w, h, mips),
            Some(size),
            "{} {w}x{h} mips={mips}",
            frame_name(frame)
        );
        assert_eq!(build(frame, w, h, mips).len(), size);
        assert!(
            decode_raster(&build(frame, w, h, mips)).is_ok(),
            "{} {w}x{h} mips={mips}: the measured shape must decode",
            frame_name(frame)
        );
    }
    // The block arithmetic, re-derived: ceil4(w)*ceil4(h)*16 for BC3 and *8 for
    // BC1. The 8-byte figure is what the old rule charged for DXT5 and is why
    // two errors cancelled.
    assert_eq!(ref_level_bytes(REF_DXT5, 512, 512, 0), Some(128 * 128 * 16));
    assert_eq!(ref_level_bytes(REF_DXT1, 512, 512, 0), Some(128 * 128 * 8));
    assert_eq!(ref_level_bytes(REF_DXT5, 8, 8, 0), Some(2 * 2 * 16));
    assert_eq!(ref_level_bytes(REF_DXT1, 8, 8, 0), Some(2 * 2 * 8));
    assert_eq!(
        ref_level_bytes(REF_DXT5, 1, 1, 0),
        Some(16),
        "a 1x1 level is still one 4x4 block"
    );
    assert_eq!(ref_level_bytes(REF_DXT5, 5, 3, 0), Some(2 * 16));
    assert_eq!(
        ref_level_bytes(REF_LUM, 5, 3, 0),
        Some(60),
        "the uncompressed family is four bytes per TEXEL, so it has no ceil4"
    );
}
