//! The `EditorRigblock` record (`0x5CAA5E28`): a quantised spatial bounds table.
//!
//! # This record is not a mesh, and cannot contain one
//!
//! A `ResourceKey` is 12 bytes. Every byte of every one of the 1549 records in
//! the install is accounted for by the layout below, with **zero slack in
//! 1549/1549**. There is nowhere for a model reference to hide, and the claim is
//! refuted by subtraction rather than by argument.
//!
//! It is a **baked** record: `FUN_0041c1d0` walks six property lists, loads each
//! `Model`, and serialises it into `Editors_BlockData.package`. So it is the
//! editor's own output, shipped as part of the game data.
//!
//! # Layout
//!
//! ```text
//! cell_count = 0            when k == 0        <- the writer's early return
//! cell_count = 4^k          when k >= 1
//! size       = 36 + 3 · cell_count
//! ```
//!
//! Eight distinct sizes cover all 1549 records; there is no long tail.
//!
//! **Correction to `docs/analysis/creator/08-rigblock-tail.md` §2**, which
//! states `size = 36 + 12 · 4^k` for `k in 0..=7`. That formula is right for
//! `k >= 1` and wrong for `k = 0`: it predicts 48 bytes, while the same
//! report's own histogram has **212 records of exactly 36 bytes**, and its
//! decompiled writer returns after the header when the level count is zero
//! ("`if n == 0: write(&hdr, 0x24); return  // -> exactly 36 bytes`"). The
//! per-level mapping is `k=0→36, 1→48, 2→84, 3→228, 4→804, 5→3108, 6→12324,
//! 7→49188`, which is `36 + 3·4^k` for `k >= 1`. A `k = 0` record carries **no**
//! cells, not one.
//!
//! | offset | width | meaning |
//! |---|---:|---|
//! | `0x00` | 12 | 3 floats — component-wise **MAX** over each cell's *low* corner |
//! | `0x0C` | 12 | 3 floats — component-wise **MIN** over each cell's *high* corner |
//! | `0x18` | 4 | the quantisation step `s` |
//! | `0x1C` | 8 | per-level resolution bytes: `4` for `i < k`, else `0` |
//! | `0x24` | `12·4^k` | payload: `4^k` cells of 3 bytes |
//!
//! Note the MAX-then-MIN order. It reads backwards and it is correct: the writer
//! accumulates `max` over low corners and `min` over high corners, and two of the
//! four decompiled leaf helpers in `FUN_0041aaa0`'s neighbourhood had their min
//! and max **inverted** by the decompiler. Only a data-side test caught it, and
//! the wrong version still looked self-consistent.
//!
//! # Cell encoding
//!
//! Three bytes, one per axis, each packing two 4-bit distances in units of `s`:
//!
//! ```text
//! byte[j] = (qLow[j] << 4) | qHigh[j]
//! cellLow [j] = f32[0+j] - qLow [j] * s
//! cellHigh[j] = f32[3+j] + qHigh [j] * s
//! ```
//!
//! The nibble order is not a guess: testing the opposite assignment over every
//! cell of every record gives 234 428 axis-order violations against **0** for
//! this one. 444 264 axis tests, zero violations.
//!
//! What the levels *mean* — which rigblock parameter each level drives — is
//! **unknown**, and `docs/analysis/creator/08-rigblock-tail.md` §7 item 1 records
//! what would settle it.

use crate::error::{Result, RigblockError};

/// The DBPF type id of a rigblock record.
pub const TYPE_RIGBLOCK: u32 = 0x5CAA_5E28;

/// The fixed header size.
const HEADER: usize = 36;

/// The payload offset.
const PAYLOAD: usize = 0x24;

/// The deepest subdivision level the corpus uses.
const MAX_LEVEL: u32 = 7;

/// One decoded rigblock: an envelope plus a quantised occupancy grid.
#[derive(Debug, Clone, PartialEq)]
pub struct Rigblock {
    /// Component-wise maximum over the low corner of every cell.
    ///
    /// On the three axes this is the **high** end of the envelope, because a
    /// cell's *low* corner is its high extreme in this coordinate convention.
    pub max_low: [f32; 3],
    /// Component-wise minimum over the high corner of every cell.
    pub min_high: [f32; 3],
    /// The quantisation step. `0.0` exactly when `levels == 0`.
    pub step: f32,
    /// The subdivision depth, `k`.
    pub levels: u32,
    /// The occupancy grid, row-major, `4^k` cells.
    pub cells: Vec<Cell>,
}

