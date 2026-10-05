//! SHA-256 (FIPS 180-4), hand-rolled, and **private to this crate**.
//!
//! # Why this is here at all
//!
//! `spore-creator-doc/1` declares a `content_sha256` over its own content
//! lines, which is the same integrity contract the semantic passport format
//! uses. This crate is allowed exactly two dependencies, `spore-core` and
//! `thiserror`, and a digest is not worth a third plus a transitive tree — so
//! the algorithm is implemented here, in auditable code, rather than pulled in.
//!
//! # Why this is not a second answer
//!
//! `spore-semantic-bridge` carries the same FIPS 180-4 implementation. Neither
//! crate can depend on the other, and SHA-256 is a published standard with
//! published test vectors rather than a measurement about Spore: both copies are
//! pinned to the same vectors below, so "two implementations" is two
//! implementations of *the specification*, checked against the specification's
//! own numbers. A digest that was subtly wrong and self-consistently wrong would
//! still pass the unit tests here and fail every real artifact, so the vectors
//! include the million-`a` message, which no implementation gets right by
//! accident.
//!
//! The unit tests at the bottom are the whole reason this module is not a
//! one-liner over a dependency: a digest implementation is exactly the kind of
//! code that fails silently.

const K: [u32; 64] = [
    0x428a_2f98,
    0x7137_4491,
    0xb5c0_fbcf,
    0xe9b5_dba5,
    0x3956_c25b,
    0x59f1_11f1,
    0x923f_82a4,
    0xab1c_5ed5,
    0xd807_aa98,
    0x1283_5b01,
    0x2431_85be,
    0x550c_7dc3,
    0x72be_5d74,
    0x80de_b1fe,
    0x9bdc_06a7,
    0xc19b_f174,
    0xe49b_69c1,
    0xefbe_4786,
    0x0fc1_9dc6,
    0x240c_a1cc,
    0x2de9_2c6f,
    0x4a74_84aa,
    0x5cb0_a9dc,
    0x76f9_88da,
    0x983e_5152,
    0xa831_c66d,
    0xb003_27c8,
    0xbf59_7fc7,
    0xc6e0_0bf3,
    0xd5a7_9147,
    0x06ca_6351,
    0x1429_2967,
    0x27b7_0a85,
    0x2e1b_2138,
    0x4d2c_6dfc,
    0x5338_0d13,
    0x650a_7354,
    0x766a_0abb,
    0x81c2_c92e,
    0x9272_2c85,
    0xa2bf_e8a1,
    0xa81a_664b,
    0xc24b_8b70,
    0xc76c_51a3,
    0xd192_e819,
    0xd699_0624,
    0xf40e_3585,
    0x106a_a070,
    0x19a4_c116,
    0x1e37_6c08,
    0x2748_774c,
    0x34b0_bcb5,
    0x391c_0cb3,
    0x4ed8_aa4a,
    0x5b9c_ca4f,
    0x682e_6ff3,
    0x748f_82ee,
    0x78a5_636f,
    0x84c8_7814,
    0x8cc7_0208,
    0x90be_fffa,
    0xa450_6ceb,
    0xbef9_a3f7,
    0xc671_78f2,
];

/// An incremental SHA-256 hasher.
#[derive(Debug, Clone)]
pub(crate) struct Sha256 {
    state: [u32; 8],
    buffer: [u8; 64],
    buffered: usize,
    length: u64,
}

impl Sha256 {
    pub(crate) fn new() -> Self {
        Self {
            state: [
                0x6a09_e667,
                0xbb67_ae85,
                0x3c6e_f372,
                0xa54f_f53a,
                0x510e_527f,
                0x9b05_688c,
                0x1f83_d9ab,
                0x5be0_cd19,
            ],
            buffer: [0; 64],
            buffered: 0,
            length: 0,
        }
    }

    pub(crate) fn update(&mut self, mut data: &[u8]) {
        self.length = self.length.wrapping_add(data.len() as u64);
        if self.buffered > 0 {
            let want = 64 - self.buffered;
            let take = want.min(data.len());
            self.buffer[self.buffered..self.buffered + take].copy_from_slice(&data[..take]);
            self.buffered += take;
            data = &data[take..];
            if self.buffered == 64 {
                let block = self.buffer;
                self.compress(&block);
                self.buffered = 0;
            }
        }
        while data.len() >= 64 {
            let (block, rest) = data.split_at(64);
            let mut chunk = [0u8; 64];
            chunk.copy_from_slice(block);
            self.compress(&chunk);
            data = rest;
        }
        if !data.is_empty() {
            self.buffer[..data.len()].copy_from_slice(data);
            self.buffered = data.len();
        }
    }

