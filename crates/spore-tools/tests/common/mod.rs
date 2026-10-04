//! Shared fixture builders for the integration tests.
//!
//! Three of the four committed fixtures cannot be used the way a naive test
//! wants to:
//!
//! * `mini_package.dbpf` holds three synthetic records whose type ids
//!   (`TSTX`, `QFS1`, `RAWB`) are absent from the canonical type table, so
//!   `describe`, `find` and `verify` have nothing to dispatch on inside it;
//! * `mini.gmdl` and `mini_rw4.rw4` are *records*, not packages, so no command can
//!   reach them without a container holding them;
//! * there is no raster fixture at all.
//!
//! So [`package_of`] builds a throwaway DBPF image around arbitrary payloads. It
//! is the only encoder in this crate's test surface: the crate itself never
//! writes a package, and a production-side encoder would be a second
//! implementation of a format whose *reader* is the deliverable.
//!
//! The layout mirrors `tests/fixtures/gen_fixtures.py::build_package` byte for
//! byte where it matters, including the `stored_size | 0x8000_0000` bit the
//! reader has to mask off.

#![allow(dead_code)]

use spore_core::ResourceKey;

/// The repository fixture directory.
pub const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/fixtures/");

/// The committed three-record package.
pub const MINI_PACKAGE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/mini_package.dbpf"
);

/// The committed gmdl record.
pub const MINI_GMDL: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/mini.gmdl"
);

/// The committed RW4 record.
pub const MINI_RW4: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/mini_rw4.rw4"
);

/// Reads a committed fixture.
pub fn read_fixture(relative: &str) -> Vec<u8> {
    let path = format!("{FIXTURES}{relative}");
    std::fs::read(&path).unwrap_or_else(|error| panic!("{path}: {error}"))
}

/// One record to place in a synthetic package.
#[derive(Debug, Clone)]
pub struct Row {
    /// Record type id.
    pub type_id: u32,
    /// Group id.
    pub group_id: u32,
    /// Instance id.
    pub instance_id: u32,
    /// The payload bytes, stored uncompressed.
    pub payload: Vec<u8>,
}

impl Row {
    /// A row with an arbitrary payload.
    pub fn new(type_id: u32, group_id: u32, instance_id: u32, payload: Vec<u8>) -> Self {
        Self {
            type_id,
            group_id,
            instance_id,
            payload,
        }
    }

    /// Replaces the payload.
    pub fn with(mut self, payload: Vec<u8>) -> Self {
        self.payload = payload;
        self
    }

    /// The identity this row will have in the built package.
    pub fn key(&self) -> ResourceKey {
        ResourceKey::new(self.type_id, self.group_id, self.instance_id)
    }
}

/// Builds a DBPF v3 image holding `rows`, all stored uncompressed.
///
/// Mirrors `gen_fixtures.py`: 96-byte header, index flags word at `0x60`, plain
/// 28-byte rows from `0x64`, payloads after the index. The stored-size word
/// carries bit 31, which the reader must mask off.
pub fn package_of(rows: &[Row]) -> Vec<u8> {
    let index_offset: u32 = 96;
    let mut payload_offset = index_offset + 4 + 28 * rows.len() as u32;
    let mut index = Vec::new();
    let mut payloads = Vec::new();
    for row in rows {
        index.extend_from_slice(&row.type_id.to_le_bytes());
        index.extend_from_slice(&row.group_id.to_le_bytes());
        index.extend_from_slice(&row.instance_id.to_le_bytes());
        index.extend_from_slice(&payload_offset.to_le_bytes());
        index.extend_from_slice(&(row.payload.len() as u32 | 0x8000_0000).to_le_bytes());
        index.extend_from_slice(&(row.payload.len() as u32).to_le_bytes());
        index.extend_from_slice(&0u16.to_le_bytes());
        index.push(0); // saved
        index.push(0); // padding
        payloads.extend_from_slice(&row.payload);
        payload_offset += row.payload.len() as u32;
    }

    let mut image = vec![0u8; index_offset as usize];
    image[0..4].copy_from_slice(b"DBPF");
    image[4..8].copy_from_slice(&3u32.to_le_bytes()); // major, never read
    image[0x24..0x28].copy_from_slice(&(rows.len() as u32).to_le_bytes());
    image[0x40..0x44].copy_from_slice(&index_offset.to_le_bytes());
    image.extend_from_slice(&0u32.to_le_bytes()); // index flags: nothing shared
    image.extend_from_slice(&index);
    image.extend_from_slice(&payloads);
    image
}