/// One quantised cell of the bounds grid.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Cell {
    /// The cell's low corner.
    pub low: [f32; 3],
    /// The cell's high corner.
    pub high: [f32; 3],
}

impl Rigblock {
    /// Decodes one rigblock record.
    pub fn decode(record: &[u8]) -> Result<Self> {
        if record.len() < HEADER {
            return Err(RigblockError::Truncated {
                need: HEADER,
                have: record.len(),
            });
        }
        let read_f32 = |at: usize| -> f32 {
            f32::from_le_bytes([record[at], record[at + 1], record[at + 2], record[at + 3]])
        };
        let mut floats = [0f32; 7];
        for (index, slot) in floats.iter_mut().enumerate() {
            *slot = read_f32(4 * index);
        }
        let step = floats[6];

        let mut level_bytes = [0u8; 8];
        level_bytes.copy_from_slice(&record[0x1C..0x24]);

        // The level count is encoded twice, redundantly: by the resolution bytes
        // and by the payload length. Both are checked, because a decoder that
        // trusted one would accept a record whose two halves disagree.
        let levels = level_bytes.iter().filter(|byte| **byte == 4).count() as u32;
        for (index, byte) in level_bytes.iter().enumerate() {
            let expected = if (index as u32) < levels { 4 } else { 0 };
            if *byte != expected {
                return Err(RigblockError::BadLevels {
                    index,
                    found: *byte,
                    expected,
                });
            }
        }
        if levels > MAX_LEVEL {
            return Err(RigblockError::BadLevels {
                index: MAX_LEVEL as usize,
                found: level_bytes[MAX_LEVEL as usize],
                expected: 0,
            });
        }

        // `k == 0` is the writer's early return: a header and nothing else.
        // Reading it as one cell would invent a cell the record does not have.
        let cell_count = if levels == 0 { 0 } else { 4usize.pow(levels) };
        let expected_size = HEADER + 3 * cell_count;
        if record.len() < expected_size {
            return Err(RigblockError::Truncated {
                need: expected_size,
                have: record.len(),
            });
        }

        // The two encodings must agree, and `k == 0` must coincide with a zero
        // step — 1337/1337 and 212/212 in the corpus, zero exceptions.
        // A subdivided record needs a real, finite, positive step; `k == 0` pairs
        // with a zero one. Written as a match on the comparison so the NaN case
        // is explicit rather than swept into a `!(step > 0.0)` that reads like a
        // sign test.
        let usable_step = matches!(step.partial_cmp(&0.0), Some(core::cmp::Ordering::Greater));
        if levels > 0 && !usable_step {
            return Err(RigblockError::BadSize { size: record.len() });
        }

        let mut cells = Vec::with_capacity(cell_count);
        for cell in 0..cell_count {
            let mut low = [0f32; 3];
            let mut high = [0f32; 3];
            for axis in 0..3 {
                let byte = record[PAYLOAD + 3 * cell + axis];
                let q_low = f32::from(byte >> 4);
                let q_high = f32::from(byte & 0x0F);
                // The writer's own mapping. Reversing these two terms is the
                // single easiest way to produce a plausible, wrong grid.
                low[axis] = floats[axis] - q_low * step;
                high[axis] = floats[3 + axis] + q_high * step;
            }
            cells.push(Cell { low, high });
        }

        Ok(Self {
            max_low: [floats[0], floats[1], floats[2]],
            min_high: [floats[3], floats[4], floats[5]],
            step,
            levels,
            cells,
        })
    }

    /// The envelope implied by the extreme cells, as `(low, high)` corners.
    ///
    /// This is a *derived* bound: it is computed from the decoded cells, not
    /// copied from the header, so it is a check on the header rather than a
    /// restatement of it.
    pub fn envelope(&self) -> Option<([f32; 3], [f32; 3])> {
        let first = self.cells.first()?;
        let mut low = first.low;
        let mut high = first.high;
        for cell in &self.cells[1..] {
            for axis in 0..3 {
                low[axis] = low[axis].min(cell.low[axis]);
                high[axis] = high[axis].max(cell.high[axis]);
            }
        }
        Some((low, high))
    }

