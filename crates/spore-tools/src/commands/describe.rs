//! `describe`: decode one record according to its type id and print its shape.
//!
//! # The rule this command exists to enforce
//!
//! Spore's record types are decodable in wildly different ways, and a summary
//! that reads the same for a decoded geometry container and for a record nothing
//! in this build can read is a lie with a nice-looking number in it. So every
//! section is introduced by one of three explicitly different openings:
//!
//! | opening | means |
//! |---|---|
//! | `decoder: gmdl (this build decodes 0x00e6bce5)` | a decoder exists; everything after it is **decoded** |
//! | `decoder: container (0x00b1b104 prop) — payload not decoded` | the id is a container; the bytes below were **not** parsed |
//! | `decoder: none for 0x011989b7 (plt)` | this build has no decoder for the id; **nothing was parsed** |
//!
//! Only the first is followed by numbers that came out of a decoder. The other
//! two print what the *index* says (identity, stored/memory size, compression)
//! and label it as index-level.
//!
//! # Which ids get a decoder here
//!
//! Strictly by id: [`spore_gmdl::GMDL_TYPE`], [`spore_rw4::RW4_TYPE`] and
//! [`spore_texture::RASTER_TYPE`].
//!
//! In particular a `png`-typed record (`0x2F7D0004`) is **not** sent to the
//! raster decoder, and not because it is an RW4 container -- an earlier
//! revision of this file claimed that, wrongly. `png` is raw PNG (10 487 of
//! 10 487 measured), which the DXT5 codec would misread as a confident wrong
//! answer; `plt` has no decoder at all. Both are refused by name.

use std::io::Write;

use spore_dbpf::DbpfEntry;

use crate::{RecordRequest, ToolError};

use super::describe_type;

/// `describe`: resolve, read, dispatch on the type id, print.
pub fn run(request: &RecordRequest, out: &mut dyn Write) -> Result<(), ToolError> {
    let store = super::single_package_store(&request.package)?;
    let found = store.find(&request.key)?;
    let entry = found.entry;
    let bytes = store.read_from(found.package_index, &request.key)?;

    writeln!(out, "record      {}", request.key)?;
    writeln!(out, "package     {}", found.package_name)?;
    writeln!(out, "type        {}", describe_type(entry.type_id))?;
    writeln!(out, "group       {}", describe_group(entry))?;
    writeln!(out, "instance    0x{:08x}", entry.instance_id)?;
    writeln!(
        out,
        "index extent            0x{:08x}+{} ({} bytes)",
        entry.offset, entry.stored_size, entry.stored_size
    )?;
    writeln!(out, "index memory size       {} bytes", entry.memory_size)?;
    writeln!(
        out,
        "index compression       {}",
        super::compression_text(entry)
    )?;

    match entry.type_id {
        spore_gmdl::GMDL_TYPE => {
            writeln!(out)?;
            let model = spore_gmdl::parse(&bytes).map_err(|error| {
                ToolError::Decode(format!("record `{key}`: gmdl: {error}", key = request.key))
            })?;
            write_gmdl(out, &bytes, &model)?;
        }
        spore_rw4::RW4_TYPE => {
            writeln!(out)?;
            let directory = spore_rw4::parse(&bytes).map_err(|error| {
                ToolError::Decode(format!("record `{key}`: rw4: {error}", key = request.key))
            })?;
            write_rw4(out, &directory)?;
        }
        spore_texture::RASTER_TYPE => {
            writeln!(out)?;
            let image = spore_texture::decode_raster(&bytes).map_err(|error| {
                ToolError::Decode(format!(
                    "record `{key}`: texture: {error}",
                    key = request.key
                ))
            })?;
            write_raster(out, &image)?;
        }
        other => {
            writeln!(out)?;
            if super::verify::is_container_type(other) {
                writeln!(
                    out,
                    "decoder: container ({} \u{2014} payload NOT decoded)",
                    describe_type(other)
                )?;
                writeln!(
                    out,
                    "  The {} bytes below are the index's memory_size. This build does not",
                    entry.memory_size
                )?;
                writeln!(
                    out,
                    "  parse this container, so nothing further is claimed about the record."
                )?;
            } else {
                writeln!(out, "decoder: none for {}", describe_type(other))?;
                writeln!(
                    out,
                    "  The {} bytes below are the index's memory_size, not the result of a",
                    entry.memory_size
                )?;
                writeln!(out, "  parse. This build has no decoder for that type id.")?;
            }
        }
    }
    Ok(())
}

