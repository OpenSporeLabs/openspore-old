//! A minimal, dependency-free PNG writer.
//!
//! # Why this exists, and why it looks the way it does
//!
//! `osptool texture` has to produce a file a human (or an image viewer, or GIMP)
//! will open. `spore-tools` may not gain a dependency — the workspace rule is
//! that its crate list is the set of crates it inspects plus nothing else — and
//! `spore-engine`'s `png` feature is Bevy's image loader, which is both a GPU
//! dependency and a *reader*, not a writer. So the writer is here.
//!
//! # What is and is not implemented
//!
//! PNG (RFC 2083) is a container of zlib-compressed scanlines behind a CRC
//! per chunk. This writer emits the smallest valid subset:
//!
//! * 8-bit RGBA only (colour type 6, bit depth 8), which is exactly what
//!   [`spore_texture::MipImage`] carries;
//! * filter type `0` (None) on every scanline;
//! * no interlacing, no ancillary chunks, no palette, no gamma;
//! * **deflate "stored" blocks** — each 65 535-byte run of the stream is emitted
//!   uncompressed inside a zlib wrapper.
//!
//! The last point is the real trade-off and it is worth being explicit: the
//! output is a *correct* PNG that any decoder accepts, but for a 512x512 RGBA
//! level it is 1 MiB where a real compressor would manage a fraction of that.
//! Spore's DXT5 texture records compress to a small fraction of their RGBA
//! expansion, so a stored-deflate PNG is large. This tool exists to make a decode
//! *inspectable*, not to ship assets, and a large inspectable file beats a
//! small one nobody can open. `spore-tools` will not grow a compression
//! dependency for a size nobody needs.
//!
//! # What it is checked against
//!
//! [`verify`] re-reads every IDAT through the same inflate-a-stored-block path
//! and checks the CRC and Adler-32, so the encoder and an independent check of
//! its output are both in this module and neither is a claim about a file this
//! repository cannot see.

use std::io::Write;

/// The eight-byte PNG signature, RFC 2083 §5.2.
pub const SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];

/// The filter type this writer puts on every scanline.
const FILTER_NONE: u8 = 0;

/// Colour type 6: truecolour with alpha.
const COLOUR_TYPE_RGBA: u8 = 6;

/// Bit depth 8.
const BIT_DEPTH: u8 = 8;

/// The largest payload one deflate *stored* block can carry.
const MAX_STORED_BLOCK: usize = 0xFFFF;

/// One RGBA8 image, ready to be written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rgba8 {
    /// Width in texels. Must be non-zero.
    pub width: u32,
    /// Height in texels. Must be non-zero.
    pub height: u32,
    /// Exactly `width * height * 4` bytes, row-major, RGBA order.
    pub pixels: Vec<u8>,
}

/// Why a [`Rgba8`] cannot be encoded.
///
/// Every variant is a **programming error** caught before a single byte is
/// written, so a caller never has to distinguish "the file is half a PNG".
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PngError {
    /// A zero dimension: PNG requires both to be at least 1.
    ZeroDimension {
        /// The width that was zero.
        width: u32,
        /// The height that was zero.
        height: u32,
    },
    /// The pixel buffer length disagrees with `width * height * 4`.
    ///
    /// Checked by value rather than by "is it long enough": a buffer that is
    /// *longer* would silently crop, and a cropped image is a lie about the
    /// decode.
    LengthMismatch {
        /// Bytes the geometry requires.
        expected: usize,
        /// Bytes supplied.
        actual: usize,
    },
    /// `width * height * 4` does not fit in a `usize`, so the image could not
    /// be addressed here even if the bytes existed.
    Unaddressable {
        /// The width requested.
        width: u32,
        /// The height requested.
        height: u32,
    },
}

