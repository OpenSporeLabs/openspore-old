//! The oracle driver: one persistent Python process per package, spoken to in
//! newline-delimited JSON.
//!
//! # Why a session and not one process per record
//!
//! `tools/spore/dbpf/dbpf.py::read` slurps the whole package into memory --
//! 995 MB for `Spore_Content.package`. Spawning a process per record would
//! re-read that file 200 times for the QFS sample and hold two copies at once
//! if anything ran in parallel. So the harness starts **one** process per
//! package, asks it to load the package once, and then exchanges one JSON
//! request for one JSON response per query. `dbpf.read` is the only entry point
//! used; no oracle is reimplemented here.
//!
//! # Why one command is NOT an oracle at all
//! #
//! # The raster framing has **no** reference implementation left. `raster.py::decode`
//! # refuses any fourcc other than DXT5, and `raster.py::layer_count` and
//! # `dxt5.py` still derive the layer count by dividing a payload by an
//! # 8-byte-block stride -- the framing a byte count over 4386 of 4386 raster
//! # records **refuted** (see `docs/RASTER-FRAMING.md` section 2). So for
//! # `raster` records there is nothing on this side of the boundary to compare
//! # against, in either language. `cmd_rasterframe` below closes that gap, and it
//! # is deliberately NOT filed as an oracle:
//! #
//! # * it re-derives the framing identity from the **measured** layout
//! #   (`docs/RASTER-FRAMING.md` section 2) in a second language, with no call
//! #   into `raster.py` or `dxt5.py` and no shared constant, so a mistake in one
//! #   language cannot hide in both and importing the refuted rule cannot make the
//! #   check vacuous;
//! # * it is checked against the crate's own answer and a disagreement is REPORTED,
//! #   never reconciled -- the same discipline `mirror_index` follows;
//! # * the crate under test and this function share a *document*, not code, so
//! #   their agreement is a statement about the arithmetic of two independent
//! #   transcriptions. The framing itself is settled by the corpus: this walk
//! #   consumes **4386 of 4386** records in the two content packages exactly.
//! #   That is what "independent agreement is the point" means here -- the
//! #   agreement is evidence about transcription, and the corpus is the evidence
//! #   about the format.
//! #
//! # What it does NOT check, and cannot: the **codec inside a block**. It compares
//! # geometry, offsets, lengths and digests of the *stored* bytes; whether those
//! # bytes are BC3 or BC1 is `spore-texture`'s `INFERRED` claim, and this command
//! # has no opinion on it.
//! #
//! # Why the driver is allowed to look at bytes `dbpf.read` does not return
//!
//! Two measurements need words `dbpf.read` discards:
//!
//! * the **raw** size word, before `& 0x7FFFFFFF`, so the report can state how
//!   many rows actually carry the top bit rather than asserting the mask is
//!   harmless;
//! * the **raw** compression word, which `dbpf.read` collapses to a bool.
//!
//! The driver's `mirror_index` re-walks the raw index for exactly this. It is
//! an instrument, not a second oracle: every value it derives is checked
//! against `dbpf.read`'s own output before any of it is used, and a mismatch is
//! reported as a failure rather than believed. That way a bug in the mirror
//! cannot masquerade as a bug in the Rust parser.
//!
//! Nothing else is reimplemented. Every other decode is a call into the oracle
//! module. The single exception is the raster framing above, and the reason is
//! stated there: the raster oracles implement a framing the corpus refuted, so
//! calling them would have produced agreement about a rule that is wrong.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use crate::json::{self, Json};
use crate::report::{Agreement, RecordIdentity};

/// The driver, as Python source. Passed to `python3 -c`, with the `tools/spore`
/// directory as `argv[1]`.
pub const DRIVER: &str = r##"# spore-differential oracle driver.
#
# Speaks newline-delimited JSON on stdin/stdout. Every request is a JSON object
# with a "cmd" key; every response is a JSON object with an "ok" key. A response
# with ok=false carries "err" (and "stage" when the failure was the decode
# rather than the extraction) so the harness can quote both sides.
#
# stdout is the protocol channel and nothing else is allowed to write to it.
# dbpf.read() announces itself on stderr, which is why that is safe.
import base64
import hashlib
import json
import os
import struct
import sys
import tempfile
import traceback

TOOLS = os.path.abspath(sys.argv[1])
for _sub in ("dbpf", "rw4", "gmdl", "raster", "dxt5", "manifest", "worldobj"):
    _p = os.path.join(TOOLS, _sub)
    if os.path.isdir(_p):
        sys.path.insert(0, _p)

import dbpf          # noqa: E402
import rw4           # noqa: E402
import gmdl          # noqa: E402
import raster        # noqa: E402
import manifest      # noqa: E402
import worldobj      # noqa: E402

SIZE_MASK = 0x7FFFFFFF

# (path, whole-image bytes, dbpf.read() rows, mirrored rows)
STATE = None


