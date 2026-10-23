//! Cryptographic primitives needed by the PocketSinger container.
//!
//! The workspace crate list does not include `aes`, `sha1`, `hmac`, `pbkdf2`
//! or `md-5`, so the small subset required by the WinZip AES (AE-2) container
//! and the BGM file hash is implemented here:
//!
//! * AES-256 block encryption and CTR mode (WinZip AES)
//! * SHA-1, HMAC-SHA-1 and PBKDF2-HMAC-SHA-1 (key derivation)
//! * MD5 (audio file checksum written to `bgm_info`)
//!
//! All functions are pure and never panic on malformed input lengths.

// ---------------------------------------------------------------------------
// AES-256 (encryption only; CTR needs no decryption routine)
// ---------------------------------------------------------------------------

const SBOX: [u8; 256] = [
    0x63, 0x7c, 0x77, 0x7b, 0xf2, 0x6b, 0x6f, 0xc5, 0x30, 0x01, 0x67, 0x2b, 0xfe, 0xd7, 0xab, 0x76,
    0xca, 0x82, 0xc9, 0x7d, 0xfa, 0x59, 0x47, 0xf0, 0xad, 0xd4, 0xa2, 0xaf, 0x9c, 0xa4, 0x72, 0xc0,
    0xb7, 0xfd, 0x93, 0x26, 0x36, 0x3f, 0xf7, 0xcc, 0x34, 0xa5, 0xe5, 0xf1, 0x71, 0xd8, 0x31, 0x15,
    0x04, 0xc7, 0x23, 0xc3, 0x18, 0x96, 0x05, 0x9a, 0x07, 0x12, 0x80, 0xe2, 0xeb, 0x27, 0xb2, 0x75,
    0x09, 0x83, 0x2c, 0x1a, 0x1b, 0x6e, 0x5a, 0xa0, 0x52, 0x3b, 0xd6, 0xb3, 0x29, 0xe3, 0x2f, 0x84,
    0x53, 0xd1, 0x00, 0xed, 0x20, 0xfc, 0xb1, 0x5b, 0x6a, 0xcb, 0xbe, 0x39, 0x4a, 0x4c, 0x58, 0xcf,
    0xd0, 0xef, 0xaa, 0xfb, 0x43, 0x4d, 0x33, 0x85, 0x45, 0xf9, 0x02, 0x7f, 0x50, 0x3c, 0x9f, 0xa8,
    0x51, 0xa3, 0x40, 0x8f, 0x92, 0x9d, 0x38, 0xf5, 0xbc, 0xb6, 0xda, 0x21, 0x10, 0xff, 0xf3, 0xd2,
    0xcd, 0x0c, 0x13, 0xec, 0x5f, 0x97, 0x44, 0x17, 0xc4, 0xa7, 0x7e, 0x3d, 0x64, 0x5d, 0x19, 0x73,
    0x60, 0x81, 0x4f, 0xdc, 0x22, 0x2a, 0x90, 0x88, 0x46, 0xee, 0xb8, 0x14, 0xde, 0x5e, 0x0b, 0xdb,
    0xe0, 0x32, 0x3a, 0x0a, 0x49, 0x06, 0x24, 0x5c, 0xc2, 0xd3, 0xac, 0x62, 0x91, 0x95, 0xe4, 0x79,
    0xe7, 0xc8, 0x37, 0x6d, 0x8d, 0xd5, 0x4e, 0xa9, 0x6c, 0x56, 0xf4, 0xea, 0x65, 0x7a, 0xae, 0x08,
    0xba, 0x78, 0x25, 0x2e, 0x1c, 0xa6, 0xb4, 0xc6, 0xe8, 0xdd, 0x74, 0x1f, 0x4b, 0xbd, 0x8b, 0x8a,
    0x70, 0x3e, 0xb5, 0x66, 0x48, 0x03, 0xf6, 0x0e, 0x61, 0x35, 0x57, 0xb9, 0x86, 0xc1, 0x1d, 0x9e,
    0xe1, 0xf8, 0x98, 0x11, 0x69, 0xd9, 0x8e, 0x94, 0x9b, 0x1e, 0x87, 0xe9, 0xce, 0x55, 0x28, 0xdf,
    0x8c, 0xa1, 0x89, 0x0d, 0xbf, 0xe6, 0x42, 0x68, 0x41, 0x99, 0x2d, 0x0f, 0xb0, 0x54, 0xbb, 0x16,
];

fn xtime(value: u8) -> u8 {
    (value << 1) ^ if value & 0x80 != 0 { 0x1b } else { 0x00 }
}

