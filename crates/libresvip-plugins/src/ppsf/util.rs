//! Small helpers shared by the `svip3`, `ppsf` and `ps_project` plugins.

use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hash, Hasher};

/// Python's `round(value)` for a single argument: round half to even.

pub use libresvip_core::core::compat::py_round;

fn entropy_u64() -> u64 {
    let state = RandomState::new();
    let mut hasher = state.build_hasher();
    libresvip_core::platform::unix_nanos().hash(&mut hasher);
    libresvip_core::platform::process_id().hash(&mut hasher);
    hasher.finish()
}

/// A tiny xorshift generator; `random.choice` only needs uniform-enough picks
/// and the Python output is non-deterministic anyway.
pub fn random_index(len: usize) -> usize {
    if len == 0 {
        return 0;
    }
    let mut state = entropy_u64() | 1;
    state ^= state << 13;
    state ^= state >> 7;
    state ^= state << 17;
    (state % len as u64) as usize
}

/// `uuid.uuid4()` — a random version 4 UUID in canonical lowercase form.
pub fn uuid_v4() -> String {
    let mut bytes = [0u8; 16];
    for (index, chunk) in bytes.chunks_mut(8).enumerate() {
        let value = entropy_u64()
            .to_le_bytes()
            .map(|byte| byte.wrapping_add(index as u8));
        chunk.copy_from_slice(&value);
    }
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7], bytes[8],
        bytes[9], bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15]
    )
}