/// A package holding the committed gmdl record at a real gmdl type id.
pub fn gmdl_package() -> Vec<u8> {
    package_of(&[Row::new(
        spore_gmdl::GMDL_TYPE,
        0x4061_6201,
        0x067a_0801,
        read_fixture("mini.gmdl"),
    )])
}

/// A package holding the committed RW4 record at the RW4 type id.
pub fn rw4_package() -> Vec<u8> {
    package_of(&[Row::new(
        spore_rw4::RW4_TYPE,
        0x4062_7100,
        0x067a_0802,
        read_fixture("mini_rw4.rw4"),
    )])
}

/// A synthetic **DXT5** raster record: one mip chain, `mips` levels.
///
/// There is no committed raster fixture — real raster records live only in the
/// git-ignored game tree — so this builds one byte by byte.
pub fn synthetic_raster(width: u32, height: u32, mips: u32) -> Vec<u8> {
    synthetic_raster_of(spore_texture::DXT5_FOURCC, width, height, mips)
}

/// A synthetic raster record of any format `spore_texture` has a frame for.
///
/// # The framing this builds, and the one it used to build
///
/// This builder used to write a **32-byte envelope followed by `layers` × 16
/// "layer headers" and then `layers` mip chains**, with each block charged 8
/// bytes. A corpus byte count refuted that: over 2 954 of 2 954 raster records in
/// `Spore_Content.package` and 1 432 of 1 432 in `Spore_EP1_Content_01.package`
/// the length is exactly
///
/// ```text
/// header + level0 + (u32 count + level m) for every m >= first_prefixed_level
/// ```
///
/// with **one** chain, no layer headers at all, 16 bytes per block for DXT5 and
/// 8 for DXT1. The old builder's two errors cancelled: a 16-byte block read at 8
/// bytes per block is two half-pictures, and two of them make one picture.
///
/// So the builder now asks [`spore_texture::framing`] for every length rather
/// than computing its own — a hand-rolled chain length that disagreed with the
/// decoder's would only prove this builder wrong — and it *asserts* the result
/// is exactly the length the framing identity requires, so a future edit to the
/// builder fails at build time instead of producing a record the decoder
/// refuses.
///
/// # The two header words that are not measurements
///
/// * `field_10` is written as `8`, which is the value every corpus record
///   carries and which no decoder reads.
/// * `field_1c` is written as `0`. For the `0x15` family it is not a header word
///   at all — that header is 28 bytes, so `parse_envelope` reads the first texel
///   of the base level there, and this builder's payload supplies it. For the two
///   block formats it is an unestablished word in the 32-byte header; `0` is
///   written because no value has been established for it and a fixture does not
///   need one. It is never used to select a decode.
pub fn synthetic_raster_of(fourcc: u32, width: u32, height: u32, mips: u32) -> Vec<u8> {
    let frame = spore_texture::framing::frame_for(fourcc)
        .unwrap_or_else(|| panic!("no measured framing for fourcc {fourcc:#010x}"));
    let base = spore_texture::framing::level_size(frame, width, height, 0);

    let mut record = Vec::new();
    for value in [1u32, width, height, mips, 8, fourcc, base as u32] {
        record.extend_from_slice(&value.to_le_bytes());
    }
    // The eighth word only exists for the 32-byte headers. For the 28-byte `0x15`
    // header the payload starts here, and the word `parse_envelope` calls
    // `field_1c` is the base level's first texel — which is what the corpus
    // records hold, and the crate refuses to pretend it is a field.
    if frame.header_size >= spore_texture::ENVELOPE_SIZE {
        record.extend_from_slice(&0u32.to_le_bytes());
    }
    assert_eq!(
        record.len(),
        frame.header_size,
        "the eight header words must exactly fill this format's header, and no more"
    );

    for mip in 0..mips {
        let size = spore_texture::framing::level_size(frame, width, height, mip);
        if mip >= frame.first_prefixed_level {
            record.extend_from_slice(&(size as u32).to_le_bytes());
        }
        // A deterministic ramp, so a decoded mip is reproducible. Block contents
        // are irrelevant to `describe`, which reports dimensions.
        let seed = mip.wrapping_mul(7);
        for byte in 0..size {
            record.push((byte as u32).wrapping_add(seed) as u8);
        }
    }

    assert_eq!(
        record.len(),
        spore_texture::framing::record_size(frame, width, height, mips),
        "the built record must be exactly the length the framing identity requires"
    );
    record
}