def mirror_index(data, idx_off, idx_count):
    """Re-walk the raw index, keeping the words dbpf.read() masks away.

    Mirrors dbpf.read()'s offset walk exactly. Callers assert every derived
    value against dbpf.read() before using it, so a drift here is reported
    rather than trusted.
    """
    o = idx_off
    flags = struct.unpack_from('<I', data, o)[0]
    o += 4
    shared_type = -1
    shared_group = -1
    if flags & 1:
        shared_type = struct.unpack_from('<I', data, o)[0]
        o += 4
    if flags & 2:
        shared_group = struct.unpack_from('<I', data, o)[0]
        o += 4
    if flags & 4:
        o += 4
    rows = []
    for _ in range(idx_count):
        t = shared_type
        g = shared_group
        if t == -1:
            t = struct.unpack_from('<I', data, o)[0]
            o += 4
        if g == -1:
            g = struct.unpack_from('<I', data, o)[0]
            o += 4
        inst = struct.unpack_from('<I', data, o)[0]
        o += 4
        off = struct.unpack_from('<I', data, o)[0]
        o += 4
        csize_raw = struct.unpack_from('<I', data, o)[0]
        o += 4
        msize = struct.unpack_from('<I', data, o)[0]
        o += 4
        comp_raw = struct.unpack_from('<H', data, o)[0]
        o += 2
        saved_raw = data[o]
        o += 2
        rows.append({
            't': t, 'g': g, 'inst': inst, 'off': off,
            'csize': csize_raw & SIZE_MASK, 'csize_raw': csize_raw,
            'msize': msize, 'comp_raw': comp_raw, 'saved_raw': saved_raw,
        })
    return rows


def require_state():
    if STATE is None:
        raise RuntimeError("no package is open; send {\"cmd\":\"open\"} first")
    return STATE


def extract(i):
    """dbpf.read()'s rows plus dbpf.getdata(), one record at a time."""
    path, data, items, rows = require_state()
    blob = dbpf.getdata(data, items[i])
    return items[i], blob


def cmd_open(req):
    global STATE
    path = req['package']
    data, items = dbpf.read(path)
    header = data[:0x100]
    idx_off = struct.unpack_from('<I', header, 0x40)[0]
    idx_count = struct.unpack_from('<I', header, 0x24)[0]
    idx_flags = struct.unpack_from('<I', data, idx_off)[0]
    rows = mirror_index(data, idx_off, idx_count)

    if len(rows) != len(items):
        return {'ok': False,
                'err': 'mirror_index walked %d rows, dbpf.read returned %d'
                       % (len(rows), len(items))}
    for n, (row, item) in enumerate(zip(rows, items)):
        agrees = (row['t'] == item['type']
                  and row['g'] == item['group']
                  and row['inst'] == item['inst']
                  and row['off'] == item['off']
                  and row['csize'] == item['csize']
                  and row['msize'] == item['msize']
                  and (row['comp_raw'] == 0xFFFF) == item['comp'])
        if not agrees:
            return {'ok': False,
                    'err': 'mirror_index disagrees with dbpf.read at row %d: %r vs %r'
                           % (n, row, item)}

    STATE = (path, data, items, rows)
    masked = sum(1 for r in rows if r['csize_raw'] != r['csize'])
    return {
        'ok': True,
        'path': path,
        'count': len(items),
        'index_flags': idx_flags,
        'masked_csize_rows': masked,
        'max_raw_csize': max((r['csize_raw'] for r in rows), default=0),
        'compression_words': sorted({r['comp_raw'] for r in rows}),
        'saved_raw_values': sorted({r['saved_raw'] for r in rows}),
    }


def cmd_index(req):
    _path, _data, _items, rows = require_state()
    return {'ok': True, 'rows': [list(r.values()) for r in rows]}


def cmd_bytes(req):
    item, blob = extract(req['i'])
    return {'ok': True, 'len': len(blob),
            'b64': base64.b64encode(blob).decode('ascii'),
            'compressed': bool(item['comp'])}


def cmd_rw4(req):
    item, blob = extract(req['i'])
    try:
        line = rw4.describe(blob)
    except Exception as exc:  # noqa: BLE001 - the message IS the finding
        return {'ok': False, 'stage': 'describe', 'len': len(blob),
                'err': '%s: %s' % (type(exc).__name__, exc)}
    return {'ok': True, 'describe': line, 'len': len(blob)}


def gmdl_from_bytes(blob):
    """gmdl.Gmdl takes a path; hand it a temp file and take it away again."""
    fd, tmp = tempfile.mkstemp(suffix='.gmdl')
    try:
        os.write(fd, blob)
        os.close(fd)
        return gmdl.Gmdl(tmp)
    finally:
        try:
            os.unlink(tmp)
        except OSError:
            pass