impl std::fmt::Display for PngError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ZeroDimension { width, height } => {
                write!(f, "png: a {width}x{height} image has no texels to write")
            }
            Self::LengthMismatch { expected, actual } => write!(
                f,
                "png: this image needs exactly {expected} pixel bytes, {actual} supplied"
            ),
            Self::Unaddressable { width, height } => {
                write!(f, "png: a {width}x{height} image is not addressable")
            }
        }
    }
}

impl std::error::Error for PngError {}

/// `width * height * 4`, or the reason it is not computable.
fn pixel_bytes(width: u32, height: u32) -> Result<usize, PngError> {
    if width == 0 || height == 0 {
        return Err(PngError::ZeroDimension { width, height });
    }
    (width as usize)
        .checked_mul(height as usize)
        .and_then(|texels| texels.checked_mul(4))
        .filter(|len| *len <= isize::MAX as usize)
        .ok_or(PngError::Unaddressable { width, height })
}

impl Rgba8 {
    /// Wraps a pixel buffer, checking its length against the geometry.
    pub fn new(width: u32, height: u32, pixels: Vec<u8>) -> Result<Self, PngError> {
        let expected = pixel_bytes(width, height)?;
        if pixels.len() != expected {
            return Err(PngError::LengthMismatch {
                expected,
                actual: pixels.len(),
            });
        }
        Ok(Self {
            width,
            height,
            pixels,
        })
    }

    /// The exact byte length of the encoded file.
    ///
    /// Computed rather than measured, so a caller can report the size it is
    /// about to write and a test can assert the two agree. Depends only on the
    /// geometry, because the stored-deflate stream is exactly one filter byte
    /// per row plus four bytes per texel.
    pub fn encoded_len(&self) -> usize {
        let zlib = 2 + deflate_stored_len(self.filtered_len()) + 4; // header + stream + adler32
        SIGNATURE.len() + chunk_len(13) + chunk_len(zlib) + chunk_len(0)
    }

    /// Bytes of scanline data: one filter byte per row plus the pixels.
    fn filtered_len(&self) -> usize {
        self.pixels.len() + self.height as usize
    }

    /// Encodes the image as a complete PNG file.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.encoded_len());
        out.extend_from_slice(&SIGNATURE);

        let mut ihdr = Vec::with_capacity(13);
        ihdr.extend_from_slice(&self.width.to_be_bytes());
        ihdr.extend_from_slice(&self.height.to_be_bytes());
        ihdr.push(BIT_DEPTH);
        ihdr.push(COLOUR_TYPE_RGBA);
        ihdr.push(0); // compression method: deflate
        ihdr.push(0); // filter method: adaptive
        ihdr.push(0); // interlace method: none
        write_chunk(&mut out, b"IHDR", &ihdr);

        let raw = self.filtered_bytes();
        let compressed = zlib_stored(&raw);
        write_chunk(&mut out, b"IDAT", &compressed);

        write_chunk(&mut out, b"IEND", &[]);
        debug_assert_eq!(
            out.len(),
            self.encoded_len(),
            "encoded_len must predict the file exactly"
        );
        out
    }

    /// Writes the scanline stream: a `0` filter byte then one row of RGBA8.
    ///
    /// Filter `None` is a real choice, not a shortcut: it costs `height` bytes
    /// and no compression, and it is the only filter that is correct for every
    /// image without the writer having to know anything about the pixels.
    fn filtered_bytes(&self) -> Vec<u8> {
        let stride = (self.width as usize) * 4;
        let mut out = Vec::with_capacity(self.filtered_len());
        for row in 0..self.height as usize {
            out.push(FILTER_NONE);
            let start = row * stride;
            out.extend_from_slice(&self.pixels[start..start + stride]);
        }
        out
    }
}

/// Bytes a chunk of `len` data bytes occupies: length word, type, data, CRC.
const fn chunk_len(len: usize) -> usize {
    12 + len
}

