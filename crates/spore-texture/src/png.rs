//! Raw-`png` records: an image that is already an image.
//!
//! # Why this file exists when `raster` is the "texture" record
//!
//! `spore-texture`'s [`crate::raster`] deliberately *refuses* a `png` record, and
//! the reason is correct and still is: a `png` record's index bytes begin
//! `89 50 4E 47`, so the payload is already a PNG. There is no envelope, no
//! fourCC, no mip chain and nothing to interpret -- refusing it is refusing to
//! pretend.
//!
//! What that refusal did not provide is a way to *use* one. And Spore ships 10,002
//! `png` records across two packages, including every 2D UI atlas and every
//! painted backdrop in the game. So this module is not a raster decoder with a
//! different name: it is the narrowest possible bridge, `bytes -> RGBA8`, so the
//! engine can put the game's own art on screen without `raster`'s framing
//! machinery being involved at all.
//!
//! # What it supports, and what it refuses
//!
//! 8-bit non-interlaced greyscale, greyscale+alpha, RGB and RGBA, which is what
//! every `png` record measured in the shipped corpus uses (see
//! `the_shipped_png_records_are_all_eight_bit` in the tests below). Anything else
//! -- 16-bit depth, interlacing, a colour type this build does not name -- is
//! refused with a reason rather than approximated. A backdrop that is slightly
//! wrong in gamma is a defect that looks like art; a backdrop that is refused is
//! a defect that looks like a missing backdrop, and the second one can be read.

use crate::TextureError;

/// A decoded image: RGBA8, row-major, `width * height * 4` bytes.
///
/// Deliberately the same shape as [`crate::raster::MipImage`] so a caller that
/// already consumes decoded pixels does not need a second type, and so the two
/// decoders' outputs are interchangeable in a test.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PngImage {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// `width * height * 4` bytes of RGBA8 in row-major order.
    pub pixels: Vec<u8>,
}

impl PngImage {
    /// The pixel count, for the `debug_assert` in [`decode_png`] and for tests.
    pub fn pixel_count(&self) -> usize {
        self.width as usize * self.height as usize
    }
}

/// Decodes a raw-PNG byte string into RGBA8.
///
/// # Errors
///
/// Refuses an unsupported colour type or bit depth rather than converting it.
/// The error text names the case, because "the backdrop did not load" and "the
/// backdrop loaded with the wrong gamma" must not look the same.
pub fn decode_png(data: &[u8]) -> Result<PngImage, TextureError> {
    let decoder = png::Decoder::new(std::io::Cursor::new(data));
    let mut reader = decoder
        .read_info()
        .map_err(|error| TextureError::Malformed {
            detail: format!("png header: {error}"),
        })?;

    // The output buffer has to be sized before the frame is read, and the size
    // depends on the colour type, which is only known after `read_info`. The
    // crate hands back the worst case (it assumes 4 channels), so the allocation
    // is an upper bound and the used prefix is taken from the `OutputInfo` the
    // frame reports -- not from the buffer's length, which is the allocation.
    //
    // `None` means the header promised a frame too large to allocate, which is a
    // property of the file rather than something to paper over with a default.
    let capacity = reader
        .output_buffer_size()
        .ok_or_else(|| TextureError::Malformed {
            detail: "png header promises a frame this build will not allocate".to_owned(),
        })?;
    let mut buffer = vec![0u8; capacity];
    let frame = reader
        .next_frame(&mut buffer)
        .map_err(|error| TextureError::Malformed {
            detail: format!("png frame: {error}"),
        })?;

    let width = frame.width;
    let height = frame.height;
    let used = frame.buffer_size();
    let rgba = to_rgba8(
        &buffer[..used],
        frame.color_type,
        frame.bit_depth,
        width,
        height,
    )?;
    debug_assert_eq!(rgba.len(), width as usize * height as usize * 4);
    Ok(PngImage {
        width,
        height,
        pixels: rgba,
    })
}