fn sub_word(word: u32) -> u32 {
    let bytes = word.to_be_bytes();
    u32::from_be_bytes([
        SBOX[bytes[0] as usize],
        SBOX[bytes[1] as usize],
        SBOX[bytes[2] as usize],
        SBOX[bytes[3] as usize],
    ])
}

fn rot_word(word: u32) -> u32 {
    word.rotate_left(8)
}

/// AES-256 key schedule: 60 words.
fn expand_key(key: &[u8; 32]) -> [u32; 60] {
    const NK: usize = 8;
    const NR: usize = 14;
    let mut words = [0u32; 4 * (NR + 1)];
    for index in 0..NK {
        words[index] = u32::from_be_bytes([
            key[4 * index],
            key[4 * index + 1],
            key[4 * index + 2],
            key[4 * index + 3],
        ]);
    }
    let mut rcon: u8 = 1;
    for index in NK..4 * (NR + 1) {
        let mut temp = words[index - 1];
        if index % NK == 0 {
            temp = sub_word(rot_word(temp)) ^ (u32::from(rcon) << 24);
            rcon = xtime(rcon);
        } else if index % NK == 4 {
            temp = sub_word(temp);
        }
        words[index] = words[index - NK] ^ temp;
    }
    words
}

const AES_ROUNDS: usize = 14;

fn add_round_key(state: &mut [u8; 16], words: &[u32; 60], round: usize) {
    for column in 0..4 {
        let word = words[round * 4 + column].to_be_bytes();
        for row in 0..4 {
            state[4 * column + row] ^= word[row];
        }
    }
}

fn sub_bytes(state: &mut [u8; 16]) {
    for byte in state.iter_mut() {
        *byte = SBOX[*byte as usize];
    }
}

fn shift_rows(state: &mut [u8; 16]) {
    let original = *state;
    for row in 0..4 {
        for column in 0..4 {
            state[4 * column + row] = original[4 * ((column + row) % 4) + row];
        }
    }
}

fn mix_columns(state: &mut [u8; 16]) {
    for column in 0..4 {
        let s0 = state[4 * column];
        let s1 = state[4 * column + 1];
        let s2 = state[4 * column + 2];
        let s3 = state[4 * column + 3];
        state[4 * column] = xtime(s0) ^ (xtime(s1) ^ s1) ^ s2 ^ s3;
        state[4 * column + 1] = s0 ^ xtime(s1) ^ (xtime(s2) ^ s2) ^ s3;
        state[4 * column + 2] = s0 ^ s1 ^ xtime(s2) ^ (xtime(s3) ^ s3);
        state[4 * column + 3] = (xtime(s0) ^ s0) ^ s1 ^ s2 ^ xtime(s3);
    }
}

fn aes256_encrypt_block(words: &[u32; 60], block: &[u8; 16]) -> [u8; 16] {
    let mut state = *block;
    add_round_key(&mut state, words, 0);
    for round in 1..AES_ROUNDS {
        sub_bytes(&mut state);
        shift_rows(&mut state);
        mix_columns(&mut state);
        add_round_key(&mut state, words, round);
    }
    sub_bytes(&mut state);
    shift_rows(&mut state);
    add_round_key(&mut state, words, AES_ROUNDS);
    state
}

/// AES-256-CTR with WinZip AES' 16-byte little-endian block counter starting
/// at 1.
pub fn aes256_ctr(key: &[u8; 32], data: &[u8]) -> Vec<u8> {
    let words = expand_key(key);
    let mut counter = [0u8; 16];
    let mut output = Vec::with_capacity(data.len());
    let mut offset = 0usize;
    while offset < data.len() {
        // Increment the 16-byte little-endian counter.
        for byte in counter.iter_mut() {
            let (value, carry) = byte.overflowing_add(1);
            *byte = value;
            if !carry {
                break;
            }
        }
        let keystream = aes256_encrypt_block(&words, &counter);
        let chunk = (data.len() - offset).min(16);
        for index in 0..chunk {
            output.push(data[offset + index] ^ keystream[index]);
        }
        offset += chunk;
    }
    output
}

// ---------------------------------------------------------------------------
// SHA-1 / HMAC-SHA-1 / PBKDF2
// ---------------------------------------------------------------------------