/// A group's id plus its canonical name when one exists.
fn describe_group(entry: &DbpfEntry) -> String {
    match super::group_name_of(entry.group_id) {
        Some(name) => format!("0x{:08x} ({name})", entry.group_id),
        None => format!("0x{:08x}", entry.group_id),
    }
}

/// The gmdl summary. Every number below comes from a decoded model.
fn write_gmdl(
    out: &mut dyn Write,
    raw: &[u8],
    model: &spore_gmdl::GmdlModel,
) -> Result<(), ToolError> {
    writeln!(
        out,
        "decoder: gmdl (decoded \u{2014} every number below came out of this record's {} bytes)",
        raw.len()
    )?;
    writeln!(out, "  version            {}", model.version)?;
    writeln!(out, "  referenced files   {}", model.referenced_files.len())?;
    for (index, key) in model.referenced_files.iter().enumerate() {
        writeln!(out, "    [{index}] {key}")?;
    }
    writeln!(out, "  mesh count         {}", model.mesh_count)?;
    writeln!(
        out,
        "  bounds             {}",
        bounds(&model.bounds_min, &model.bounds_max)
    )?;
    writeln!(out, "  radius             {:.4}", model.radius)?;

    writeln!(out, "  index buffers      {}", model.index_buffers.len())?;
    for (index, buffer) in model.index_buffers.iter().enumerate() {
        let topology = spore_gmdl::Topology::from_prim_code(buffer.prim_type)
            .map_or_else(|| "?".to_owned(), |t| t.as_str().to_owned());
        writeln!(
            out,
            "    [{index}] prim={} ({topology}) indices={} bits={} payload={} bytes",
            buffer.prim_type,
            buffer.index_count,
            buffer.index_bits,
            buffer.bytes.len()
        )?;
    }

    writeln!(out, "  vertex descriptors {}", model.descriptors.len())?;
    for (index, descriptor) in model.descriptors.iter().enumerate() {
        writeln!(
            out,
            "    [{index}] {} element(s), stride {}",
            descriptor.len(),
            match spore_gmdl::vertex_stride(descriptor) {
                Ok(stride) => stride.to_string(),
                Err(error) => format!("<invalid: {error}>"),
            }
        )?;
        for (element_index, element) in descriptor.iter().enumerate() {
            writeln!(
                out,
                "      [{element_index}] {}",
                vertex_element_text(element)?
            )?;
        }
    }

    writeln!(out, "  vertex buffers     {}", model.vertex_buffers.len())?;
    for (index, buffer) in model.vertex_buffers.iter().enumerate() {
        writeln!(
            out,
            "    [{index}] descriptor={} vertices={} payload={} bytes",
            buffer.desc_index,
            buffer.vertex_count,
            buffer.bytes.len()
        )?;
    }

    writeln!(out, "  meshes             {}", model.meshes.len())?;
    for (index, mesh) in model.meshes.iter().enumerate() {
        writeln!(
            out,
            "    [{index}] index_buffer={} vertex_buffer={}",
            mesh.index_buffer, mesh.vertex_buffer
        )?;
    }

    writeln!(out, "  material ids       {}", model.material_ids.len())?;
    for (index, id) in model.material_ids.iter().enumerate() {
        writeln!(out, "    [{index}] 0x{id:08x}")?;
    }

    writeln!(out, "  texture refs       {}", model.texture_refs.len())?;
    for (index, reference) in model.texture_refs.iter().enumerate() {
        // A gmdl texture reference carries only `{instance, group}`; the record
        // type comes from context, so this states the known pair rather than
        // inventing a third component.
        writeln!(
            out,
            "    [{index}] instance=0x{:08x} group=0x{:08x} (type not carried by the reference)",
            reference.instance_id, reference.group_id
        )?;
    }

    writeln!(out, "  bone ranges        {}", model.bone_ranges.len())?;
    for (index, (start, count)) in model.bone_ranges.iter().enumerate() {
        writeln!(out, "    [{index}] start={start} count={count}")?;
    }

    write_trailer(out, model, raw.len())?;

    writeln!(out, "  per mesh")?;
    for index in 0..model.mesh_count {
        write_mesh(out, model, index)?;
    }
    Ok(())
}

