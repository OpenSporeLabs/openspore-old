//! Shared fixtures for the integration tests.
//!
//! Everything here is **synthetic**. No Spore asset byte and no committed
//! snapshot line is copied into a test: the synthetic records are written by
//! hand from the schema, and the committed 98 MB artifact is only ever *read*,
//! and only by the tests that are explicitly about it.

#![allow(dead_code)]

use std::path::{Path, PathBuf};

/// A temporary file that removes itself.
///
/// Hand-rolled because this crate's test dependencies are `spore-core` and
/// `thiserror` and nothing else — no `tempfile`.
pub struct TempFile {
    path: PathBuf,
}

impl TempFile {
    /// Writes `contents` to a uniquely named temporary file.
    pub fn new(label: &str, contents: &str) -> Self {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "spore-semantic-bridge-{label}-{}-{:p}.jsonl",
            std::process::id(),
            &contents
        ));
        std::fs::write(&path, contents).expect("the temporary fixture is writable");
        Self { path }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

/// The metadata line, with every counter a synthetic file might need.
///
/// `content_sha256` is a placeholder by default: no synthetic test needs a real
/// digest, and the one that checks the digest path asserts the mismatch.
pub fn metadata_line(functions: usize, content_sha256: &str) -> String {
    let counts = [
        ("functions", functions),
        ("with_sdk_name", 0),
        ("with_abi_data", 0),
        ("with_abi_unknown", 0),
        ("with_receiver", 0),
        ("with_receiver_unknown", 0),
        ("with_vtable_membership", 0),
        ("with_vtable_attributed", 0),
        ("with_subsystem", 0),
        ("with_renderware_role", 0),
        ("with_reconstruction_package", 0),
        ("with_promotion", 0),
        ("with_evidence_pack", 0),
        ("with_validation", 0),
        ("with_semantics", 0),
        ("with_globals", 0),
        ("with_types", 0),
        ("with_callers", 0),
        ("with_callees", 0),
        ("with_refuted_identity", 0),
        ("resolved_interior_addresses", 0),
        ("attached_stray_evidence_packs", 0),
        ("explicitly_unavailable", 0),
        ("total_fact_groups", 0),
    ];
    let counters = counts
        .iter()
        .map(|(key, value)| format!("\"{key}\":{value}"))
        .collect::<Vec<_>>()
        .join(",");
    let header = concat!(
        r#"{"record":"metadata","schema":"spore-semantic-snapshot-1","#,
        r#""binary":{"binary_sha256":"25d42a7a5c4d438fb155233230f57d29e2849bfdff5c889a5d0847f0469d914e","#,
        r#""image_base":"0x00400000","architecture":"x86:LE:32","program":"SporeApp.exe","version":"3.1.0.22"},"#,
        r#""generator":{"name":"spore-semantic","version":"1"},"inputs":[],"absent_inputs":[],"#,
        r#""counts":{COUNTERS},"content_sha256":"DIGEST","#,
        r#""export":{"universe_source":".spore-analysis/ghidra-exports/functions.tsv","#,
        r#""universe_fallback":false,"evidence_pack_dirs_read":0,"orphan_evidence_packs":0,"#,
        r#""orphan_evidence_pack_paths":[]}}"#,
    );
    header
        .replace("COUNTERS", &counters)
        .replace("DIGEST", content_sha256)
}

