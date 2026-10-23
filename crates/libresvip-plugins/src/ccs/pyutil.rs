//! Small helpers for the CCS plugin: UUIDs and XML escaping.

use std::sync::atomic::{AtomicU64, Ordering};

/// `str(uuid.uuid4())` — a random-looking v4 UUID in canonical form.
pub fn uuid_str() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let mut state = (libresvip_core::platform::unix_nanos() as u64)
        ^ (COUNTER.fetch_add(1, Ordering::Relaxed).wrapping_mul(0x9E37_79B9_7F4A_7C15));
    let mut bytes = [0u8; 16];
    for slot in &mut bytes {
        // xorshift64* — enough entropy for an identifier that is never used
        // for anything but matching units with their groups.
        state ^= state >> 12;
        state ^= state << 25;
        state ^= state >> 27;
        *slot = (state.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 56) as u8;
    }
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[0],
        bytes[1],
        bytes[2],
        bytes[3],
        bytes[4],
        bytes[5],
        bytes[6],
        bytes[7],
        bytes[8],
        bytes[9],
        bytes[10],
        bytes[11],
        bytes[12],
        bytes[13],
        bytes[14],
        bytes[15],
    )
}

/// Escapes text content (xsdata escapes `&`, `<`, `>`).
pub fn escape_text(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            other => out.push(other),
        }
    }
    out
}

/// Escapes an attribute value.
pub fn escape_attribute(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            other => out.push(other),
        }
    }
    out
}