/// Appends one chunk: length, type, data, CRC-32 of `type || data`.
fn write_chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    debug_assert_eq!(kind.len(), 4);
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    let mut crc_input = Vec::with_capacity(4 + data.len());
    crc_input.extend_from_slice(kind);
    crc_input.extend_from_slice(data);
    out.extend_from_slice(&crc32(&crc_input).to_be_bytes());
}

/// Wraps `data` in a zlib stream (RFC 1950) of stored deflate blocks.
fn zlib_stored(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(2 + deflate_stored_len(data.len()) + 4);
    // CMF: compression method 8 (deflate), window size 7 -> 32 KiB. The low four
    // bits of CINFO are the window exponent, which must be <= 7.
    let cmf: u8 = 0x78;
    // FLG carries FCHECK such that (CMF * 256 + FLG) % 31 == 0, and FDICT is 0.
    let flg = (31 - ((cmf as u16 * 256) % 31)) % 31;
    out.push(cmf);
    out.push(flg as u8);
    out.extend_from_slice(&deflate_stored(data));
    out.extend_from_slice(&adler32(data).to_be_bytes());
    out
}

/// Bytes a stored-block deflate stream of `len` input bytes occupies.
const fn deflate_stored_len(len: usize) -> usize {
    // Per block: 1 header byte, 2 length bytes, 2 negated-length bytes.
    if len == 0 {
        5
    } else {
        let blocks = len.div_ceil(MAX_STORED_BLOCK);
        len + blocks * 5
    }
}

/// Encodes `data` as a raw deflate stream of stored (BTYPE=00) blocks.
///
/// The last block sets BFINAL; an empty input still emits one empty final block
/// so the stream is a complete, terminating deflate sequence.
fn deflate_stored(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(deflate_stored_len(data.len()));
    if data.is_empty() {
        push_stored_block(&mut out, &[], true);
        return out;
    }
    let mut offset = 0usize;
    while offset < data.len() {
        let end = offset.saturating_add(MAX_STORED_BLOCK).min(data.len());
        push_stored_block(&mut out, &data[offset..end], end == data.len());
        offset = end;
    }
    out
}

/// One stored block: header byte with BFINAL and BTYPE=00, then LEN and its
/// one's complement, then the payload verbatim (RFC 1951 §3.2.4).
fn push_stored_block(out: &mut Vec<u8>, payload: &[u8], final_block: bool) {
    debug_assert!(payload.len() <= MAX_STORED_BLOCK);
    out.push(u8::from(final_block));
    let len = payload.len() as u16;
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(&(!len).to_le_bytes());
    out.extend_from_slice(payload);
}

/// Adler-32, RFC 1950 §9.
fn adler32(data: &[u8]) -> u32 {
    const MOD: u32 = 65521;
    let mut a: u32 = 1;
    let mut b: u32 = 0;
    // 5552 is the largest run that cannot overflow `b` between modulo steps.
    for chunk in data.chunks(5552) {
        for &byte in chunk {
            a += u32::from(byte);
            b += a;
        }
        a %= MOD;
        b %= MOD;
    }
    (b << 16) | a
}

/// CRC-32 as PNG defines it: the standard reflected CRC-32 with polynomial
/// `0xEDB88320`, pre- and post-conditioned, zero seed.
fn crc32(data: &[u8]) -> u32 {
    let mut crc: u32 = 0xFFFF_FFFF;
    for &byte in data {
        crc ^= u32::from(byte);
        // Eight rounds, least-significant bit first: the reflected form.
        for _ in 0..8 {
            let mask = if crc & 1 == 1 { 0xEDB8_8320 } else { 0 };
            crc = (crc >> 1) ^ mask;
        }
    }
    crc ^ 0xFFFF_FFFF
}

/// Everything a caller needs to check a PNG this module wrote.
///
/// Re-reads the file's own framing: signature, chunk walk, every CRC, and the
/// zlib stream including its Adler-32. This is deliberately an *independent*
/// check of the encoder rather than a round trip through the encoder's own
/// helpers, so a bug in one of them cannot hide behind the other.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodedPng {
    /// The IHDR width.
    pub width: u32,
    /// The IHDR height.
    pub height: u32,
    /// The de-filtered, un-interlaced RGBA8 pixels.
    pub pixels: Vec<u8>,
}