/// A minimal, fully valid function record.
///
/// Every fact group is published as an explicit absence and every optional list
/// is omitted, which is the shape 58 085 of the committed records actually have.
pub fn record(va: &str, rva: &str, size: i64) -> String {
    format!(
        concat!(
            r#"{{"record":"function","#,
            r#""identity":{{"canonical_va":"{va}","rva":"{rva}","size":{size},"size_known":true,"#,
            r#""is_thunk":false,"section":".text","identity_resolution":null}},"#,
            r#""names":{{"ghidra_name":"FUN_{bare}","normalized_symbol":"FUN_{bare}","sdk_name":null,"#,
            r#""identity_refuted":null}},"#,
            r#""classification":{{"category":"ENGINE_IMPLEMENTATION","priority":"P3","evidence":"INFERRED","#,
            r#""subsystem":"App","cluster":null,"renderware_role":null,"subsystem_source":"triage_jsonl"}},"#,
            r#""abi":{{"state":"unavailable","evidence_level":"UNKNOWN","evidence_state":"MISSING","#,
            r#""reason":"no_evidence_pack"}},"#,
            r#""vtable":{{"state":"unavailable","evidence_level":"UNKNOWN","evidence_state":"MISSING","#,
            r#""reason":"no_vtable_membership"}},"#,
            r#""graph":{{"caller_count":0,"callee_count":0,"vtable_reference_count":0,"data_reference_count":0}},"#,
            r#""globals":{{"state":"unavailable","evidence_level":"UNKNOWN","evidence_state":"MISSING","#,
            r#""reason":"no_global_references"}},"#,
            r#""types":{{"state":"unavailable","evidence_level":"UNKNOWN","evidence_state":"MISSING","#,
            r#""reason":"no_associated_types"}},"#,
            r#""semantics":{{"state":"unavailable","evidence_level":"UNKNOWN","evidence_state":"MISSING","#,
            r#""reason":"no_semantic_record"}},"#,
            r#""reconstruction":{{"state":"unavailable","evidence_level":"UNKNOWN","evidence_state":"MISSING","#,
            r#""reason":"no_reconstruction_package"}},"#,
            r#""evidence":{{"pack":null,"pack_state":null,"pack_content_sha256":null,"#,
            r#""target_address_kind":null,"validation_report":null,"promotion_record":null}}}}"#
        ),
        va = va,
        rva = rva,
        size = size,
        bare = va.trim_start_matches("0x"),
    )
}

/// Assembles a whole snapshot file from record lines.
pub fn snapshot_file(records: &[String]) -> String {
    let mut out = metadata_line(records.len(), &"0".repeat(64));
    out.push('\n');
    for record in records {
        out.push_str(record);
        out.push('\n');
    }
    out
}

// ---------------------------------------------------------------------------
// A record builder, for the fact groups the contract tests need
// ---------------------------------------------------------------------------

/// How the `abi` group should be published.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AbiShape {
    /// An explicit absence with this reason code.
    Absent(&'static str),
    /// Available, naming a convention.
    Known {
        /// The convention, or `None` for an undetermined record.
        convention: Option<&'static str>,
        /// The convention's grade.
        confidence: &'static str,
    },
}

/// How the `vtable` group should be published.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VtShape {
    /// An explicit absence.
    Absent,
    /// Available with these `(table_va, slot, slot_width)` memberships.
    Memberships(Vec<(u32, u32, Option<u32>)>),
}

/// How the `reconstruction` group's runtime gate should be published.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GateShape {
    /// No reconstruction record at all.
    Absent,
    /// A record that states the gate, or states nothing about it.
    Stated(Option<bool>),
}

/// Composes one fully valid synthetic record.
///
/// The shape [`record`] writes is the shape 58,085 of the committed records have:
/// every fact group an explicit absence. This builder exists because the contract
/// layer needs the other shapes — a named record, a table membership, an ABI
/// record, a stated gate — and hand-writing four JSON blobs per test is how a
/// fixture starts disagreeing with the schema.
#[derive(Debug, Clone)]
pub struct RecordBuilder {
    va: String,
    rva: String,
    size: i64,
    sdk_name: Option<String>,
    sdk_name_source: Option<String>,
    category: &'static str,
    evidence: &'static str,
    subsystem: Option<&'static str>,
    abi: AbiShape,
    vtable: VtShape,
    gate: GateShape,
}

impl RecordBuilder {
    /// A record with every group absent, at `va` with `size` bytes.
    pub fn new(va: &str, rva: &str, size: i64) -> Self {
        Self {
            va: va.to_owned(),
            rva: rva.to_owned(),
            size,
            sdk_name: None,
            sdk_name_source: None,
            category: "ENGINE_IMPLEMENTATION",
            evidence: "INFERRED",
            subsystem: Some("App"),
            abi: AbiShape::Absent("no_evidence_pack"),
            vtable: VtShape::Absent,
            gate: GateShape::Absent,
        }
    }

    /// States an SDK name, with the artifact that stated it.
    pub fn sdk(mut self, name: &str) -> Self {
        self.sdk_name = Some(name.to_owned());
        self.sdk_name_source = Some("sdk_functions.tsv".to_owned());
        self
    }