def cmd_gmdl(req):
    _item, blob = extract(req['i'])
    try:
        g = gmdl_from_bytes(blob)
    except Exception as exc:  # noqa: BLE001
        return {'ok': False, 'stage': 'parse', 'len': len(blob),
                'err': '%s: %s' % (type(exc).__name__, exc)}
    return {
        'ok': True,
        'len': len(blob),
        'version': g.version,
        'refs': [list(r) for r in g.refs],
        'mesh_count': g.meshCount,
        'bbox_min': list(g.bboxMin),
        'bbox_max': list(g.bboxMax),
        'radius': g.radius,
        'index_buffers': [
            {'prim': ib['prim'], 'count': ib['count'], 'bits': ib['bits'],
             'size': ib['size'], 'bytes': len(ib['data'])}
            for ib in g.indexBuffers
        ],
        'descriptors': [
            [{'stream': e['stream'], 'offset': e['off'], 'type': e['type'],
              'method': e['method'], 'usage': e['usage'],
              'usage_index': e['usageIndex'], 'type_code': e['typeCode']}
             for e in desc]
            for desc in g.vertexDescriptors
        ],
        'vertex_buffers': [
            {'desc_idx': vb['descIdx'], 'vertex_count': vb['vertexCount'],
             'size': vb['size'], 'bytes': len(vb['data'])}
            for vb in g.vertexBuffers
        ],
        'meshes': [list(m) for m in g.meshes],
        'material_ids': list(g.materialIDs),
        'unk': g.unk,
        'material_info_groups': len(g.materialInfos),
        # One entry per material-info block: how many texture-set REFERENCES
        # that block contributed. Counts references, not texture-set entries: a
        # block may hold more than one 0x20D entry, and an entry may hold more
        # than one reference, so counting entries instead makes a block look
        # like it contributed one reference when it contributed several.
        'block_tex_counts': [sum(len(e[3]) for e in mi if e[0] == 'texset')
                             for mi in g.materialInfos],
        # And whether two blocks of the same record name the same texture set,
        # which is what decides whether the block ORDER carries any information
        # at all for this record.
        'block_tex_sets': [[[t[2], t[3]] for e in mi if e[0] == 'texset' for t in e[3]]
                           for mi in g.materialInfos],
        # Flattened exactly the way spore-gmdl flattens it: every 0x20D texture
        # set in every material group, in order of appearance, as (inst, grp).
        # The tree itself is NOT projected -- see the Rust-side note about the
        # two sides exposing different shapes for the same bytes.
        'texture_refs': [[t[2], t[3]] for mi in g.materialInfos
                         for e in mi if e[0] == 'texset' for t in e[3]],
        'bone_ranges': [list(r) for r in g.boneRanges],
        'unknown_key': list(g.unknownKey),
        'final_offset': g.finalOffset,
        'file_len': len(g.b),
        'matches': bool(g.matches()),
    }


def cmd_raster(req):
    _item, blob = extract(req['i'])
    out = {'ok': True, 'len': len(blob)}
    try:
        env = raster.parse_envelope(blob)
    except Exception as exc:  # noqa: BLE001
        out['ok'] = False
        out['stage'] = 'envelope'
        out['err'] = '%s: %s' % (type(exc).__name__, exc)
        return out
    out['env'] = env
    # The oracle's own arithmetic, reported so a disagreement can quote it rather
    # than paraphrase it: `payload` bytes after the envelope, and the per-layer
    # stride it divides them by. Both come from raster.py's OWN constants.
    out['payload'] = len(blob) - raster.ENV_SIZE
    out['stride'] = raster.LAYER_HDR + raster.chain_size(env)
    try:
        out['layers'] = raster.layer_count(blob, env)
    except Exception as exc:  # noqa: BLE001
        out['layers_error'] = '%s: %s' % (type(exc).__name__, exc)
        return out
    try:
        _env, mip0, _mips = raster.decode(blob)
    except Exception as exc:  # noqa: BLE001
        out['decode_error'] = '%s: %s' % (type(exc).__name__, exc)
        return out
    out['mip0_sha16'] = hashlib.sha256(mip0).hexdigest()[:16]
    out['mip0_len'] = len(mip0)
    return out


LUMINANCE_FOURCC = 0x00000015
DXT5_FOURCC = 0x35545844
DXT1_FOURCC = 0x31545844
ENV_BYTES = 32
MIP_FLOOR = 32
LEVEL_PREFIX = 4
TEXEL_BYTES = 4

# fourcc -> (header bytes, bytes per 4x4 block, first prefixed level).
#
# A None in the second slot means the format has no blocks at all: it is four
# bytes per TEXEL. The three rows are the measured per-format table in
# docs/RASTER-FRAMING.md section 2, transcribed here independently of
# spore_texture::framing. `first_prefixed_level` differs between the block
# formats and the uncompressed family, and WHY is UNKNOWN; this file states the
# measurement, not a cause.
FRAMES = {
    DXT5_FOURCC: (32, 16, 2),
    DXT1_FOURCC: (32, 8, 2),
    LUMINANCE_FOURCC: (28, None, 1),
}


def frame_for(fourcc):
    """The measured framing of `fourcc`, or None: no rule, not a verdict."""
    return FRAMES.get(fourcc)


def _frame_dim(base, mip):
    """max(1, base >> mip), with every mip at or beyond 32 clamped to 1."""
    if mip >= MIP_FLOOR:
        return 1
    return max(1, base >> mip)