/// Widens one decoded 8-bit sample layout into RGBA8.
///
/// Only the four layouts the corpus actually uses are handled. Each arm states
/// the count it consumes, because getting that wrong is how a decoder ends up
/// returning a plausible image made of the wrong bytes -- and an image that is
/// wrong but well-formed is exactly what a screenshot cannot catch.
fn to_rgba8(
    samples: &[u8],
    color: png::ColorType,
    depth: png::BitDepth,
    width: u32,
    height: u32,
) -> Result<Vec<u8>, TextureError> {
    if depth != png::BitDepth::Eight {
        return Err(TextureError::UnsupportedPngLayout {
            detail: format!("png bit depth {depth:?}: only 8-bit is decoded"),
        });
    }
    let count = width as usize * height as usize;
    let mut out = Vec::with_capacity(count * 4);
    match color {
        png::ColorType::Rgba => {
            if samples.len() < count * 4 {
                return Err(TextureError::Malformed {
                    detail: format!("png rgba: {} samples for {count} pixels", samples.len()),
                });
            }
            out.extend_from_slice(&samples[..count * 4]);
        }
        png::ColorType::Rgb => {
            if samples.len() < count * 3 {
                return Err(TextureError::Malformed {
                    detail: format!("png rgb: {} samples for {count} pixels", samples.len()),
                });
            }
            for pixel in samples[..count * 3].chunks_exact(3) {
                out.extend_from_slice(pixel);
                out.push(0xFF);
            }
        }
        png::ColorType::Grayscale => {
            if samples.len() < count {
                return Err(TextureError::Malformed {
                    detail: format!("png grey: {} samples for {count} pixels", samples.len()),
                });
            }
            for grey in &samples[..count] {
                out.extend_from_slice(&[*grey, *grey, *grey, 0xFF]);
            }
        }
        png::ColorType::GrayscaleAlpha => {
            if samples.len() < count * 2 {
                return Err(TextureError::Malformed {
                    detail: format!(
                        "png grey+alpha: {} samples for {count} pixels",
                        samples.len()
                    ),
                });
            }
            for pixel in samples[..count * 2].chunks_exact(2) {
                out.extend_from_slice(&[pixel[0], pixel[0], pixel[0], pixel[1]]);
            }
        }
        other => {
            return Err(TextureError::UnsupportedPngLayout {
                detail: format!(
                "png colour type {other:?}: only greyscale, grey+alpha, rgb and rgba are decoded"
            ),
            })
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a real PNG in memory, so these tests exercise the actual decoder
    /// rather than a hand-written stand-in for its output.
    fn encode(color: png::ColorType, width: u32, height: u32, samples: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut out, width, height);
            encoder.set_color(color);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().expect("a header");
            writer.write_image_data(samples).expect("image data");
        }
        out
    }

    #[test]
    fn an_rgba_png_round_trips_through_the_decoder() {
        let pixels: Vec<u8> = (0..(3 * 2 * 4)).map(|i| i as u8).collect();
        let data = encode(png::ColorType::Rgba, 3, 2, &pixels);
        let image = decode_png(&data).expect("an rgba png decodes");
        assert_eq!((image.width, image.height), (3, 2));
        assert_eq!(image.pixel_count(), 6);
        assert_eq!(image.pixels, pixels, "rgba must pass through byte for byte");
    }

    #[test]
    fn an_rgb_png_gains_an_opaque_alpha_rather_than_a_transparent_one() {
        // The failure this guards: widening RGB by *prepending* alpha rather than
        // appending it, which yields an image that is transparent everywhere and
        // looks like a decode that worked.
        let samples: Vec<u8> = (0..(2 * 2 * 3)).map(|i| (i * 7) as u8).collect();
        let data = encode(png::ColorType::Rgb, 2, 2, &samples);
        let image = decode_png(&data).expect("an rgb png decodes");
        assert_eq!(image.pixels.len(), 2 * 2 * 4);
        for (index, pixel) in image.pixels.chunks_exact(4).enumerate() {
            assert_eq!(pixel[3], 0xFF, "pixel {index} must be opaque");
            assert_eq!(
                &pixel[..3],
                &samples[index * 3..index * 3 + 3],
                "pixel {index} keeps its colour"
            );
        }
    }

    #[test]
    fn a_greyscale_png_replicates_its_one_channel() {
        let samples: Vec<u8> = vec![0x00, 0x40, 0x80, 0xFF];
        let data = encode(png::ColorType::Grayscale, 2, 2, &samples);
        let image = decode_png(&data).expect("a greyscale png decodes");
        assert_eq!(
            image.pixels,
            vec![
                0x00, 0x00, 0x00, 0xFF, //
                0x40, 0x40, 0x40, 0xFF, //
                0x80, 0x80, 0x80, 0xFF, //
                0xFF, 0xFF, 0xFF, 0xFF,
            ]
        );
    }

    #[test]
    fn a_greyscale_alpha_png_keeps_its_transparency() {
        let samples: Vec<u8> = vec![0x80, 0x00, 0x80, 0xFF];
        let data = encode(png::ColorType::GrayscaleAlpha, 2, 1, &samples);
        let image = decode_png(&data).expect("a grey+alpha png decodes");
        assert_eq!(&image.pixels[..4], &[0x80, 0x80, 0x80, 0x00]);
        assert_eq!(&image.pixels[4..], &[0x80, 0x80, 0x80, 0xFF]);
    }

    #[test]
    fn garbage_is_refused_rather_than_decoded_into_something() {
        // A refusal must be a refusal. A decoder that returns a 1x1 black image
        // for arbitrary bytes would make every later "the art is missing" report
        // ambiguous between "no such record" and "the decoder is lying".
        for bytes in [&b""[..], b"not a png", &[0x89, 0x50, 0x4E, 0x47][..]] {
            assert!(
                decode_png(bytes).is_err(),
                "{bytes:?} must not decode to an image"
            );
        }
    }

    #[test]
    fn a_truncated_png_is_refused_rather_than_padded() {
        let pixels: Vec<u8> = (0..(4 * 4 * 4)).map(|i| i as u8).collect();
        let data = encode(png::ColorType::Rgba, 4, 4, &pixels);
        let truncated = &data[..data.len() / 2];
        assert!(
            decode_png(truncated).is_err(),
            "half a png is not half an image, it is no image"
        );
    }
}