/// The strict/best-effort boundary, reported whenever the two disagree.
///
/// `strict_consumed` is how far the validated walk reached; `consumed` is always
/// the record length. When they differ, the bytes in between were **not**
/// validated — and the parse still succeeded, because the trailer is
/// best-effort by design. Printing only `consumed` would claim every byte was
/// read when only a prefix was.
fn write_trailer(
    out: &mut dyn Write,
    model: &spore_gmdl::GmdlModel,
    record_len: usize,
) -> Result<(), ToolError> {
    match model.trailer {
        spore_gmdl::TrailerWalk::Complete => writeln!(
            out,
            "  trailer            complete ({} of {} bytes validated)",
            model.strict_consumed, record_len
        )?,
        spore_gmdl::TrailerWalk::Truncated { at } => {
            let stage = trailer_stage_name(at);
            writeln!(out, "  trailer            TRUNCATED at {stage}")?;
            writeln!(
                out,
                "  strict consumed    {} of {} bytes",
                model.strict_consumed, record_len
            )?;
            if record_len > model.strict_consumed {
                writeln!(
                    out,
                    "  unvalidated tail   {} bytes after the last successful read \u{2014} not parsed, not claimed",
                    record_len - model.strict_consumed
                )?;
            } else {
                // `strict_consumed` is the reader's *position*, so a read that
                // failed on its last four bytes leaves it already at the end.
                // Printing "0 bytes unvalidated" here would claim the walk
                // validated a word it never read, so the shortfall is described
                // instead of measured.
                writeln!(
                    out,
                    "  unvalidated        the shortfall is inside the read that failed at `{stage}`;"
                )?;
                writeln!(
                    out,
                    "                     `strict_consumed` is the reader position, so it cannot show it"
                )?;
            }
        }
    }
    writeln!(
        out,
        "  consumed           {} of {} bytes",
        model.consumed, record_len
    )?;
    Ok(())
}

/// One mesh of the model: what it draws, from the shared buffers it references.
fn write_mesh(
    out: &mut dyn Write,
    model: &spore_gmdl::GmdlModel,
    index: u32,
) -> Result<(), ToolError> {
    let stride = model
        .vertex_stride_of(index as usize)
        .map(|result| match result {
            Ok(stride) => stride.to_string(),
            Err(error) => format!("<invalid: {error}>"),
        });
    let reference = model.meshes.get(index as usize);
    match spore_gmdl::mesh_from_gmdl(model, index) {
        Ok(mesh) => {
            writeln!(
                out,
                "    [{index}] vertices={} indices={} stride={} topology={}",
                mesh.positions.len(),
                mesh.indices.len(),
                stride.as_deref().unwrap_or("?"),
                mesh.topology.as_str()
            )?;
            writeln!(
                out,
                "         normals={} uvs={}",
                mesh.normals.len(),
                mesh.uvs.len()
            )?;
            writeln!(
                out,
                "         bounds   {}",
                bounds(&mesh.bounds_min, &mesh.bounds_max)
            )?;
            writeln!(out, "         radius   {:.4}", mesh.radius)?;
        }
        Err(error) => {
            // The model decoded; this mesh could not be extracted. Two different
            // claims, so both are printed.
            writeln!(
                out,
                "    [{index}] mesh extraction FAILED: {error} (stride={}, buffers={})",
                stride.as_deref().unwrap_or("?"),
                reference
                    .map(|m| format!("index {} / vertex {}", m.index_buffer, m.vertex_buffer))
                    .unwrap_or_else(|| "no mesh row".to_owned())
            )?;
        }
    }
    Ok(())
}

