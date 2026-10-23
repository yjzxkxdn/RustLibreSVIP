//! Port of `libresvip.plugins.acep.color_pool`.

pub const COLORS: [&str; 14] = [
    "#b17ef3", "#da9de3", "#dd88d5", "#f57db7", "#dd8585", "#ebac99", "#f4a070", "#f1c970",
    "#c0cf63", "#99f07b", "#70d4b1", "#91bcdc", "#96afef", "#979ae4",
];

/// `count_color`.
pub fn count_color() -> i64 {
    COLORS.len() as i64
}

/// `get_color`.
pub fn get_color(index: i64) -> String {
    let count = COLORS.len() as i64;
    let mut index = index % count;
    if index < 0 {
        index += count;
    }
    COLORS[index as usize].to_owned()
}

/// `random.randint(0, count_color() - 1)`.
///
/// Python seeds `random` from the OS entropy pool, so the reference output is
/// already non-deterministic; the Rust port draws from a clock/pid mix instead
/// of pulling in an RNG crate.
pub fn random_color_index() -> i64 {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};

    let mut hasher = RandomState::new().build_hasher();
    hasher.write_u128(libresvip_core::platform::unix_nanos());
    hasher.write_u32(libresvip_core::platform::process_id());
    (hasher.finish() % count_color() as u64) as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colour_pool_wraps() {
        assert_eq!(count_color(), 14);
        assert_eq!(get_color(0), "#b17ef3");
        assert_eq!(get_color(14), "#b17ef3");
        assert_eq!(get_color(11), "#91bcdc");
    }
}