def _frame_payload(frame, width, height, mip):
    """Payload bytes of one level under `frame`."""
    block = frame[1]
    w = _frame_dim(width, mip)
    h = _frame_dim(height, mip)
    if block is None:
        return w * h * TEXEL_BYTES
    return ((w + 3) // 4) * ((h + 3) // 4) * block


def frame_identity(frame, width, height, mip_count):
    """The one record size this framing allows.

    header, then the base level bare, then for every level at or beyond the
    frame's `first_prefixed_level` a little-endian u32 byte count and that
    level's payload. Levels at or beyond MIP_FLOOR are 1x1 and are charged a
    prefix plus one texel/block each, which is what makes the identity close for
    a mip count the corpus never contains.
    """
    header, block, first_prefixed = frame
    del block
    total = header
    bounded = min(mip_count, MIP_FLOOR)
    for mip in range(bounded):
        if mip >= first_prefixed:
            total += LEVEL_PREFIX
        total += _frame_payload(frame, width, height, mip)
    total += max(0, mip_count - bounded) * (
        LEVEL_PREFIX + _frame_payload(frame, width, height, MIP_FLOOR))
    return total


def cmd_rasterframe(req):
    """An independent framing walk over ANY raster record. See the module docs.

    Returns the derived identity, the byte window and digest of every level, and
    whether the walk consumed the record exactly. Refusals are reported as
    refusals with a reason, never as an empty success. No codec is applied: the
    digest is of the bytes as stored, because the framing is what this checks.
    """
    _item, blob = extract(req['i'])
    out = {'ok': True, 'len': len(blob)}
    if len(blob) < ENV_BYTES:
        out['ok'] = False
        out['err'] = 'shorter than the %d-byte envelope' % ENV_BYTES
        return out
    # 0x04 width, 0x08 height, 0x0c mipCount, 0x14 fourcc. The four words at
    # 0x04..0x14 are NOT consecutive with the format word: 0x10 is field_10, the
    # unresolved word that reads 8 in every record. Unpacking four consecutive
    # words from 0x04 reads field_10 as the format, which is exactly the kind of
    # off-by-one-word this function exists to catch.
    width, height, mip_count = struct.unpack_from('<3I', blob, 4)
    fourcc = struct.unpack_from('<I', blob, 0x14)[0]
    field_10 = struct.unpack_from('<I', blob, 0x10)[0]
    field_18 = struct.unpack_from('<I', blob, 0x18)[0]
    out.update({'width': width, 'height': height, 'mipCount': mip_count,
                'fourcc': fourcc, 'field_10': field_10, 'field_18': field_18})
    frame = frame_for(fourcc)
    if frame is None:
        out['ok'] = False
        out['err'] = 'fourcc 0x%08X has no measured framing rule' % fourcc
        return out
    header, block_bytes, first_prefixed = frame
    out['header'] = header
    out['block_bytes'] = block_bytes
    out['first_prefixed_level'] = first_prefixed
    if mip_count == 0:
        out['ok'] = False
        out['err'] = 'mipCount 0'
        return out
    if width == 0 or height == 0:
        out['ok'] = False
        out['err'] = 'zero dimension %dx%d' % (width, height)
        return out
    identity = frame_identity(frame, width, height, mip_count)
    out['identity'] = identity
    out['identity_holds'] = (identity == len(blob))
    if not out['identity_holds']:
        out['ok'] = False
        out['err'] = 'identity says %d bytes, record is %d' % (identity, len(blob))
        return out
    # field_18 is the base level's own byte count in all three formats. Reported
    # as a boolean, not asserted here: the harness compares it against the crate
    # so a disagreement between the two readings is visible as one.
    base_geom = _frame_payload(frame, width, height, 0)
    out['base_geom'] = base_geom
    out['field_18_is_base_geom'] = (field_18 == base_geom)
    mips = []
    at = header
    bad = None
    for mip in range(mip_count):
        declared = None
        if mip >= first_prefixed:
            declared = struct.unpack_from('<I', blob, at)[0]
            at += LEVEL_PREFIX
        geom = _frame_payload(frame, width, height, mip)
        window = blob[at:at + geom]
        if len(window) != geom:
            bad = 'level %d window runs past the record' % mip
            break
        if declared is not None and declared != geom:
            bad = 'level %d declares %d bytes, geometry says %d' % (mip, declared, geom)
            break
        mips.append({
            'mip': mip,
            'offset': at,
            'declared': declared,
            'geom': geom,
            'width': _frame_dim(width, mip),
            'height': _frame_dim(height, mip),
            'sha16': hashlib.sha256(window).hexdigest()[:16],
        })
        at += geom
    out['mips'] = mips
    out['consumed'] = at
    out['prefix_mismatch'] = bad
    if bad is not None:
        out['ok'] = False
        out['err'] = bad
        return out
    out['first_payload_word'] = list(blob[header:header + 4])
    out['base_sha16'] = mips[0]['sha16']
    return out


def cmd_manifest(req):
    """Run the reference manifest's own row builder over this package.

    manifest.py::build() writes a SQLite sidecar; this calls the module's _row()
    and the same sort, so the row set is exactly the one build() would produce
    without creating a database. The gmdl probe is handed the same bytes and the
    same (off, csize, msize) manifest.py itself uses -- i.e. the STORED, still
    compressed bytes. That is manifest.py's behaviour and it is not corrected
    here: if it is wrong, the disagreement is the finding.
    """
    path, data, items, _rows = require_state()
    types = manifest._load(manifest._TYPE_F)
    groups = manifest._load(manifest._GROUP_F)
    built = [manifest._row(path, rec, types, groups, data) for rec in items]
    built.sort(key=lambda row: (row[0], row[1], row[2]))
    ordered = all(
        (built[n][0], built[n][1], built[n][2])
        <= (built[n + 1][0], built[n + 1][1], built[n + 1][2])
        for n in range(len(built) - 1)
    )
    def types_with(status):
        return sorted({row[0] for row in built if row[7] == status})
    return {
        'ok': True,
        'rows': len(built),
        'ordered': ordered,
        'decodable': types_with('ok'),
        'containers': types_with('container-undecoded'),
        'undecoded': types_with('undecoded'),
        'walk_fail': types_with('walk-fail'),
        'sizes': {str(row[0]): row[5] for row in built},
        'statuses': {str(row[0]): row[7] for row in built},
    }


def cmd_worldobj(req):
    _item, blob = extract(req['i'])
    try:
        head = worldobj.parse_header(blob)
    except Exception as exc:  # noqa: BLE001
        return {'ok': False, 'stage': 'parse', 'len': len(blob),
                'err': '%s: %s' % (type(exc).__name__, exc)}
    return {
        'ok': True,
        'len': len(blob),
        'header': {k: head[k] for k in ('magic', 'version', 'count_c',
                                        'count_d', 'count_e')},
        'size': head['size'],
        'body_len': head['body_len'],
    }


def cmd_rw4_file(req):
    """rw4.describe() over a standalone file, for the committed fixture."""
    with open(req['path'], 'rb') as handle:
        blob = handle.read()
    try:
        line = rw4.describe(blob)
    except Exception as exc:  # noqa: BLE001
        return {'ok': False, 'stage': 'describe', 'len': len(blob),
                'err': '%s: %s' % (type(exc).__name__, exc)}
    return {'ok': True, 'describe': line, 'len': len(blob)}


def cmd_gmdl_file(req):
    with open(req['path'], 'rb') as handle:
        blob = handle.read()
    try:
        g = gmdl_from_bytes(blob)
    except Exception as exc:  # noqa: BLE001
        return {'ok': False, 'stage': 'parse', 'len': len(blob),
                'err': '%s: %s' % (type(exc).__name__, exc)}
    return {
        'ok': True,
        'len': len(blob),
        'version': g.version,
        'refs': [list(r) for r in g.refs],
        'mesh_count': g.meshCount,
        'bbox_min': list(g.bboxMin),
        'bbox_max': list(g.bboxMax),
        'radius': g.radius,
        'index_buffers': [
            {'prim': ib['prim'], 'count': ib['count'], 'bits': ib['bits'],
             'size': ib['size'], 'bytes': len(ib['data'])}
            for ib in g.indexBuffers
        ],
        'descriptors': [
            [{'stream': e['stream'], 'offset': e['off'], 'type': e['type'],
              'method': e['method'], 'usage': e['usage'],
              'usage_index': e['usageIndex'], 'type_code': e['typeCode']}
             for e in desc]
            for desc in g.vertexDescriptors
        ],
        'vertex_buffers': [
            {'desc_idx': vb['descIdx'], 'vertex_count': vb['vertexCount'],
             'size': vb['size'], 'bytes': len(vb['data'])}
            for vb in g.vertexBuffers
        ],
        'meshes': [list(m) for m in g.meshes],
        'material_ids': list(g.materialIDs),
        'unk': g.unk,
        'material_info_groups': len(g.materialInfos),
        # Flattened exactly the way spore-gmdl flattens it: every 0x20D texture
        # set in every material group, in order of appearance, as (inst, grp).
        # The tree itself is NOT projected -- see the Rust-side note about the
        # two sides exposing different shapes for the same bytes.
        'texture_refs': [[t[2], t[3]] for mi in g.materialInfos
                         for e in mi if e[0] == 'texset' for t in e[3]],
        'bone_ranges': [list(r) for r in g.boneRanges],
        'unknown_key': list(g.unknownKey),
        'final_offset': g.finalOffset,
        'file_len': len(g.b),
        'matches': bool(g.matches()),
    }


HANDLERS = {
    'open': cmd_open,
    'index': cmd_index,
    'bytes': cmd_bytes,
    'rw4': cmd_rw4,
    'gmdl': cmd_gmdl,
    'raster': cmd_raster,
    'rasterframe': cmd_rasterframe,
    'manifest': cmd_manifest,
    'worldobj': cmd_worldobj,
    'rw4_file': cmd_rw4_file,
    'gmdl_file': cmd_gmdl_file,
}


def reply(payload):
    sys.stdout.write(json.dumps(payload))
    sys.stdout.write('\n')
    sys.stdout.flush()


def main():
    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        try:
            request = json.loads(line)
        except Exception as exc:  # noqa: BLE001
            reply({'ok': False, 'err': 'unparsable request: %s' % exc})
            continue
        handler = HANDLERS.get(request.get('cmd'))
        if handler is None:
            reply({'ok': False, 'err': 'unknown command %r' % request.get('cmd')})
            continue
        try:
            reply(handler(request))
        except Exception as exc:  # noqa: BLE001
            reply({'ok': False, 'stage': 'driver',
                   'err': '%s: %s' % (type(exc).__name__, exc),
                   'traceback': traceback.format_exc()})
    return 0


if __name__ == '__main__':
    sys.exit(main())
"##;

/// Where the oracles and the fixture paths come from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OracleConfig {
    /// Interpreter to run. `python3` unless overridden.
    pub python: String,
    /// The `tools/spore` directory the driver imports the oracles from.
    pub tools_dir: std::path::PathBuf,
}