/// One `D3DVERTEXELEMENT`-shaped record, with every field named.
fn vertex_element_text(element: &spore_gmdl::GmdlVertexElement) -> Result<String, ToolError> {
    let decl_type = match spore_gmdl::DeclType::from_code(element.decl_type) {
        Some(found) => format!("{} ({})", element.decl_type, variant_name(found)),
        None => format!("{} (UNDOCUMENTED)", element.decl_type),
    };
    let usage = match spore_gmdl::DeclUsage::from_code(element.decl_usage) {
        Some(found) => format!("{} ({})", element.decl_usage, usage_name(found)),
        None => format!("{} (UNDOCUMENTED)", element.decl_usage),
    };
    Ok(format!(
        "stream={} offset={} type={decl_type} method={} usage={usage} usage_index={} type_code=0x{:08x}",
        element.stream,
        element.offset,
        element.decl_method,
        element.usage_index,
        element.type_code
    ))
}

/// `DeclType` has no `Display`; these are the names the gmdl crate's own docs
/// use for each variant, so a line here and a line there agree.
fn variant_name(decl_type: spore_gmdl::DeclType) -> &'static str {
    use spore_gmdl::DeclType::*;
    match decl_type {
        Float1 => "FLOAT1",
        Float2 => "FLOAT2",
        Float3 => "FLOAT3",
        Float4 => "FLOAT4",
        D3dColor => "D3DCOLOR",
        UByte4 => "UBYTE4",
        Short2 => "SHORT2",
        Short4 => "SHORT4",
        UByte4N => "UBYTE4N",
        Short2N => "SHORT2N",
        Short4N => "SHORT4N",
        UShort2N => "USHORT2N",
        UShort4N => "USHORT4N",
        UDec3 => "UDEC3",
        Dec3N => "DEC3N",
        Float16x2 => "FLOAT16x2",
        Float16x4 => "FLOAT16x4",
    }
}

/// `DeclUsage` names, same reasoning as [`variant_name`].
fn usage_name(usage: spore_gmdl::DeclUsage) -> &'static str {
    use spore_gmdl::DeclUsage::*;
    match usage {
        Position => "POSITION",
        BlendWeight => "BLENDWEIGHT",
        BlendIndices => "BLENDINDICES",
        Normal => "NORMAL",
        PSize => "PSIZE",
        TexCoord => "TEXCOORD",
        Tangent => "TANGENT",
        Binormal => "BINORMAL",
        TessFactor => "TESSFACTOR",
        PositionT => "POSITIONT",
        Color => "COLOR",
        Fog => "FOG",
        Depth => "DEPTH",
        Sample => "SAMPLE",
    }
}

/// The trailer stage's name, for the "truncated at ..." line.
fn trailer_stage_name(stage: spore_gmdl::TrailerStage) -> &'static str {
    use spore_gmdl::TrailerStage::*;
    match stage {
        BoneRangeCount => "the bone-range count word",
        BoneRanges => "the bone-range array",
        AnimDataCount => "the anim-data count word",
        AnimData => "an anim-data record prefix",
        BakedDeform => "an anim-data baked-deform array",
        UnknownKey => "the trailing three-word key",
    }
}

/// A bounding box on one line, at the precision the record carries.
fn bounds(min: &[f32; 3], max: &[f32; 3]) -> String {
    format!(
        "min ({:.4}, {:.4}, {:.4}) max ({:.4}, {:.4}, {:.4})",
        min[0], min[1], min[2], max[0], max[1], max[2]
    )
}