/// A package holding the synthetic raster, plus one record of a type with no
/// decoder in this build and one container record.
pub fn raster_package() -> Vec<u8> {
    package_of(&[
        Row::new(
            spore_texture::RASTER_TYPE,
            0x4066_2900,
            0x067a_0901,
            synthetic_raster(32, 16, 3),
        ),
        // `plt`, a named type this build has no decoder for.
        Row::new(
            spore_core::record::type_id::PLT,
            0x406b_6a00,
            0x067a_0902,
            vec![0x11; 40],
        ),
        // `prop`, a container.
        Row::new(spore_core::record::type_id::PROP, 0x0, 0x1, vec![0x22; 8]),
    ])
}

/// Writes `bytes` to a unique temporary file and returns its path.
///
/// The file lives in the OS temp directory with the test's own name in it, so two
/// tests running at once cannot collide. `Drop` removes it.
pub struct TempFile {
    path: std::path::PathBuf,
}

impl TempFile {
    /// Writes `bytes` under a name derived from `tag`.
    pub fn with(tag: &str, bytes: &[u8]) -> Self {
        let mut path = std::env::temp_dir();
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let process = std::process::id();
        path.push(format!("osptool-{process}-{unique}-{tag}.bin"));
        std::fs::write(&path, bytes).expect("temp file must be writable");
        Self { path }
    }

    /// The path written to.
    pub fn path(&self) -> &std::path::Path {
        &self.path
    }

    /// The bytes on disk.
    pub fn read(&self) -> Vec<u8> {
        std::fs::read(&self.path).expect("temp file must be readable")
    }
}

impl Drop for TempFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

/// A temporary directory that removes itself.
pub struct TempDir {
    path: std::path::PathBuf,
}

impl TempDir {
    /// Creates a fresh empty directory under the OS temp directory.
    pub fn new(tag: &str) -> Self {
        let mut path = std::env::temp_dir();
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let process = std::process::id();
        path.push(format!("osptool-{process}-{unique}-{tag}"));
        std::fs::create_dir_all(&path).expect("temp dir must be creatable");
        Self { path }
    }

    /// The directory path.
    pub fn path(&self) -> &std::path::Path {
        &self.path
    }

    /// Every entry currently in the directory, as file names.
    pub fn entries(&self) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(&self.path)
            .expect("temp dir must be readable")
            .map(|entry| entry.expect("entry must be readable").file_name())
            .map(|name| name.to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// Runs a command line and returns `(exit code, stdout, stderr)`.
pub fn osptool(args: &[&str]) -> (i32, String, String) {
    let mut out = Vec::new();
    let mut err = Vec::new();
    let args: Vec<String> = args.iter().map(|a| (*a).to_owned()).collect();
    let code = spore_tools::main_with(args, &mut out, &mut err);
    (
        code,
        String::from_utf8(out).expect("stdout is utf-8"),
        String::from_utf8(err).expect("stderr is utf-8"),
    )
}

/// The same, as `Vec<&str>` so it can go straight into [`osptool`].
///
/// Panics if the path is not UTF-8, which every path these tests build is.
pub fn argv(subcommand: &str, path: &std::path::Path, rest: &[&str]) -> Vec<String> {
    let path = path.to_str().expect("test paths are utf-8").to_owned();
    let mut all = vec![subcommand.to_owned(), path];
    all.extend(rest.iter().map(|a| (*a).to_owned()));
    all
}

/// Runs a command line built from owned strings.
pub fn run_owned(args: Vec<String>) -> (i32, String, String) {
    let mut out = Vec::new();
    let mut err = Vec::new();
    let code = spore_tools::main_with(args, &mut out, &mut err);
    (
        code,
        String::from_utf8(out).expect("stdout is utf-8"),
        String::from_utf8(err).expect("stderr is utf-8"),
    )
}

/// Writes a package image to a temporary file so a command can open it by path.
pub fn temp_package(tag: &str, image: &[u8]) -> TempFile {
    TempFile::with(tag, image)
}