pub fn sha1(data: &[u8]) -> [u8; 20] {
    let mut h: [u32; 5] = [0x67452301, 0xEFCDAB89, 0x98BADCFE, 0x10325476, 0xC3D2E1F0];
    let mut message = data.to_vec();
    let bit_len = (data.len() as u64).wrapping_mul(8);
    message.push(0x80);
    while message.len() % 64 != 56 {
        message.push(0);
    }
    message.extend_from_slice(&bit_len.to_be_bytes());
    for chunk in message.chunks_exact(64) {
        let mut w = [0u32; 80];
        for index in 0..16 {
            w[index] = u32::from_be_bytes([
                chunk[4 * index],
                chunk[4 * index + 1],
                chunk[4 * index + 2],
                chunk[4 * index + 3],
            ]);
        }
        for index in 16..80 {
            w[index] = (w[index - 3] ^ w[index - 8] ^ w[index - 14] ^ w[index - 16]).rotate_left(1);
        }
        let [mut a, mut b, mut c, mut d, mut e] = h;
        for (index, word) in w.iter().enumerate() {
            let (f, k) = match index {
                0..=19 => ((b & c) | ((!b) & d), 0x5A827999u32),
                20..=39 => (b ^ c ^ d, 0x6ED9EBA1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1BBCDC),
                _ => (b ^ c ^ d, 0xCA62C1D6),
            };
            let temp = a
                .rotate_left(5)
                .wrapping_add(f)
                .wrapping_add(e)
                .wrapping_add(k)
                .wrapping_add(*word);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = temp;
        }
        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
    }
    let mut output = [0u8; 20];
    for (index, value) in h.iter().enumerate() {
        output[4 * index..4 * index + 4].copy_from_slice(&value.to_be_bytes());
    }
    output
}

fn hmac_sha1(key: &[u8], data: &[u8]) -> [u8; 20] {
    const BLOCK: usize = 64;
    let mut key_block = [0u8; BLOCK];
    if key.len() > BLOCK {
        let digest = sha1(key);
        key_block[..20].copy_from_slice(&digest);
    } else {
        key_block[..key.len()].copy_from_slice(key);
    }
    let mut inner = Vec::with_capacity(BLOCK + data.len());
    let mut outer = Vec::with_capacity(BLOCK + 20);
    for byte in key_block.iter() {
        inner.push(byte ^ 0x36);
    }
    inner.extend_from_slice(data);
    let inner_digest = sha1(&inner);
    for byte in key_block.iter() {
        outer.push(byte ^ 0x5c);
    }
    outer.extend_from_slice(&inner_digest);
    sha1(&outer)
}

/// HMAC-SHA-1 truncated to `len` bytes (WinZip AES authentication code).
pub fn hmac_sha1_truncated(key: &[u8], data: &[u8], len: usize) -> Vec<u8> {
    let digest = hmac_sha1(key, data);
    digest[..len.min(20)].to_vec()
}

/// PBKDF2-HMAC-SHA-1.
pub fn pbkdf2_hmac_sha1(password: &[u8], salt: &[u8], iterations: u32, key_len: usize) -> Vec<u8> {
    let mut output = Vec::with_capacity(key_len + 20);
    let mut block_index: u32 = 1;
    while output.len() < key_len {
        let mut salt_block = salt.to_vec();
        salt_block.extend_from_slice(&block_index.to_be_bytes());
        let mut u = hmac_sha1(password, &salt_block);
        let mut t = u;
        for _ in 1..iterations {
            u = hmac_sha1(password, &u);
            for index in 0..20 {
                t[index] ^= u[index];
            }
        }
        output.extend_from_slice(&t);
        block_index += 1;
    }
    output.truncate(key_len);
    output
}

// ---------------------------------------------------------------------------
// MD5 (used for `bgm_info[*].fileMD5`)
// ---------------------------------------------------------------------------

const MD5_SHIFTS: [u32; 64] = [
    7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 5, 9, 14, 20, 5, 9, 14, 20, 5, 9,
    14, 20, 5, 9, 14, 20, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 6, 10, 15,
    21, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21,
];

const MD5_K: [u32; 64] = [
    0xd76aa478, 0xe8c7b756, 0x242070db, 0xc1bdceee, 0xf57c0faf, 0x4787c62a, 0xa8304613, 0xfd469501,
    0x698098d8, 0x8b44f7af, 0xffff5bb1, 0x895cd7be, 0x6b901122, 0xfd987193, 0xa679438e, 0x49b40821,
    0xf61e2562, 0xc040b340, 0x265e5a51, 0xe9b6c7aa, 0xd62f105d, 0x02441453, 0xd8a1e681, 0xe7d3fbc8,
    0x21e1cde6, 0xc33707d6, 0xf4d50d87, 0x455a14ed, 0xa9e3e905, 0xfcefa3f8, 0x676f02d9, 0x8d2a4c8a,
    0xfffa3942, 0x8771f681, 0x6d9d6122, 0xfde5380c, 0xa4beea44, 0x4bdecfa9, 0xf6bb4b60, 0xbebfbc70,
    0x289b7ec6, 0xeaa127fa, 0xd4ef3085, 0x04881d05, 0xd9d4d039, 0xe6db99e5, 0x1fa27cf8, 0xc4ac5665,
    0xf4292244, 0x432aff97, 0xab9423a7, 0xfc93a039, 0x655b59c3, 0x8f0ccc92, 0xffeff47d, 0x85845dd1,
    0x6fa87e4f, 0xfe2ce6e0, 0xa3014314, 0x4e0811a1, 0xf7537e82, 0xbd3af235, 0x2ad7d2bb, 0xeb86d391,
];