/// The RW4 summary.
fn write_rw4(out: &mut dyn Write, directory: &spore_rw4::Rw4) -> Result<(), ToolError> {
    writeln!(
        out,
        "decoder: rw4 (decoded \u{2014} the section *directory* only; section payloads are not decoded)"
    )?;
    writeln!(out, "  describe()          {}", directory.describe())?;
    writeln!(
        out,
        "  file type           0x{:08x} ({})",
        directory.file_type,
        directory.file_type_enum()
    )?;
    writeln!(
        out,
        "  section count       {} declared, {} row(s) present",
        directory.section_count,
        directory.sections.len()
    )?;
    writeln!(
        out,
        "  buffer size         {} bytes (arena base 0x{:08x})",
        directory.buffer_size, directory.buffer_pointer
    )?;
    writeln!(out, "  object count        {}", directory.object_count)?;
    writeln!(out, "  is_complete         {}", directory.is_complete())?;
    writeln!(
        out,
        "  unknown type codes  {} of {} row(s), counted against the C++ known set",
        directory.unknown_type_codes(),
        directory.sections.len()
    )?;
    writeln!(out, "  type codes          {}", directory.type_codes.len())?;
    for (index, code) in directory.type_codes.iter().enumerate() {
        writeln!(out, "    [{index}] 0x{code:08x}{}", type_code_names(*code))?;
    }
    writeln!(out, "  sections")?;
    for (index, section) in directory.sections.iter().enumerate() {
        writeln!(
            out,
            "    [{index}] type=0x{:08x}{} size={} align={} data=0x{:08x}{} stored_pointer=0x{:08x}",
            section.type_code,
            type_code_names(section.type_code),
            section.size,
            section.align,
            section.data_address,
            section
                .end_address()
                .map(|end| format!(" end=0x{end:x}"))
                .unwrap_or_else(|| " end=?".to_owned()),
            section.data_pointer,
        )?;
    }
    Ok(())
}

/// A section type code with both reference tables' names, when they exist.
///
/// The two tables genuinely disagree — `0x40001` is `Mesh` in
/// `tools/spore/rw4/rw4.py` and absent from the C++ `inKnownSet`, and `Mesh`
/// itself is `0x20009` in C++ — so a single name here would pick a side without
/// saying so. `spore-rw4` documents the divergence; this line shows it at the
/// point where an operator would otherwise have to remember it.
fn type_code_names(code: u32) -> String {
    let cpp = spore_rw4::cpp_type_name(code);
    let python = spore_rw4::python_type_name(code);
    match (cpp, python) {
        (Some(name), Some(same)) if name == same => format!(" ({name})"),
        (Some(name), Some(other)) => {
            format!(" ({name} in C++, {other} in the Python oracle \u{2014} they disagree)")
        }
        (Some(name), None) => format!(" ({name})"),
        (None, Some(name)) => {
            format!(" (unknown to the C++ known set; {name} in the Python oracle)")
        }
        (None, None) => " (unknown to both reference tables)".to_owned(),
    }
}

/// The raster summary. Envelope, then the mip chain, then the decoded per-level
/// mip dimensions.
///
/// # The words here were wrong twice, and the corpus is why
///
/// * `field_18` used to be printed `UNRESOLVED (never read by any decoder)`. It
///   **is** read: it is the base level's own byte count in all three formats
///   (2954 of 2954 raster records in `Spore_Content.package`, 1432 of 1432 in
///   `Spore_EP1_Content_01.package`), and `decode_raster` refuses a record whose
///   header disagrees with its own geometry. Printing "never read" about a field
///   that gates every decode was the false half of a bigger error.
/// * `field_1c` used to be printed "format-specific per the oracle". In the
///   **28-byte** `0x15` header it is not a field at all: those bytes are the first
///   texel of the base level, which is why a real `0x15` record shows
///   `0x008080ff` there — the corpus's own flat normal `(255, 128, 128, 0)`.
///
/// The layer line is gone for the same reason: there is one chain, and calling
/// its count "derived" described a derivation the corpus refuted.
fn write_raster(out: &mut dyn Write, image: &spore_texture::RasterImage) -> Result<(), ToolError> {
    let envelope = &image.envelope;
    writeln!(
        out,
        "decoder: raster (decoded \u{2014} the record's single mip chain, every level)"
    )?;
    writeln!(out, "  envelope")?;
    writeln!(out, "    version         {}", envelope.version)?;
    writeln!(out, "    width           {}", envelope.width)?;
    writeln!(out, "    height          {}", envelope.height)?;
    writeln!(out, "    mip count       {}", envelope.mip_count)?;
    // `field_10` is still unresolved: observed `8` on every record, never read.
    writeln!(
        out,
        "    field_10        0x{:08x} UNRESOLVED (never read by any decoder; carried verbatim)",
        envelope.field_10
    )?;
    writeln!(
        out,
        "    fourcc          {}",
        super::describe_format(envelope.fourcc)
    )?;
    // Read, checked, and the reason this decode was allowed to proceed at all:
    // reaching this line means the header word and the geometry agreed.
    writeln!(
        out,
        "    field_18        0x{:08x} the base level's byte count \u{2014} re-read and checked against the\n                     geometry, and this record is only decoded because they agreed",
        envelope.field_18
    )?;
    writeln!(
        out,
        "    field_1c        0x{:08x} {}",
        envelope.field_1c,
        field_1c_text(envelope.fourcc)
    )?;
    writeln!(
        out,
        "  mip chains      {} (every raster record measured is one chain; a raster record has no\n                   layer count and none is derived \u{2014} the \"two layers\" reading was 8\n                   bytes charged per 4x4 block where DXT5 holds 16)",
        image.layers.len()
    )?;
    for (layer_index, layer) in image.layers.iter().enumerate() {
        writeln!(out, "    chain {layer_index} ({})", dimensions(layer))?;
    }
    Ok(())
}