impl Default for OracleConfig {
    fn default() -> Self {
        Self::detect()
    }
}

impl OracleConfig {
    /// Reads the environment, falling back to `python3` and the workspace root.
    #[must_use]
    pub fn detect() -> Self {
        let root = crate::corpus::workspace_root();
        Self {
            python: std::env::var("OPENSPORE_PYTHON").unwrap_or_else(|_| "python3".to_owned()),
            tools_dir: root.join("tools").join("spore"),
        }
    }

    /// `true` when the interpreter and the oracle modules are both reachable.
    ///
    /// Checked once per run so a machine without `python3` reports `Skipped`
    /// rather than a wall of spawn failures.
    #[must_use]
    pub fn is_available(&self) -> bool {
        if !self.tools_dir.is_dir() {
            return false;
        }
        Command::new(&self.python)
            .arg("-c")
            .arg("import sys; sys.exit(0 if sys.version_info >= (3, 8) else 1)")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|status| status.success())
    }

    /// `true` when the interpreter is missing or the oracles are not in place.
    #[must_use]
    pub fn unavailable_reason(&self) -> Option<String> {
        if !self.tools_dir.is_dir() {
            return Some(format!(
                "oracle modules not found at {} (set {} to the checkout root)",
                self.tools_dir.display(),
                crate::corpus::ROOT_ENV
            ));
        }
        let probe = Command::new(&self.python)
            .arg("-c")
            .arg("import sys; sys.exit(0)")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        match probe {
            Ok(status) if status.success() => None,
            Ok(status) => Some(format!(
                "{} exited with {status} on a trivial script",
                self.python
            )),
            Err(error) => Some(format!(
                "{} could not be started: {error} (set {} to override)",
                self.python, "OPENSPORE_PYTHON"
            )),
        }
    }
}