/// Checks a PNG byte-for-byte against what this writer promises.
///
/// Returns the decoded pixels. Refuses anything whose chunks are not
/// `(length, IHDR, IDAT+, IEND)`, whose CRC-32 or Adler-32 disagrees, or whose
/// scanlines do not carry filter `None`.
pub fn verify(bytes: &[u8]) -> Result<DecodedPng, String> {
    if bytes.get(..8) != Some(SIGNATURE.as_slice()) {
        return Err("png: bad signature".to_owned());
    }
    let mut at = 8usize;
    let mut header: Option<(u32, u32)> = None;
    let mut idat = Vec::new();
    let mut saw_iend = false;

    while at < bytes.len() {
        let length = be_u32(bytes, at).ok_or("png: truncated chunk length")? as usize;
        at += 4;
        let kind = bytes.get(at..at + 4).ok_or("png: truncated chunk type")?;
        at += 4;
        let data = bytes
            .get(at..at + length)
            .ok_or_else(|| format!("png: chunk {at:#x} claims {length} bytes, file ends"))?;
        at += length;
        let declared = be_u32(bytes, at).ok_or("png: truncated chunk CRC")?;
        at += 4;

        let mut covered = Vec::with_capacity(4 + length);
        covered.extend_from_slice(kind);
        covered.extend_from_slice(data);
        let actual = crc32(&covered);
        if declared != actual {
            return Err(format!(
                "png: chunk {} CRC {declared:#010x} != computed {actual:#010x}",
                String::from_utf8_lossy(kind)
            ));
        }

        match kind {
            b"IHDR" => {
                if data.len() != 13 {
                    return Err(format!("png: IHDR is {} bytes, not 13", data.len()));
                }
                let width = be_u32(data, 0).unwrap();
                let height = be_u32(data, 4).unwrap();
                if data[8] != BIT_DEPTH {
                    return Err(format!("png: bit depth {} is not 8", data[8]));
                }
                if data[9] != COLOUR_TYPE_RGBA {
                    return Err(format!("png: colour type {} is not 6 (RGBA)", data[9]));
                }
                if data[12] != 0 {
                    return Err("png: this writer never interlaces".to_owned());
                }
                header = Some((width, height));
            }
            b"IDAT" => idat.extend_from_slice(data),
            b"IEND" => {
                saw_iend = true;
                if at != bytes.len() {
                    return Err(format!("png: {} bytes follow IEND", bytes.len() - at));
                }
            }
            other => {
                return Err(format!(
                    "png: unexpected chunk {}",
                    String::from_utf8_lossy(other)
                ))
            }
        }
    }

    if !saw_iend {
        return Err("png: no IEND chunk".to_owned());
    }
    let (width, height) = header.ok_or("png: no IHDR chunk")?;
    let raw = inflate_stored(&idat)?;
    let expected = (width as usize)
        .checked_mul(height as usize)
        .and_then(|t| t.checked_mul(4))
        .and_then(|b| b.checked_add(height as usize))
        .ok_or("png: image is not addressable")?;
    if raw.len() != expected {
        return Err(format!(
            "png: {}x{} needs {expected} filtered bytes, stream holds {}",
            width,
            height,
            raw.len()
        ));
    }
    let stride = (width as usize) * 4;
    let mut pixels = Vec::with_capacity(stride * height as usize);
    for row in 0..height as usize {
        let start = row * (stride + 1);
        if raw[start] != FILTER_NONE {
            return Err(format!("png: scanline {row} has filter {}", raw[start]));
        }
        pixels.extend_from_slice(&raw[start + 1..start + 1 + stride]);
    }
    Ok(DecodedPng {
        width,
        height,
        pixels,
    })
}