    /// Sets the triage classifier's own view.
    pub fn classified(mut self, category: &'static str, evidence: &'static str) -> Self {
        self.category = category;
        self.evidence = evidence;
        self
    }

    /// Sets the subsystem, or `None` for the source's own `"Unknown"` sentinel.
    pub fn subsystem(mut self, subsystem: Option<&'static str>) -> Self {
        self.subsystem = subsystem;
        self
    }

    /// Publishes an ABI record naming a convention.
    pub fn abi_known(self, convention: &'static str, confidence: &'static str) -> Self {
        Self {
            abi: AbiShape::Known {
                convention: Some(convention),
                confidence,
            },
            ..self
        }
    }

    /// Publishes an ABI record whose convention is `null`.
    pub fn abi_undetermined(self, confidence: &'static str) -> Self {
        Self {
            abi: AbiShape::Known {
                convention: None,
                confidence,
            },
            ..self
        }
    }

    /// Publishes table memberships.
    pub fn vtable(self, memberships: &[(u32, u32, Option<u32>)]) -> Self {
        Self {
            vtable: VtShape::Memberships(memberships.to_vec()),
            ..self
        }
    }

    /// Publishes a reconstruction record, with or without a stated gate.
    pub fn runtime_gate(self, gated: Option<bool>) -> Self {
        Self {
            gate: GateShape::Stated(gated),
            ..self
        }
    }