/// The oracle could not be run, or refused to answer.
///
/// `Unavailable` is its own variant because it is not a failure of the
/// comparison -- it means the comparison did not happen.
#[derive(Debug)]
pub enum OracleError {
    /// The interpreter or the oracle modules are not present.
    Unavailable(String),
    /// The driver process could not be started.
    Spawn {
        /// The interpreter that was attempted.
        python: String,
        /// The underlying error.
        source: std::io::Error,
    },
    /// The driver died, or said something that is not a response.
    Protocol(String),
    /// The driver answered, and the answer was a refusal.
    Refused {
        /// Which step failed, when the driver said: `getdata`, `describe`,
        /// `parse`, `open`, ...
        stage: String,
        /// The oracle's own message, verbatim.
        message: String,
    },
}

impl std::fmt::Display for OracleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unavailable(why) => write!(f, "oracle unavailable: {why}"),
            Self::Spawn { python, source } => {
                write!(f, "oracle could not be spawned as {python}: {source}")
            }
            Self::Protocol(why) => write!(f, "oracle protocol failure: {why}"),
            Self::Refused { stage, message } => {
                write!(f, "oracle refused at {stage}: {message}")
            }
        }
    }
}

impl std::error::Error for OracleError {}

impl OracleError {
    /// `true` when the oracle was not available at all, as opposed to having
    /// answered with a refusal.
    #[must_use]
    pub fn is_unavailable(&self) -> bool {
        matches!(self, Self::Unavailable(_))
    }

    /// The oracle's message, for quoting on the oracle side of a verdict.
    #[must_use]
    pub fn message(&self) -> String {
        match self {
            Self::Unavailable(why) => why.clone(),
            Self::Spawn { python, source } => format!("{python}: {source}"),
            Self::Protocol(why) => why.clone(),
            Self::Refused { message, .. } => message.clone(),
        }
    }
}

/// A live driver process with one package open.
#[derive(Debug)]
pub struct OracleSession {
    child: Child,
    stdin: Option<ChildStdin>,
    stdout: BufReader<ChildStdout>,
    config: OracleConfig,
    opened: Option<PathBuf>,
}

impl OracleSession {
    /// Starts a driver process.
    pub fn spawn(config: &OracleConfig) -> Result<Self, OracleError> {
        let mut child = Command::new(&config.python)
            .arg("-c")
            .arg(DRIVER)
            .arg(&config.tools_dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            // Inherited, not captured: dbpf.read() announces the package header
            // there, and swallowing it would hide the fact that a 995 MB file
            // was read.
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|source| OracleError::Spawn {
                python: config.python.clone(),
                source,
            })?;
        let stdin = child.stdin.take();
        let stdout = child.stdout.take();
        match (stdin, stdout) {
            (Some(stdin), Some(stdout)) => Ok(Self {
                child,
                stdin: Some(stdin),
                stdout: BufReader::new(stdout),
                config: config.clone(),
                opened: None,
            }),
            _ => Err(OracleError::Protocol(
                "driver pipes were not created".to_owned(),
            )),
        }
    }