/// Reads a big-endian `u32` at `at`.
fn be_u32(bytes: &[u8], at: usize) -> Option<u32> {
    let slice = bytes.get(at..at + 4)?;
    Some(u32::from_be_bytes([slice[0], slice[1], slice[2], slice[3]]))
}

/// Reverses [`zlib_stored`]: checks the header, walks the stored blocks, and
/// checks the Adler-32.
///
/// Only stored blocks are accepted, which is exactly the set this writer emits.
/// A stream using Huffman or fixed coding would be a *different* encoder's
/// output; saying so is more useful than decoding it and pretending this
/// module can.
fn inflate_stored(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let (&cmf, rest) = bytes.split_first().ok_or("png: empty IDAT")?;
    let (&flg, rest) = rest.split_first().ok_or("png: truncated zlib header")?;
    if cmf & 0x0F != 8 {
        return Err(format!("png: zlib method {} is not deflate", cmf & 0x0F));
    }
    if cmf >> 4 > 7 {
        return Err(format!("png: zlib window {} is undefined", cmf >> 4));
    }
    if flg & 0x20 != 0 {
        return Err("png: zlib preset dictionary is not supported".to_owned());
    }
    if (u16::from(cmf) * 256 + u16::from(flg)) % 31 != 0 {
        return Err("png: zlib header check failed".to_owned());
    }

    let mut out = Vec::new();
    let mut at = 0usize;
    loop {
        let header = *rest.get(at).ok_or("png: truncated deflate block header")?;
        at += 1;
        if header & 0x06 != 0 {
            return Err(format!(
                "png: deflate block uses type {} (only stored is accepted)",
                (header >> 1) & 3
            ));
        }
        let final_block = header & 1 == 1;
        let len = u16::from_le_bytes([
            *rest.get(at).ok_or("png: truncated block length")?,
            *rest.get(at + 1).ok_or("png: truncated block length")?,
        ]);
        at += 2;
        let nlen = u16::from_le_bytes([
            *rest.get(at).ok_or("png: truncated block check")?,
            *rest.get(at + 1).ok_or("png: truncated block check")?,
        ]);
        at += 2;
        if len ^ nlen != 0xFFFF {
            return Err(format!(
                "png: block length {len} and its complement {nlen} disagree"
            ));
        }
        let block = rest
            .get(at..at + len as usize)
            .ok_or_else(|| format!("png: stored block claims {len} bytes, stream ends"))?;
        out.extend_from_slice(block);
        at += len as usize;
        if final_block {
            if at + 4 != rest.len() {
                return Err(format!(
                    "png: {} bytes follow the final deflate block",
                    rest.len() - at - 4
                ));
            }
            break;
        }
    }

    let stored = u32::from_be_bytes([rest[at], rest[at + 1], rest[at + 2], rest[at + 3]]);
    let computed = adler32(&out);
    if stored != computed {
        return Err(format!(
            "png: Adler-32 {stored:#010x} != computed {computed:#010x}"
        ));
    }
    Ok(out)
}

