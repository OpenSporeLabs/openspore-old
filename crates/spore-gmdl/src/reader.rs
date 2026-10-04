//! A bounds-checked little-/big-endian reader.
//!
//! Every read is a `Option`, never a panic: a decoder fed arbitrary bytes must
//! answer `Err`, and the only way to guarantee that for every prefix of every
//! input is to make each read fallible and to have no other way to reach a
//! byte. The reader therefore does not expose the backing slice to callers and
//! holds no `unsafe` escape hatch; `#![forbid(unsafe_code)]` at the crate root
//! is what makes that a compile-time property rather than a convention.

/// A cursor over a byte slice with fallible reads.
#[derive(Debug, Clone)]
pub(crate) struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    /// Starts a reader at offset 0.
    pub(crate) fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    /// Current offset from the start of the input.
    pub(crate) fn position(&self) -> usize {
        self.pos
    }

    /// Bytes left after the cursor. Saturating by construction: `pos` never
    /// exceeds `data.len()` because every advance is a successful `get`.
    pub(crate) fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.pos)
    }

    /// Consumes `len` bytes and returns them, or `None` (advancing nothing) if
    /// fewer than `len` bytes remain.
    pub(crate) fn take(&mut self, len: usize) -> Option<&'a [u8]> {
        let end = self.pos.checked_add(len)?;
        let slice = self.data.get(self.pos..end)?;
        self.pos = end;
        Some(slice)
    }

    /// Consumes `len` bytes, reporting whether they were there.
    pub(crate) fn skip(&mut self, len: usize) -> bool {
        self.take(len).is_some()
    }

    /// Reads one byte.
    pub(crate) fn read_u8(&mut self) -> Option<u8> {
        self.take(1)?.first().copied()
    }

    /// Reads a little-endian `u16`.
    pub(crate) fn read_u16(&mut self) -> Option<u16> {
        Some(u16::from_le_bytes(self.take(2)?.try_into().ok()?))
    }

    /// Reads a little-endian `u32` — the record's ordinary word order.
    pub(crate) fn read_u32(&mut self) -> Option<u32> {
        Some(u32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }

    /// Reads a **big-endian** `u32`.
    ///
    /// This exists for exactly 52 bytes in the whole format — the gmdl
    /// reference count and the three words of every key behind it — and is
    /// deliberately not the default. See the loud note at its call site in
    /// [`crate::parse`].
    pub(crate) fn read_u32_be(&mut self) -> Option<u32> {
        Some(u32::from_be_bytes(self.take(4)?.try_into().ok()?))
    }

    /// Reads a little-endian IEEE-754 `f32` from its 32 bits.
    pub(crate) fn read_f32(&mut self) -> Option<f32> {
        self.read_u32().map(f32::from_bits)
    }
}

#[cfg(test)]
mod tests {
    use super::Reader;

    #[test]
    fn reads_little_endian_by_default() {
        let mut r = Reader::new(&[0x08, 0x00, 0x00, 0x00]);
        assert_eq!(r.read_u32(), Some(8));
        assert_eq!(r.position(), 4);
        assert_eq!(r.remaining(), 0);
    }

    #[test]
    fn the_big_endian_read_differs_from_the_little_one() {
        let bytes = [0x00, 0x00, 0x00, 0x02];
        let mut be = Reader::new(&bytes);
        let mut le = Reader::new(&bytes);
        assert_eq!(be.read_u32_be(), Some(2));
        assert_eq!(le.read_u32(), Some(0x0200_0000));
    }

    #[test]
    fn a_short_read_advances_nothing() {
        let mut r = Reader::new(&[1, 2, 3]);
        assert_eq!(r.read_u32(), None);
        assert_eq!(r.position(), 0, "a failed read must not move the cursor");
        assert_eq!(r.read_u16(), Some(0x0201));
        assert_eq!(r.position(), 2);
        assert_eq!(r.read_u16(), None);
    }

    #[test]
    fn every_prefix_of_a_short_buffer_reports_absence_rather_than_panicking() {
        let data: Vec<u8> = (0u8..=7).collect();
        for len in 0..=data.len() {
            let mut r = Reader::new(data.get(..len).unwrap_or(&[]));
            // Read to exhaustion in every width; the contract under test is
            // that this loop always terminates and never panics.
            while r.read_u8().is_some() {}
            let mut r = Reader::new(data.get(..len).unwrap_or(&[]));
            while r.read_u32().is_some() {}
            let mut r = Reader::new(data.get(..len).unwrap_or(&[]));
            while r.read_u32_be().is_some() {}
            let mut r = Reader::new(data.get(..len).unwrap_or(&[]));
            while r.read_f32().is_some() {}
        }
    }

    #[test]
    fn take_does_not_overflow_the_cursor() {
        let mut r = Reader::new(&[0u8; 4]);
        assert_eq!(r.take(usize::MAX), None);
        assert_eq!(r.position(), 0);
        assert!(r.skip(4));
        assert!(!r.skip(1));
        assert_eq!(r.remaining(), 0);
    }

    #[test]
    fn f32_is_a_plain_bit_reinterpretation() {
        let bytes = 1.5f32.to_le_bytes();
        let mut r = Reader::new(&bytes);
        assert_eq!(r.read_f32(), Some(1.5));
    }
}
