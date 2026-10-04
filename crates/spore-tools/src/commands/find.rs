//! `find`: resolve one identity and report what the container says about it.
//!
//! Three questions, one command: *which package answered*, *where is it and how
//! big*, and *does it decode*. The third is a verdict, not an error — a record
//! that is present but unreadable is a finding, not a failed command, and the
//! exit code stays [`EXIT_OK`](crate::EXIT_OK) so a sweep over many identities
//! is not aborted by the first bad record. `describe` is the command where a
//! decode failure is fatal.
//!
//! The miss path prints `AssetError::NotFound`'s own message plus the candidate
//! identities the store already collected. The candidates are **not**
//! recomputed here.

use std::io::Write;

use spore_core::record::RecordType;
use spore_dbpf::DbpfEntry;

use crate::json::{hex_id, Json};
use crate::{FindRequest, ToolError};

use super::describe_type;
use spore_assets::AssetError;

/// The verdict `find` reaches about a record's payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecodeVerdict {
    /// A decoder in this build accepted the record and produced this summary.
    Decoded(String),
    /// This build has no decoder for the record's type id.
    NoDecoder {
        /// The id that stopped the lookup.
        type_id: u32,
    },
    /// This build recognises the id as a *container* and does not decode it.
    Container {
        /// The container id.
        type_id: u32,
    },
    /// A decoder exists and refused the record.
    Failed(String),
}

impl DecodeVerdict {
    /// The word a human line leads with, so the reader never has to guess
    /// whether the following numbers came out of a decoder.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Decoded(_) => "decoded",
            Self::NoDecoder { .. } => "not decoded",
            Self::Container { .. } => "not decoded",
            Self::Failed(_) => "decode FAILED",
        }
    }

    /// The one-line detail.
    pub fn detail(&self) -> String {
        match self {
            Self::Decoded(summary) => summary.clone(),
            Self::NoDecoder { type_id } => format!(
                "type-level only: {} has no decoder in this build, so nothing was parsed",
                describe_type(*type_id)
            ),
            Self::Container { type_id } => format!(
                "type-level only: {} is a container this build does not decode",
                describe_type(*type_id)
            ),
            Self::Failed(message) => message.clone(),
        }
    }
}

/// `find`: resolve, extract, decode, report.
pub fn run(request: &FindRequest, out: &mut dyn Write) -> Result<(), ToolError> {
    let store = super::single_package_store(&request.package)?;

    // `find` over a store so a miss carries the library's near-miss candidates
    // and the "which package" line is the store's own priority answer.
    let found = store.find(&request.key).map_err(ToolError::from)?;
    let entry = found.entry;

    // Reading the bytes is where a corrupt extent or an unsupported compression
    // word surfaces, and that is a decode failure (exit 4) even though the
    // identity was found: the record is not retrievable.
    let bytes = store
        .read_from(found.package_index, &request.key)
        .map_err(classify_read_error)?;
    let verdict = decode_verdict(entry, &bytes);

    if request.json {
        writeln!(out, "{}", find_json(found, entry, bytes.len(), &verdict))?;
        return Ok(());
    }

    writeln!(out, "found {}", request.key)?;
    writeln!(
        out,
        "  package       {} (priority {} of {})",
        found.package_name,
        found.package_index,
        store.len()
    )?;
    writeln!(out, "  type          {}", describe_type(entry.type_id))?;
    writeln!(out, "  group         {}", describe_group(entry))?;
    writeln!(out, "  instance      0x{:08x}", entry.instance_id)?;
    writeln!(out, "  offset        0x{:08x}", entry.offset)?;
    writeln!(out, "  stored size   {} bytes", entry.stored_size)?;
    writeln!(out, "  memory size   {} bytes", entry.memory_size)?;
    writeln!(out, "  compression   {}", super::compression_text(entry))?;
    writeln!(out, "  bytes read    {} bytes", bytes.len())?;
    writeln!(out, "  decode        {}", verdict.label())?;
    writeln!(out, "                {}", verdict.detail())?;
    Ok(())
}

/// `AssetError::NotFound` cannot arise from `read_from` for an identity `find`
/// just resolved, but its type is `AssetError`, so the classification is
/// explicit rather than a blanket `Decode`.
fn classify_read_error(error: AssetError) -> ToolError {
    match error {
        AssetError::NotFound { .. } | AssetError::EmptyStore => ToolError::from(error),
        other => ToolError::Decode(other.to_string()),
    }
}

/// A group's id and canonical name.
fn describe_group(entry: &DbpfEntry) -> String {
    match super::group_name_of(entry.group_id) {
        Some(name) => format!("0x{:08x} ({name})", entry.group_id),
        None => format!("0x{:08x}", entry.group_id),
    }
}

/// Runs the type-appropriate decoder over already-extracted record bytes.
///
/// `find` reads the whole payload once and then answers from it, so the verdict
/// never depends on a second read that could disagree.
pub fn decode_verdict(entry: &DbpfEntry, bytes: &[u8]) -> DecodeVerdict {
    match entry.type_id {
        spore_gmdl::GMDL_TYPE => match spore_gmdl::parse(bytes) {
            Err(error) => DecodeVerdict::Failed(format!("gmdl: {error}")),
            Ok(model) => {
                let mut summary = format!(
                    "gmdl version {} with {} mesh(es)",
                    model.version, model.mesh_count
                );
                match mesh_summary(&model) {
                    Some(detail) => {
                        summary.push_str(", ");
                        summary.push_str(&detail);
                    }
                    None => summary.push_str(", no mesh could be extracted from the model tables"),
                }
                DecodeVerdict::Decoded(summary)
            }
        },
        spore_rw4::RW4_TYPE => match spore_rw4::parse(bytes) {
            Err(error) => DecodeVerdict::Failed(format!("rw4: {error}")),
            Ok(directory) => DecodeVerdict::Decoded(format!(
                "rw4 section directory: {} section row(s), is_complete={}",
                directory.sections.len(),
                directory.is_complete()
            )),
        },
        spore_texture::RASTER_TYPE => match spore_texture::decode_raster(bytes) {
            Err(error) => DecodeVerdict::Failed(format!("texture: {error}")),
            Ok(image) => {
                let mips = image.layers.iter().map(Vec::len).sum::<usize>();
                DecodeVerdict::Decoded(format!(
                    "raster {}x{} base level, {mips} mip level(s) in one chain",
                    image.envelope.width, image.envelope.height
                ))
            }
        },
        other => {
            if super::verify::is_container_type(other) {
                DecodeVerdict::Container { type_id: other }
            } else {
                DecodeVerdict::NoDecoder { type_id: other }
            }
        }
    }
}

