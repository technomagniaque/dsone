//! Deezer "BF_CBC_STRIPE" decryption.
//!
//! The CDN serves the audio file split into 2048-byte chunks. Every third
//! chunk (absolute chunk index `i % 3 == 0`) is Blowfish-CBC encrypted with a
//! per-track key and a fixed IV, with no padding and a fresh IV per chunk. All
//! other chunks, and a trailing chunk shorter than 2048 bytes, are plain.

use blowfish::Blowfish;
use cbc::cipher::{block_padding::NoPadding, BlockDecryptMut, KeyIvInit};

/// Size of one stripe chunk, in bytes.
pub const CHUNK_SIZE: usize = 2048;

const SECRET: &[u8; 16] = b"g4el58wc0zvf9na1";
const IV: [u8; 8] = [0, 1, 2, 3, 4, 5, 6, 7];

type BfCbcDec = cbc::Decryptor<Blowfish>;

/// Blowfish key for a track: `md5hex(id)[i] ^ md5hex(id)[i + 16] ^ SECRET[i]`.
///
/// `track_id` is the id of the track whose file is actually streamed (for a
/// region fallback this is the fallback's id, not the one the user picked).
pub fn track_key(track_id: u64) -> [u8; 16] {
    let hex = format!("{:x}", md5::compute(track_id.to_string().as_bytes()));
    let hex = hex.as_bytes();
    let mut key = [0u8; 16];
    for (i, k) in key.iter_mut().enumerate() {
        *k = hex[i] ^ hex[i + 16] ^ SECRET[i];
    }
    key
}

/// Decrypt one full 2048-byte chunk in place.
pub fn decrypt_chunk(key: &[u8; 16], chunk: &mut [u8]) {
    debug_assert_eq!(chunk.len(), CHUNK_SIZE);
    // Blowfish accepts 4..=56 byte keys and the IV is a constant 8 bytes, so
    // neither length can be wrong here.
    let dec = BfCbcDec::new_from_slices(key, &IV).expect("16-byte key and 8-byte IV are valid");
    // 2048 is a multiple of the 8-byte block size, so NoPadding cannot fail.
    let _ = dec.decrypt_padded_mut::<NoPadding>(chunk);
}

/// Incremental stripe decryptor fed with arbitrary-sized network buffers.
///
/// It must be started at a chunk boundary: `first_chunk` is the absolute
/// index (byte offset / 2048) of the first byte it will be fed.
pub struct StripeDecryptor {
    key: [u8; 16],
    chunk_index: u64,
    pending: Vec<u8>,
}

impl StripeDecryptor {
    pub fn new(key: [u8; 16], first_chunk: u64) -> Self {
        Self {
            key,
            chunk_index: first_chunk,
            pending: Vec::with_capacity(CHUNK_SIZE),
        }
    }

    /// Feed bytes; returns every byte that could be completed (whole chunks).
    pub fn feed(&mut self, mut data: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(data.len() + self.pending.len());
        while !data.is_empty() {
            let need = CHUNK_SIZE - self.pending.len();
            let take = need.min(data.len());
            self.pending.extend_from_slice(&data[..take]);
            data = &data[take..];
            if self.pending.len() == CHUNK_SIZE {
                if self.chunk_index.is_multiple_of(3) {
                    decrypt_chunk(&self.key, &mut self.pending);
                }
                out.extend_from_slice(&self.pending);
                self.pending.clear();
                self.chunk_index += 1;
            }
        }
        out
    }

    /// End of stream: a trailing partial chunk is never encrypted.
    pub fn finish(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.pending)
    }
}

#[cfg(test)]
pub(crate) fn encrypt_stripes(key: &[u8; 16], first_chunk: u64, data: &[u8]) -> Vec<u8> {
    use cbc::cipher::BlockEncryptMut;
    type BfCbcEnc = cbc::Encryptor<Blowfish>;
    let mut out = data.to_vec();
    for (n, chunk) in out.chunks_mut(CHUNK_SIZE).enumerate() {
        if chunk.len() == CHUNK_SIZE && (first_chunk + n as u64).is_multiple_of(3) {
            let enc = BfCbcEnc::new_from_slices(key, &IV).unwrap();
            let _ = enc.encrypt_padded_mut::<NoPadding>(chunk, CHUNK_SIZE);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(b: &[u8]) -> String {
        b.iter().map(|x| format!("{x:02x}")).collect()
    }

    #[test]
    fn deezer_key_derivation_matches_independent_vectors() {
        // Computed with Python's hashlib, outside this code.
        assert_eq!(hex(&track_key(3135556)), "6c6c666b39662c37652575603c643439");
        assert_eq!(&track_key(3135556), b"llfk9f,7e%u`<d49");
        assert_eq!(hex(&track_key(1)), "3464656e343a7d3a672c236a33696061");
    }

    #[test]
    fn deezer_chunk_decrypt_matches_openssl_vector() {
        // Ciphertext from `openssl enc -bf-cbc -K <key> -iv 0001020304050607
        // -nopad` over bytes i % 256 for i in 0..2048.
        let plain: Vec<u8> = (0..CHUNK_SIZE).map(|i| (i % 256) as u8).collect();
        let key = track_key(3135556);
        let mut ct = encrypt_stripes(&key, 0, &plain);
        assert_eq!(hex(&ct[..16]), "367d4cd324ec3aa13d894f19198289f1");
        assert_eq!(
            hex(&ct[CHUNK_SIZE - 16..]),
            "9718527e99b05b853936b86c1c9c3bbc"
        );
        decrypt_chunk(&key, &mut ct);
        assert_eq!(ct, plain);
    }

    fn sample(len: usize) -> Vec<u8> {
        (0..len).map(|i| (i * 31 + 7) as u8).collect()
    }

    #[test]
    fn deezer_stripe_roundtrip_with_odd_tail_and_odd_feeds() {
        let key = track_key(987654321);
        let plain = sample(CHUNK_SIZE * 7 + 1001);
        let enc = encrypt_stripes(&key, 0, &plain);
        assert_ne!(enc[..CHUNK_SIZE], plain[..CHUNK_SIZE]);
        assert_eq!(
            enc[CHUNK_SIZE..2 * CHUNK_SIZE],
            plain[CHUNK_SIZE..2 * CHUNK_SIZE]
        );
        assert_eq!(enc[7 * CHUNK_SIZE..], plain[7 * CHUNK_SIZE..]);

        for feed in [1usize, 7, 500, 2048, 3000, 100_000] {
            let mut dec = StripeDecryptor::new(key, 0);
            let mut out = Vec::new();
            for piece in enc.chunks(feed) {
                out.extend(dec.feed(piece));
            }
            out.extend(dec.finish());
            assert_eq!(out, plain, "feed size {feed}");
        }
    }

    #[test]
    fn deezer_stripe_decrypt_from_mid_stream_chunk() {
        let key = track_key(42);
        let plain = sample(CHUNK_SIZE * 10 + 17);
        let enc = encrypt_stripes(&key, 0, &plain);
        for first in [1u64, 3, 4, 9] {
            let start = first as usize * CHUNK_SIZE;
            let mut dec = StripeDecryptor::new(key, first);
            let mut out = dec.feed(&enc[start..]);
            out.extend(dec.finish());
            assert_eq!(out, plain[start..], "first chunk {first}");
        }
    }
}