pub fn md5_hex(data: &[u8]) -> String {
    let mut message = data.to_vec();
    let bit_len = (data.len() as u64).wrapping_mul(8);
    message.push(0x80);
    while message.len() % 64 != 56 {
        message.push(0);
    }
    message.extend_from_slice(&bit_len.to_le_bytes());
    let mut a0: u32 = 0x67452301;
    let mut b0: u32 = 0xefcdab89;
    let mut c0: u32 = 0x98badcfe;
    let mut d0: u32 = 0x10325476;
    for chunk in message.chunks_exact(64) {
        let mut m = [0u32; 16];
        for index in 0..16 {
            m[index] = u32::from_le_bytes([
                chunk[4 * index],
                chunk[4 * index + 1],
                chunk[4 * index + 2],
                chunk[4 * index + 3],
            ]);
        }
        let (mut a, mut b, mut c, mut d) = (a0, b0, c0, d0);
        for index in 0..64 {
            let (f, g) = match index {
                0..=15 => ((b & c) | ((!b) & d), index),
                16..=31 => ((d & b) | ((!d) & c), (5 * index + 1) % 16),
                32..=47 => (b ^ c ^ d, (3 * index + 5) % 16),
                _ => (c ^ (b | (!d)), (7 * index) % 16),
            };
            let temp = d;
            d = c;
            c = b;
            b = b.wrapping_add(
                a.wrapping_add(f)
                    .wrapping_add(MD5_K[index])
                    .wrapping_add(m[g])
                    .rotate_left(MD5_SHIFTS[index]),
            );
            a = temp;
        }
        a0 = a0.wrapping_add(a);
        b0 = b0.wrapping_add(b);
        c0 = c0.wrapping_add(c);
        d0 = d0.wrapping_add(d);
    }
    let mut digest = [0u8; 16];
    digest[0..4].copy_from_slice(&a0.to_le_bytes());
    digest[4..8].copy_from_slice(&b0.to_le_bytes());
    digest[8..12].copy_from_slice(&c0.to_le_bytes());
    digest[12..16].copy_from_slice(&d0.to_le_bytes());
    let mut output = String::with_capacity(32);
    for byte in digest {
        output.push_str(&format!("{byte:02x}"));
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha1_vectors() {
        assert_eq!(
            sha1(b"abc").iter().map(|b| format!("{b:02x}")).collect::<String>(),
            "a9993e364706816aba3e25717850c26c9cd0d89d"
        );
        assert_eq!(
            sha1(b"").iter().map(|b| format!("{b:02x}")).collect::<String>(),
            "da39a3ee5e6b4b0d3255bfef95601890afd80709"
        );
    }

    #[test]
    fn md5_vectors() {
        assert_eq!(md5_hex(b""), "d41d8cd98f00b204e9800998ecf8427e");
        assert_eq!(md5_hex(b"abc"), "900150983cd24fb0d6963f7d28e17f72");
    }

    #[test]
    fn aes256_known_vector() {
        // FIPS-197 Appendix C.3 (single block).
        let key: [u8; 32] = [
            0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d,
            0x0e, 0x0f, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b,
            0x1c, 0x1d, 0x1e, 0x1f,
        ];
        let plain: [u8; 16] = [
            0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd,
            0xee, 0xff,
        ];
        let words = expand_key(&key);
        let cipher = aes256_encrypt_block(&words, &plain);
        assert_eq!(
            cipher,
            [
                0x8e, 0xa2, 0xb7, 0xca, 0x51, 0x67, 0x45, 0xbf, 0xea, 0xfc, 0x49, 0x90, 0x4b,
                0x49, 0x60, 0x89
            ]
        );
    }

    #[test]
    fn pbkdf2_vector() {
        // RFC 6070 (PBKDF2-HMAC-SHA1, c=1).
        let output = pbkdf2_hmac_sha1(b"password", b"salt", 1, 20);
        assert_eq!(
            output.iter().map(|b| format!("{b:02x}")).collect::<String>(),
            "0c60c80f961f0e71f3a9b524af6012062fe037a6"
        );
    }
}