    /// The package this session currently holds, if any.
    #[must_use]
    pub fn opened_package(&self) -> Option<&Path> {
        self.opened.as_deref()
    }

    /// The configuration this session was started with.
    #[must_use]
    pub fn config(&self) -> &OracleConfig {
        &self.config
    }

    /// Asks the driver to load `package`, replacing anything already open.
    ///
    /// Closing the previous package first is not an optimisation: the driver
    /// holds the whole image, and the harness never holds two.
    pub fn open(&mut self, package: &Path) -> Result<OraclePackageInfo, OracleError> {
        self.close();
        let request = json::object(vec![
            ("cmd", Json::Str("open".to_owned())),
            ("package", Json::Str(package.to_string_lossy().into_owned())),
        ]);
        let response = self.exchange(&request.to_string())?;
        self.opened = Some(package.to_path_buf());
        Ok(OraclePackageInfo {
            path: package.to_path_buf(),
            count: response.get("count").and_then(Json::as_u32).unwrap_or(0) as usize,
            index_flags: response
                .get("index_flags")
                .and_then(Json::as_u32)
                .unwrap_or(0),
            masked_csize_rows: response
                .get("masked_csize_rows")
                .and_then(Json::as_u32)
                .unwrap_or(0) as usize,
            max_raw_csize: response.get("max_raw_csize").and_then(Json::as_u32),
            compression_words: u32_list(response.get("compression_words")),
            saved_raw_values: u32_list(response.get("saved_raw_values")),
        })
    }

    /// Releases the package, if one is open.
    pub fn close(&mut self) {
        self.opened = None;
    }

    /// Sends one request and reads one response.
    ///
    /// `{"cmd":"..."}`-shaped callers use the `request_*` helpers; this is the
    /// single place the protocol is spoken.
    pub fn exchange(&mut self, request: &str) -> Result<Json, OracleError> {
        let Some(stdin) = self.stdin.as_mut() else {
            return Err(OracleError::Protocol("driver stdin is closed".to_owned()));
        };
        stdin
            .write_all(request.as_bytes())
            .and_then(|()| stdin.write_all(b"\n"))
            .and_then(|()| stdin.flush())
            .map_err(|source| {
                OracleError::Protocol(format!("writing the request failed: {source}"))
            })?;

        let mut line = String::new();
        let read = self.stdout.read_line(&mut line).map_err(|source| {
            OracleError::Protocol(format!("reading the response failed: {source}"))
        })?;
        if read == 0 {
            return Err(OracleError::Protocol(
                "the driver exited without answering".to_owned(),
            ));
        }
        let value = json::parse(line.trim())
            .map_err(|error| OracleError::Protocol(format!("unparsable response: {error}")))?;

        if value.get("ok").and_then(Json::as_bool) == Some(true) {
            return Ok(value);
        }
        let stage = value
            .get("stage")
            .and_then(Json::as_str)
            .unwrap_or("unspecified")
            .to_owned();
        let message = value
            .get("err")
            .and_then(Json::as_str)
            .unwrap_or("the driver reported ok=false with no message")
            .to_owned();
        let traceback = value.get("traceback").and_then(Json::as_str);
        let message = match traceback {
            Some(trace) => format!("{message}\n{trace}"),
            None => message,
        };
        Err(OracleError::Refused { stage, message })
    }

    /// Asks the driver to run one record command over index `index`.
    pub fn request_record(&mut self, command: &str, index: usize) -> Result<Json, OracleError> {
        let request = json::object(vec![
            ("cmd", Json::Str(command.to_owned())),
            ("i", Json::Num(index as f64)),
        ]);
        self.exchange(&request.to_string())
    }

    /// Asks the driver to run one whole-file command over `path`.
    pub fn request_file(&mut self, command: &str, path: &Path) -> Result<Json, OracleError> {
        let request = json::object(vec![
            ("cmd", Json::Str(command.to_owned())),
            ("path", Json::Str(path.to_string_lossy().into_owned())),
        ]);
        self.exchange(&request.to_string())
    }

    /// Asks the driver for its independent framing walk over the raster record
    /// at index `index`, for any of the three measured `fourcc` values.
    ///
    /// Not an oracle -- see the module documentation, "Why one command is NOT
    /// an oracle at all". It is a second transcription of the *measured*
    /// framing in another language, sharing no constant with `spore-texture`,
    /// and a disagreement with the crate is a finding rather than something to
    /// reconcile. It says nothing about the block codec.
    pub fn request_raster_frame(&mut self, index: usize) -> Result<Json, OracleError> {
        self.request_record("rasterframe", index)
    }

    /// Asks for the whole-file manifest rows.
    pub fn request_manifest(&mut self) -> Result<Json, OracleError> {
        let request = json::object(vec![("cmd", Json::Str("manifest".to_owned()))]);
        self.exchange(&request.to_string())
    }
}