    /// The record line.
    pub fn build(&self) -> String {
        let bare = self.va.trim_start_matches("0x").to_owned();
        // The trailing comma lives with the value, so the `names` object closes
        // correctly whether or not a name and a source were stated.
        let sdk_name = self
            .sdk_name
            .as_ref()
            .map_or_else(|| "null,".to_owned(), |name| format!("\"{name}\""));
        let sdk_source = self
            .sdk_name_source
            .as_ref()
            .map_or_else(String::new, |source| {
                format!(",\"sdk_name_source\":\"{source}\",")
            });
        let subsystem = self
            .subsystem
            .map_or_else(|| "null".to_owned(), |value| format!("\"{value}\""));

        let abi = match &self.abi {
            AbiShape::Absent(reason) => absent_envelope(reason),
            AbiShape::Known {
                convention,
                confidence,
            } => {
                let confidence = *confidence;
                let convention =
                    convention.map_or_else(|| "null".to_owned(), |c| format!("\"{c}\""));
                format!(
                    concat!(
                        r#"{{"state":"available","evidence_level":"{level}","evidence_state":"DERIVED","#,
                        r#""provenance":["synthetic-fixture"],"value":{{"origin":"abi_inference","#,
                        r#""schema":"openspore-abi-inference-1","convention":{convention},"#,
                        r#""convention_confidence":"{confidence}","verdict":"ABI_TEST","#,
                        r#""completeness":"CORE_RESOLVED","receiver":null,"cleanup":null,"#,
                        r#""return":null,"sret":null,"variadic":"UNKNOWN","#,
                        r#""stack_argument_slots":null,"seh_or_cookie_frame":null,"declared":null}}}}"#
                    ),
                    level = if confidence == "UNKNOWN" {
                        "INFERRED"
                    } else {
                        confidence
                    },
                    convention = convention,
                    confidence = confidence,
                )
            }
        };

        let vtable = match &self.vtable {
            VtShape::Absent => absent_envelope("no_vtable_membership"),
            VtShape::Memberships(memberships) => {
                let items = memberships
                    .iter()
                    .map(|(table, slot, width)| {
                        let width = width.map_or_else(|| "null".to_owned(), |w| w.to_string());
                        format!(
                            r#"{{"table_va":"0x{table:08x}","slot":{slot},"slot_width":{width}}}"#
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(",");
                format!(
                    concat!(
                        r#"{{"state":"available","evidence_level":"INFERRED","evidence_state":"DERIVED","#,
                        r#""provenance":["synthetic-fixture"],"value":{{"memberships":[{items}],"#,
                        r#""abi_attributed":null,"triage_addresses":[],"triage_family":null}}}}"#
                    ),
                    items = items
                )
            }
        };

        let reconstruction = match &self.gate {
            GateShape::Absent => absent_envelope("no_reconstruction_package"),
            GateShape::Stated(gated) => {
                let gated = gated.map_or_else(|| "null".to_owned(), |value| value.to_string());
                format!(
                    concat!(
                        r#"{{"state":"available","evidence_level":"VERIFIED","evidence_state":"PERSISTED","#,
                        r#""provenance":["synthetic-fixture"],"value":{{"package":"synthetic","#,
                        r#""status":"implemented","review_status":"reviewed","integration_status":"integrated","#,
                        r#""reconstructed":true,"blocked":false,"promoted":"true","promotion_schema":null,"#,
                        r#""promotion_supersedes":null,"static_status":"PASS","generated_source":[],"#,
                        r#""handoffs":[],"metadata":[],"runtime_gated":{gated},"runtime_validated":null,"#,
                        r#""validation":null}}}}"#
                    ),
                    gated = gated
                )
            }
        };

        format!(
            concat!(
                r#"{{"record":"function","#,
                r#""identity":{{"canonical_va":"{va}","rva":"{rva}","size":{size},"size_known":true,"#,
                r#""is_thunk":false,"section":".text","identity_resolution":null}},"#,
                r#""names":{{"ghidra_name":"FUN_{bare}","normalized_symbol":"FUN_{bare}","sdk_name":{sdk_name}"#,
                r#"{sdk_source}"identity_refuted":null}},"#,
                r#""classification":{{"category":"{category}","priority":"P3","evidence":"{evidence}","#,
                r#""subsystem":{subsystem},"cluster":null,"renderware_role":null,"#,
                r#""subsystem_source":"triage_jsonl"}},"#,
                r#""abi":{abi},"#,
                r#""vtable":{vtable},"#,
                r#""graph":{{"caller_count":0,"callee_count":0,"vtable_reference_count":0,"data_reference_count":0}},"#,
                r#""globals":{{"state":"unavailable","evidence_level":"UNKNOWN","evidence_state":"MISSING","#,
                r#""reason":"no_global_references"}},"#,
                r#""types":{{"state":"unavailable","evidence_level":"UNKNOWN","evidence_state":"MISSING","#,
                r#""reason":"no_associated_types"}},"#,
                r#""semantics":{{"state":"unavailable","evidence_level":"UNKNOWN","evidence_state":"MISSING","#,
                r#""reason":"no_semantic_record"}},"#,
                r#""reconstruction":{reconstruction},"#,
                r#""evidence":{{"pack":null,"pack_state":null,"pack_content_sha256":null,"#,
                r#""target_address_kind":null,"validation_report":null,"promotion_record":null}}}}"#
            ),
            va = self.va,
            rva = self.rva,
            size = self.size,
            bare = bare,
            sdk_name = sdk_name,
            sdk_source = sdk_source,
            category = self.category,
            evidence = self.evidence,
            subsystem = subsystem,
            abi = abi,
            vtable = vtable,
            reconstruction = reconstruction,
        )
    }
}

/// The canonical explicit-absence envelope, for a group this fixture leaves empty.
fn absent_envelope(reason: &'static str) -> String {
    format!(
        concat!(
            r#"{{"state":"unavailable","evidence_level":"UNKNOWN","evidence_state":"MISSING","#,
            r#""reason":"{reason}"}}"#
        ),
        reason = reason
    )
}

/// Two entries far enough apart to leave a gap, for the canonicalization rules.
pub fn two_entry_file() -> TempFile {
    TempFile::new(
        "two-entry",
        &snapshot_file(&[
            record("0x00401000", "0x00001000", 0x4a),
            record("0x004010c0", "0x000010c0", 0x20),
        ]),
    )
}

/// A snapshot plus the temporary file it reads.
///
/// The [`Snapshot`] deliberately holds a *path*, not an open handle, so the file
/// has to outlive it — and a fixture that deleted itself at the end of `open()`
/// would have turned every later lookup into a `NotFound`. Returning both is the
/// honest shape, and it documents the lifetime rule for real callers too.
pub struct Fixture {
    pub file: TempFile,
    pub snapshot: Result<spore_semantic_bridge::Snapshot, spore_semantic_bridge::BridgeError>,
}

impl Fixture {
    /// The opened snapshot, panicking with the typed error on failure.
    pub fn snapshot(&self) -> &spore_semantic_bridge::Snapshot {
        self.opened()
            .unwrap_or_else(|error| panic!("the fixture must open: {error}"))
    }

    /// The opened snapshot, or the typed error it refused with.
    pub fn opened(
        &self,
    ) -> Result<&spore_semantic_bridge::Snapshot, spore_semantic_bridge::BridgeError> {
        // Cloning the error out keeps the borrow tied to the fixture, which is
        // what has to outlive the snapshot anyway.
        match &self.snapshot {
            Ok(snapshot) => Ok(snapshot),
            Err(error) => Err(clone_error(error)),
        }
    }
}

/// Rebuilds an error so a fixture can hand it out by value.
fn clone_error(error: &spore_semantic_bridge::BridgeError) -> spore_semantic_bridge::BridgeError {
    use spore_semantic_bridge::BridgeError;
    match error {
        BridgeError::SnapshotUnavailable { path } => {
            BridgeError::SnapshotUnavailable { path: path.clone() }
        }
        BridgeError::Corrupt { path, line, detail } => BridgeError::Corrupt {
            path: path.clone(),
            line: *line,
            detail: detail.clone(),
        },
        BridgeError::UnsupportedSchema { path, line, detail } => BridgeError::UnsupportedSchema {
            path: path.clone(),
            line: *line,
            detail: detail.clone(),
        },
        BridgeError::ContentDigestMismatch {
            path,
            declared,
            computed,
        } => BridgeError::ContentDigestMismatch {
            path: path.clone(),
            declared: declared.clone(),
            computed: computed.clone(),
        },
        BridgeError::RecordCountMismatch {
            path,
            declared,
            computed,
        } => BridgeError::RecordCountMismatch {
            path: path.clone(),
            declared: *declared,
            computed: *computed,
        },
        BridgeError::BinaryMismatch { snapshot, required } => BridgeError::BinaryMismatch {
            snapshot: snapshot.clone(),
            required: required.clone(),
        },
        BridgeError::UnknownFunction {
            requested,
            image_base,
            indexed,
        } => BridgeError::UnknownFunction {
            requested: *requested,
            image_base: *image_base,
            indexed: *indexed,
        },
        BridgeError::UnknownSymbol { name } => BridgeError::UnknownSymbol { name: name.clone() },
        BridgeError::AmbiguousSymbol { name, matches } => BridgeError::AmbiguousSymbol {
            name: name.clone(),
            matches: *matches,
        },
        BridgeError::NonCanonicalVa { path, source } => BridgeError::NonCanonicalVa {
            path: path.clone(),
            source: source.clone(),
        },
        BridgeError::Io { path, .. } => BridgeError::SnapshotUnavailable { path: path.clone() },
        other => panic!("unhandled error variant in the test helper: {other:?}"),
    }
}

/// Opens a fixture over exactly these record lines.
pub fn fixture(records: &[String]) -> Fixture {
    let file = TempFile::new("strict", &snapshot_file(records));
    let snapshot = spore_semantic_bridge::Snapshot::open(file.path());
    Fixture { file, snapshot }
}

/// Opens a fixture over one record line.
pub fn one(body: &str) -> Fixture {
    fixture(&[body.to_owned()])
}

/// The location of the committed 98 MB snapshot, or `None` when absent.
///
/// The artifact is optional, so every test that reads it skips loudly rather
/// than failing: a machine without the file must still run this suite green,
/// and one that silently passed without testing anything would be worse.
pub fn committed_path() -> Option<PathBuf> {
    let mut roots: Vec<PathBuf> = Vec::new();
    if let Some(root) = std::env::var_os("OPENSPORE_ROOT") {
        roots.push(PathBuf::from(root));
    }
    let mut cursor = std::env::current_dir().ok();
    while let Some(dir) = cursor {
        roots.push(dir.clone());
        cursor = dir.parent().map(Path::to_path_buf);
    }
    roots
        .into_iter()
        .map(|root| root.join("knowledge/semantic/function-passport-v1.jsonl"))
        .find(|candidate| candidate.is_file())
}