    pub(crate) fn finish(mut self) -> [u8; 32] {
        let bits = self.length.wrapping_mul(8);
        self.update_no_count(&[0x80]);
        while self.buffered != 56 {
            self.update_no_count(&[0x00]);
        }
        self.update_no_count(&bits.to_be_bytes());
        let mut out = [0u8; 32];
        for (index, word) in self.state.iter().enumerate() {
            out[index * 4..index * 4 + 4].copy_from_slice(&word.to_be_bytes());
        }
        out
    }

    /// The lowercase hex digest, which is the spelling every committed artifact
    /// in this repository uses.
    pub(crate) fn hex(self) -> String {
        let mut out = String::with_capacity(64);
        for byte in self.finish() {
            out.push(char::from_digit(u32::from(byte >> 4), 16).expect("a nibble is a digit"));
            out.push(char::from_digit(u32::from(byte & 0x0f), 16).expect("a nibble is a digit"));
        }
        out
    }

    /// Feeds bytes without counting them, so padding cannot extend the length.
    fn update_no_count(&mut self, data: &[u8]) {
        for &byte in data {
            self.buffer[self.buffered] = byte;
            self.buffered += 1;
            if self.buffered == 64 {
                let block = self.buffer;
                self.compress(&block);
                self.buffered = 0;
            }
        }
    }

    fn compress(&mut self, block: &[u8; 64]) {
        let mut w = [0u32; 64];
        for (index, chunk) in block.chunks_exact(4).enumerate() {
            w[index] = u32::from_be_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
        }
        for index in 16..64 {
            let s0 = w[index - 15].rotate_right(7)
                ^ w[index - 15].rotate_right(18)
                ^ (w[index - 15] >> 3);
            let s1 = w[index - 2].rotate_right(17)
                ^ w[index - 2].rotate_right(19)
                ^ (w[index - 2] >> 10);
            w[index] = w[index - 16]
                .wrapping_add(s0)
                .wrapping_add(w[index - 7])
                .wrapping_add(s1);
        }

        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = self.state;
        for index in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = h
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[index])
                .wrapping_add(w[index]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);

            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }
        for (slot, value) in self.state.iter_mut().zip([a, b, c, d, e, f, g, h]) {
            *slot = slot.wrapping_add(value);
        }
    }
}

/// The digest of `data`, as lowercase hex.
#[cfg(test)]
pub(crate) fn hex_of(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hasher.hex()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn digest(data: &[u8]) -> String {
        hex_of(data)
    }

    #[test]
    fn matches_the_fips_180_4_vectors() {
        assert_eq!(
            digest(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            digest(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            digest(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
        assert_eq!(
            digest(b"abcdefghbcdefghicdefghijdefghijkefghijklfghijklmghijklmnhijklmnoijklmnopjklmnopqklmnopqrlmnopqrsmnopqrstnopqrstu"),
            "cf5b16a778af8380036ce59e7b0492370b249b11e8f07a51afac45037afee9d1"
        );
    }

    #[test]
    fn the_million_byte_vector_passes() {
        // The vector every implementation gets right by accident. It exists
        // here because it is the one that catches a padding bug.
        let data = vec![b'a'; 1_000_000];
        assert_eq!(
            digest(&data),
            "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0"
        );
    }

    #[test]
    fn incremental_updates_match_a_single_update() {
        let data: Vec<u8> = (0u8..=255).cycle().take(100_000).collect();
        let mut split = Sha256::new();
        split.update(&data[..1]);
        split.update(&data[1..63]);
        split.update(&data[63..64]);
        split.update(&data[64..9999]);
        split.update(&data[9999..]);
        assert_eq!(split.hex(), digest(&data));
    }

    #[test]
    fn a_multi_block_message_is_hashed_across_block_boundaries() {
        // 55, 56, 63, 64 and 65 bytes exercise every padding path.
        for len in [55usize, 56, 63, 64, 65, 119, 120] {
            let data = vec![b'a'; len];
            let mut hasher = Sha256::new();
            hasher.update(&data);
            let once = hasher.hex();
            let mut split = Sha256::new();
            split.update(&data[..len / 2]);
            split.update(&data[len / 2..]);
            assert_eq!(once, split.hex(), "len {len}");
        }
    }
}