impl Drop for OracleSession {
    fn drop(&mut self) {
        // Closing stdin is the documented way to end the driver's read loop, so
        // the process exits on its own terms rather than on a signal.
        drop(self.stdin.take());
        let _ = self.child.wait();
    }
}

/// What the driver measured when it opened a package.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OraclePackageInfo {
    /// The path that was opened.
    pub path: std::path::PathBuf,
    /// Row count as `dbpf.read` sees it.
    pub count: usize,
    /// The index flags word.
    pub index_flags: u32,
    /// Rows whose raw size word carried bit 31.
    pub masked_csize_rows: usize,
    /// Largest raw size word seen, unmasked.
    pub max_raw_csize: Option<u32>,
    /// The distinct raw compression words in this package.
    pub compression_words: Vec<u32>,
    /// The distinct raw `saved` bytes in this package.
    pub saved_raw_values: Vec<u32>,
}

impl OraclePackageInfo {
    /// `true` when `record_index` names a row that exists.
    #[must_use]
    pub fn has(&self, record_index: usize) -> bool {
        record_index < self.count
    }

    /// The identity of row `record_index`, for a report.
    #[must_use]
    pub fn identity(
        &self,
        record_index: usize,
        type_id: u32,
        group_id: u32,
        instance_id: u32,
    ) -> RecordIdentity {
        RecordIdentity::new(
            self.path.file_name().map_or_else(
                || self.path.display().to_string(),
                |n| n.to_string_lossy().into_owned(),
            ),
            record_index,
            type_id,
            group_id,
            instance_id,
        )
    }
}

/// Turns two decode attempts into the right verdict for one record.
///
/// This is the rule the whole crate rests on: two refusals are agreement,
/// exactly one refusal is a finding, and neither is a crash.
///
/// Both sides hand over a message rather than their own error type, because the
/// whole point is that the two messages are kept side by side and neither is
/// rewritten into the other's vocabulary.
#[must_use]
pub fn classify_refusal(oracle: Result<(), String>, rust: Result<(), String>) -> Agreement {
    match (oracle, rust) {
        (Ok(()), Ok(())) => Agreement::Equal,
        (Ok(()), Err(why)) => Agreement::OracleOnly(format!("rust refused: {why}")),
        (Err(why), Ok(())) => Agreement::RustOnly(format!("oracle refused: {why}")),
        (Err(oracle), Err(rust)) => Agreement::BothFailed { oracle, rust },
    }
}

fn u32_list(value: Option<&Json>) -> Vec<u32> {
    value
        .and_then(Json::as_array)
        .map(|items| items.iter().filter_map(Json::as_u32).collect())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_driver_is_valid_python_that_starts_with_a_docstring_comment() {
        // The single most damaging failure mode for this crate is a driver that
        // does not compile, discovered only when a test is `#[ignore]`d.
        assert!(DRIVER.starts_with("# spore-differential oracle driver."));
        assert!(DRIVER.contains("if __name__ == '__main__':"));
        // Raw-string hazard: the driver must not contain the sequence that
        // would terminate the Rust raw string it lives in.
        assert!(!DRIVER.contains("\"##"));
    }

    #[test]
    fn two_refusals_are_agreement_and_one_is_a_finding() {
        let both = classify_refusal(
            Err("ValueError: unsupported fourcc 0x00001500".to_owned()),
            Err("texture: unsupported fourcc 0x00001500".to_owned()),
        );
        assert!(both.is_clean());
        assert_eq!(both.label(), "both-failed");
        assert!(both.to_string().contains("unsupported fourcc"));

        let oracle_only = classify_refusal(Ok(()), Err("rust said no".to_owned()));
        assert_eq!(oracle_only.label(), "oracle-only");
        assert!(!oracle_only.is_clean());

        let rust_only = classify_refusal(Err("struct.error".to_owned()), Ok(()));
        assert_eq!(rust_only.label(), "rust-only");
        assert!(!rust_only.is_clean());

        let equal = classify_refusal(Ok(()), Ok(()));
        assert_eq!(equal.label(), "equal");
    }

    #[test]
    fn an_unavailable_oracle_is_distinguishable_from_a_refusal() {
        let error = OracleError::Unavailable("python3 missing".to_owned());
        assert!(error.is_unavailable());
        assert!(!OracleError::Refused {
            stage: "parse".to_owned(),
            message: "x".to_owned()
        }
        .is_unavailable());
    }

    #[test]
    fn a_package_info_answers_whether_a_row_exists_and_names_a_row() {
        let info = OraclePackageInfo {
            path: std::path::PathBuf::from("/tmp/Spore_Content.package"),
            count: 3,
            index_flags: 4,
            masked_csize_rows: 3,
            max_raw_csize: Some(0x8000_0001),
            compression_words: vec![0, 0xFFFF],
            saved_raw_values: vec![1],
        };
        assert!(info.has(2));
        assert!(!info.has(3));
        let id = info.identity(2, 1, 2, 3);
        assert_eq!(
            id.to_string(),
            "Spore_Content.package[2] t=0x00000001 g=0x00000002 i=0x00000003"
        );
    }
}