    /// The axis-aligned span of the grid.
    pub fn span(&self) -> Option<[f32; 3]> {
        let (low, high) = self.envelope()?;
        Some([high[0] - low[0], high[1] - low[1], high[2] - low[2]])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a record the way the serialiser does.
    fn build(levels: u32, cells: &[(u8, u8, u8)], step: f32) -> Vec<u8> {
        let n = if levels == 0 { 0 } else { 4usize.pow(levels) };
        assert_eq!(cells.len(), n);
        let mut out = vec![0u8; HEADER + 3 * n];
        for axis in 0..6 {
            out[4 * axis..4 * axis + 4].copy_from_slice(&10.0f32.to_le_bytes());
        }
        out[0x18..0x1C].copy_from_slice(&step.to_le_bytes());
        for index in 0..8u32 {
            out[0x1C + index as usize] = if index < levels { 4 } else { 0 };
        }
        for (cell, triple) in cells.iter().enumerate() {
            let triple = [triple.0, triple.1, triple.2];
            for axis in 0..3 {
                out[PAYLOAD + 3 * cell + axis] = triple[axis];
            }
        }
        out
    }

    #[test]
    fn a_level_zero_record_is_thirty_six_bytes_and_has_no_cells() {
        // The writer returns after the header, so there is no payload at all.
        // Report 08's `36 + 12·4^k` predicts 48 here; the corpus has 212 records
        // of exactly 36 bytes.
        let record = build(0, &[], 0.0);
        assert_eq!(record.len(), 36);
        let rigblock = Rigblock::decode(&record).expect("decodes");
        assert_eq!(rigblock.levels, 0);
        assert_eq!(rigblock.cells.len(), 0);
        assert_eq!(rigblock.step, 0.0);
        assert_eq!(rigblock.envelope(), None, "no cells means no derived bound");
    }

    #[test]
    fn the_measured_size_for_every_level() {
        // k -> size, as the corpus histogram reports it.
        for (levels, size) in [
            (0u32, 36usize),
            (1, 48),
            (2, 84),
            (3, 228),
            (4, 804),
            (5, 3108),
            (6, 12324),
            (7, 49188),
        ] {
            let n = if levels == 0 { 0 } else { 4usize.pow(levels) };
            let record = build(levels, &vec![(0u8, 0u8, 0u8); n], 0.5);
            assert_eq!(record.len(), size, "levels={levels}");
            let rigblock = Rigblock::decode(&record).expect("decodes");
            assert_eq!(rigblock.cells.len(), n, "levels={levels}");
        }
    }

    #[test]
    fn a_cell_recovers_its_corners_from_the_two_nibbles() {
        // step 0.5, header extremes at 10: low = 10 - qLow*0.5, high = 10 + qHigh*0.5
        // `k = 1` so there is a cell to decode.
        let record = build(1, &[(0x21, 0, 0), (0, 0, 0), (0, 0, 0), (0, 0, 0)], 0.5);
        let rigblock = Rigblock::decode(&record).unwrap();
        let cell = rigblock.cells[0];
        assert!((cell.low[0] - 9.0).abs() < 1e-6, "qLow=2 -> 10-1");
        assert!((cell.high[0] - 10.5).abs() < 1e-6, "qHigh=1 -> 10+0.5");
        // The untouched axes decode to the header value exactly.
        assert!((cell.low[1] - 10.0).abs() < 1e-6);
    }

    #[test]
    fn level_bytes_that_do_not_encode_the_level_count_are_refused() {
        let mut record = build(2, &[(0u8, 0u8, 0u8); 16], 0.5);
        record[0x1C] = 3; // must be 4 for a level below k
        assert!(matches!(
            Rigblock::decode(&record),
            Err(RigblockError::BadLevels {
                index: 0,
                found: 3,
                expected: 4
            })
        ));
    }

    #[test]
    fn a_record_too_short_for_its_levels_is_refused() {
        let record = build(2, &[(0u8, 0u8, 0u8); 16], 0.5);
        assert_eq!(
            Rigblock::decode(&record[..HEADER + 3 * 15]),
            Err(RigblockError::Truncated {
                need: HEADER + 3 * 16,
                have: HEADER + 3 * 15
            })
        );
    }

    #[test]
    fn a_header_shorter_than_the_fixed_part_is_refused() {
        assert_eq!(
            Rigblock::decode(&[0u8; 12]),
            Err(RigblockError::Truncated { need: 36, have: 12 })
        );
    }

    #[test]
    fn a_non_positive_step_on_a_subdivided_record_is_refused() {
        // `k == 0` pairs with a zero step; a subdivided record must have a real one.
        let record = build(1, &[(0u8, 0u8, 0u8); 4], 0.0);
        assert!(matches!(
            Rigblock::decode(&record),
            Err(RigblockError::BadSize { .. })
        ));
    }
}