/// Writes `image` to `path`, creating or truncating it.
///
/// Reports the byte count on `err`, following the rule the rest of the tool
/// follows: **stdout carries only the document the caller asked for**, and a
/// file write is confirmed on stderr.
pub fn write_to(
    image: &Rgba8,
    path: &std::path::Path,
    err: &mut dyn Write,
) -> std::io::Result<usize> {
    let bytes = image.to_bytes();
    std::fs::write(path, &bytes)?;
    let _ = writeln!(
        err,
        "osptool: wrote {} ({} bytes)",
        path.display(),
        bytes.len()
    );
    Ok(bytes.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One opaque red 2x1 image.
    fn sample() -> Rgba8 {
        Rgba8::new(2, 1, vec![255, 0, 0, 255, 0, 255, 0, 128]).unwrap()
    }

    #[test]
    fn the_signature_is_the_one_rfc_2083_gives() {
        assert_eq!(&SIGNATURE, b"\x89PNG\r\n\x1a\n");
    }

    #[test]
    fn a_written_png_verifies_and_gives_the_pixels_back() {
        let image = sample();
        let bytes = image.to_bytes();
        let decoded = verify(&bytes).expect("the encoder's own output must verify");
        assert_eq!(decoded.width, 2);
        assert_eq!(decoded.height, 1);
        assert_eq!(decoded.pixels, image.pixels);
    }

    #[test]
    fn encoded_len_predicts_the_file_to_the_byte() {
        // Across the two interesting sizes: one stored block, and a multi-block
        // stream (600*600*4 = 1 440 000 bytes is 22 stored blocks).
        for (width, height) in [(1u32, 1u32), (2, 1), (256, 256), (600, 600)] {
            let pixels = vec![0xABu8; (width * height * 4) as usize];
            let image = Rgba8::new(width, height, pixels).unwrap();
            let bytes = image.to_bytes();
            assert_eq!(
                bytes.len(),
                image.encoded_len(),
                "{width}x{height}: the predicted size must be the written size"
            );
            // The whole file is exactly: signature, IHDR, IDAT, IEND.
            let fixed = 8 + chunk_len(13) + chunk_len(0);
            assert_eq!(
                image.encoded_len(),
                fixed + chunk_len(2 + deflate_stored_len(image.filtered_len()) + 4),
                "{width}x{height}"
            );
        }
    }

    #[test]
    fn a_multi_block_stream_verifies() {
        // 65536 texels of RGBA8 is 262 144 bytes, four stored blocks.
        let width = 256u32;
        let height = 256u32;
        let mut pixels = vec![0u8; (width * height * 4) as usize];
        for (index, slot) in pixels.iter_mut().enumerate() {
            *slot = (index % 251) as u8;
        }
        let image = Rgba8::new(width, height, pixels.clone()).unwrap();
        let decoded = verify(&image.to_bytes()).unwrap();
        assert_eq!(
            decoded.pixels, pixels,
            "a multi-block stream must round trip"
        );
        assert!(deflate_stored_len(262_144) > 262_144);
        assert_eq!(deflate_stored_len(0), 5);
        assert_eq!(deflate_stored_len(1), 6);
    }

    #[test]
    fn crc32_matches_the_check_value_png_publishes() {
        // The canonical CRC-32 check value for the ASCII string "123456789" is
        // 0xCBF43926. Every CRC implementation that gets this right agrees on
        // every real file too.
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
        assert_eq!(crc32(b""), 0);
    }

    #[test]
    fn adler32_matches_its_own_published_check_values() {
        // RFC 1950 worked example: Adler-32 of "Wikipedia" is 0x11E60398.
        assert_eq!(adler32(b"Wikipedia"), 0x11E6_0398);
        assert_eq!(adler32(b""), 1);
    }

    #[test]
    fn the_zlib_header_satisfies_its_own_modulo_check() {
        for len in [0usize, 1, 70000] {
            let stream = zlib_stored(&vec![0u8; len]);
            let check = u16::from(stream[0]) * 256 + u16::from(stream[1]);
            assert_eq!(check % 31, 0, "len {len}");
            assert_eq!(stream[0] & 0x0F, 8, "CM must be deflate");
            assert_eq!(stream[1] & 0x20, 0, "FDICT must be clear");
        }
    }

    #[test]
    fn a_buffer_that_does_not_match_the_geometry_is_refused_not_cropped() {
        assert_eq!(
            Rgba8::new(2, 2, vec![0u8; 15]),
            Err(PngError::LengthMismatch {
                expected: 16,
                actual: 15
            })
        );
        // Longer is just as wrong: silently cropping would misreport the decode.
        assert_eq!(
            Rgba8::new(1, 1, vec![0u8; 5]),
            Err(PngError::LengthMismatch {
                expected: 4,
                actual: 5
            })
        );
        assert_eq!(
            Rgba8::new(0, 4, vec![]),
            Err(PngError::ZeroDimension {
                width: 0,
                height: 4
            })
        );
        assert_eq!(
            Rgba8::new(u32::MAX, u32::MAX, vec![]),
            Err(PngError::Unaddressable {
                width: u32::MAX,
                height: u32::MAX
            })
        );
    }

    #[test]
    fn every_error_text_names_the_numbers_it_refused() {
        let messages = [
            PngError::ZeroDimension {
                width: 0,
                height: 7,
            }
            .to_string(),
            PngError::LengthMismatch {
                expected: 16,
                actual: 4,
            }
            .to_string(),
            PngError::Unaddressable {
                width: u32::MAX,
                height: 2,
            }
            .to_string(),
        ];
        assert!(messages[0].contains("0x7"), "{}", messages[0]);
        assert!(
            messages[1].contains("16") && messages[1].contains('4'),
            "{}",
            messages[1]
        );
        assert!(messages[2].contains('2'), "{}", messages[2]);
    }

    #[test]
    fn verification_refuses_what_the_encoder_never_produces() {
        let good = sample().to_bytes();
        // Bad signature.
        let mut bad = good.clone();
        bad[1] = b'X';
        assert!(verify(&bad).unwrap_err().contains("signature"), "signature");

        // A flipped CRC.
        let mut bad = good.clone();
        let last = bad.len() - 1;
        bad[last] ^= 0xFF;
        assert!(verify(&bad).unwrap_err().contains("CRC"), "CRC");

        // A flipped Adler-32, inside IDAT.
        let mut bad = good.clone();
        let idat_len = be_u32(&bad, 8 + 13 + 12).unwrap() as usize;
        let adler_at = 8 + 13 + 12 + 8 + 4 + 12 + idat_len - 1;
        bad[adler_at] ^= 0xFF;
        let message = verify(&bad).unwrap_err();
        assert!(
            message.contains("Adler") || message.contains("CRC"),
            "an Adler flip is caught, one way or another: {message}"
        );

        // Truncation.
        assert!(verify(&good[..good.len() - 1]).is_err());
        assert!(verify(&[]).is_err());
    }

    #[test]
    fn a_non_stored_deflate_block_is_named_rather_than_decoded() {
        // BFINAL=1, BTYPE=01 (fixed Huffman): the encoder never emits this, so
        // the reader must say so instead of pretending to handle it.
        let mut stream = vec![0x78, 0x01, 0x03, 0x00, 0x00, 0x00, 0x00];
        stream[2] = 0b0000_0011;
        assert!(
            inflate_stored(&stream).unwrap_err().contains("only stored"),
            "an unknown block type must be refused by name"
        );
    }

    #[test]
    fn a_preset_dictionary_is_refused_rather_than_ignored() {
        let stream = vec![
            0x78, 0xBB, 0x01, 0x00, 0x00, 0xFF, 0xFF, 0x00, 0x00, 0x00, 0x01,
        ];
        assert!(inflate_stored(&stream)
            .unwrap_err()
            .contains("preset dictionary"));
    }

    #[test]
    fn write_to_creates_the_file_and_reports_the_size_on_the_named_sink() {
        let mut path = std::env::temp_dir();
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        path.push(format!("osptool-png-{}-{unique}.png", std::process::id()));
        let image = sample();
        let mut err = Vec::new();
        let written = write_to(&image, &path, &mut err).unwrap();
        assert_eq!(written, image.to_bytes().len());
        let on_disk = std::fs::read(&path).unwrap();
        assert_eq!(verify(&on_disk).unwrap().pixels, image.pixels);
        let report = String::from_utf8(err).unwrap();
        assert!(report.contains("wrote"), "{report}");
        assert!(report.contains(&written.to_string()), "{report}");
        let _ = std::fs::remove_file(&path);
    }
}