/// What the word at `0x1c` is, for one format.
///
/// A total function over the three frames, because a word that means "the first
/// texel of the payload" in one format and "an unestablished metadata word" in
/// another must not be labelled the same way in both.
fn field_1c_text(fourcc: u32) -> &'static str {
    if fourcc == spore_texture::LUMINANCE_FOURCC {
        "is NOT a header field: the 0x15 header is 28 bytes, so these four bytes are the first\n                   texel of the base level (a flat normal reads 0x008080ff here)"
    } else {
        "carried verbatim; never used to select a decode and no meaning established"
    }
}

/// A mip chain as `512x512, 256x256, ...`, using the dimensions the *decoder*
/// produced rather than re-deriving them.
fn dimensions(mips: &[spore_texture::MipImage]) -> String {
    if mips.is_empty() {
        return "no mips".to_owned();
    }
    mips.iter()
        .map(|mip| format!("{}x{}", mip.width, mip.height))
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_undocumented_decl_type_is_named_rather_than_guessed() {
        let element = spore_gmdl::GmdlVertexElement {
            stream: 0,
            offset: 4,
            decl_type: 200,
            decl_method: 0,
            decl_usage: 0,
            usage_index: 0,
            type_code: 0,
        };
        let text = vertex_element_text(&element).unwrap();
        assert!(text.contains("type=200 (UNDOCUMENTED)"), "{text}");
        assert!(text.contains("usage=0 (POSITION)"), "{text}");
    }

    #[test]
    fn an_undocumented_decl_usage_is_named_rather_than_guessed() {
        let element = spore_gmdl::GmdlVertexElement {
            stream: 0,
            offset: 0,
            decl_type: 2,
            decl_method: 0,
            decl_usage: 250,
            usage_index: 0,
            type_code: 7,
        };
        let text = vertex_element_text(&element).unwrap();
        assert!(text.contains("usage=250 (UNDOCUMENTED)"), "{text}");
        assert!(text.contains("type=2 (FLOAT3)"), "{text}");
    }

    #[test]
    fn every_documented_decl_type_and_usage_has_a_name() {
        // A `match` arm added without its name helper would not compile; the
        // point of this test is that no arm is *wrong*, which the round trip
        // through `as_code` proves.
        for decl_type in spore_gmdl::DeclType::ALL {
            assert_eq!(
                spore_gmdl::DeclType::from_code(decl_type.as_code()),
                Some(decl_type)
            );
            assert!(!variant_name(decl_type).is_empty());
        }
        for usage in spore_gmdl::DeclUsage::ALL {
            assert_eq!(
                spore_gmdl::DeclUsage::from_code(usage.as_code()),
                Some(usage)
            );
            assert!(!usage_name(usage).is_empty());
        }
    }

    #[test]
    fn every_trailer_stage_has_a_readable_name() {
        use spore_gmdl::TrailerStage::*;
        for stage in [
            BoneRangeCount,
            BoneRanges,
            AnimDataCount,
            AnimData,
            BakedDeform,
            UnknownKey,
        ] {
            assert!(!trailer_stage_name(stage).is_empty(), "{stage:?}");
        }
    }

    #[test]
    fn an_empty_mip_chain_says_so_instead_of_printing_nothing() {
        assert_eq!(dimensions(&[]), "no mips");
    }
}