/// A mesh-level summary, when at least one mesh extracts.
///
/// A gmdl can parse cleanly and still yield no mesh: `mesh_from_gmdl` refuses
/// an unsupported topology or index width, and "parsed but nothing drawable" is
/// a real state that a mesh-count alone would hide.
fn mesh_summary(model: &spore_gmdl::GmdlModel) -> Option<String> {
    let mut parts = Vec::new();
    for index in 0..model.mesh_count {
        let Ok(mesh) = spore_gmdl::mesh_from_gmdl(model, index) else {
            continue;
        };
        parts.push(format!(
            "mesh {index}: {} vertices / {} indices",
            mesh.positions.len(),
            mesh.indices.len()
        ));
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join(", "))
    }
}

/// The `--json` view. Fixed key order; ids are hex strings.
fn find_json(
    found: spore_assets::RecordRef<'_>,
    entry: &DbpfEntry,
    bytes_read: usize,
    verdict: &DecodeVerdict,
) -> Json {
    Json::obj()
        .push("found", Json::from(true))
        .push("key", Json::from(entry.key.to_tgi()))
        .push("package", Json::from(found.package_name))
        .push("package_priority", Json::from(found.package_index))
        .push("packages_searched", Json::from(found.package_index + 1))
        .push("type_id", hex_id(entry.type_id))
        .push(
            "type_name",
            Json::from(RecordType::new(entry.type_id).name()),
        )
        .push("group_id", hex_id(entry.group_id))
        .push(
            "group_name",
            Json::from(super::group_name_of(entry.group_id)),
        )
        .push("instance_id", hex_id(entry.instance_id))
        .push("offset", hex_id(entry.offset))
        .push("stored_size", Json::from(entry.stored_size))
        .push("memory_size", Json::from(entry.memory_size))
        .push("compression", hex_id(u32::from(entry.compression)))
        .push("compressed", Json::from(entry.compressed))
        .push("bytes_read", Json::from(bytes_read))
        .push("decode", Json::from(verdict.label()))
        .push("decode_detail", Json::from(verdict.detail()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_container_id_and_an_unknown_id_are_different_verdicts() {
        let container = DbpfEntry::new(spore_core::record::type_id::PROP, 1, 2, 0, 0, 0, 0, false);
        assert_eq!(
            decode_verdict(&container, b"not a prop"),
            DecodeVerdict::Container {
                type_id: spore_core::record::type_id::PROP
            }
        );
        let unknown = DbpfEntry::new(0xdead_beef, 1, 2, 0, 0, 0, 0, false);
        assert_eq!(
            decode_verdict(&unknown, b"whatever"),
            DecodeVerdict::NoDecoder {
                type_id: 0xdead_beef
            }
        );
        // Both say "not decoded", and neither claims to have parsed anything.
        for verdict in [
            DecodeVerdict::Container { type_id: 1 },
            DecodeVerdict::NoDecoder { type_id: 1 },
        ] {
            assert_eq!(verdict.label(), "not decoded");
            assert!(verdict.detail().contains("type-level only"));
        }
    }

    #[test]
    fn a_malformed_gmdl_payload_is_a_failure_not_a_no_decoder() {
        let entry = DbpfEntry::new(spore_gmdl::GMDL_TYPE, 1, 2, 0, 3, 3, 0, false);
        match decode_verdict(&entry, b"abc") {
            DecodeVerdict::Failed(message) => assert!(message.starts_with("gmdl:")),
            other => panic!("expected a failure, got {other:?}"),
        }
    }

    #[test]
    fn the_real_fixture_gmdl_reports_decoded_numbers() {
        let gmdl = std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../tests/fixtures/mini.gmdl"
        ))
        .unwrap();
        let entry = DbpfEntry::new(spore_gmdl::GMDL_TYPE, 1, 2, 0, 4, 4, 0, false);
        let DecodeVerdict::Decoded(summary) = decode_verdict(&entry, &gmdl) else {
            panic!("expected the fixture to decode")
        };
        assert!(summary.contains("version 8"), "{summary}");
        assert!(summary.contains("1 mesh"), "{summary}");
        assert!(summary.contains("50 vertices"), "{summary}");
        assert!(summary.contains("153 indices"), "{summary}");
    }

    #[test]
    fn the_real_fixture_rw4_and_raster_are_decoded_by_type() {
        let rw4 = std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../tests/fixtures/mini_rw4.rw4"
        ))
        .unwrap();
        let entry = DbpfEntry::new(spore_rw4::RW4_TYPE, 1, 2, 0, 4, 4, 0, false);
        let DecodeVerdict::Decoded(summary) = decode_verdict(&entry, &rw4) else {
            panic!("expected the fixture to decode")
        };
        assert!(summary.contains("3 section row(s)"), "{summary}");
        assert!(summary.contains("is_complete=true"), "{summary}");
    }
}
