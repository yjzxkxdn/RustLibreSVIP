//! Zstandard compression.
//!
//! Native builds use the reference C implementation (`zstd`), which is what the
//! Python build links, so `.acep`/`.dspx` output stays byte-identical. `wasm32`
//! has no C toolchain here, so it uses a pure-Rust decoder (`ruzstd`, which
//! reads every frame the C library produces) and a minimal frame writer that
//! emits *stored* blocks: valid zstd, just not compressed.

#[cfg(not(target_arch = "wasm32"))]
pub fn compress(data: &[u8], level: i32) -> crate::Result<Vec<u8>> {
    zstd::bulk::compress(data, level)
        .map_err(|error| crate::LibreSvipError::msg(format!("zstd compression failed: {error}")))
}

#[cfg(not(target_arch = "wasm32"))]
pub fn decompress(data: &[u8]) -> crate::Result<Vec<u8>> {
    zstd::stream::decode_all(data)
        .map_err(|error| crate::LibreSvipError::msg(format!("zstd decompression failed: {error}")))
}

/// Zstandard frame magic number, little endian.
pub const MAGIC: [u8; 4] = [0x28, 0xB5, 0x2F, 0xFD];

/// Largest content a single (raw) block can hold.
const MAX_BLOCK: usize = 128 * 1024;

#[cfg(target_arch = "wasm32")]
pub fn compress(data: &[u8], _level: i32) -> crate::Result<Vec<u8>> {
    Ok(compress_stored(data))
}

#[cfg(target_arch = "wasm32")]
pub fn decompress(data: &[u8]) -> crate::Result<Vec<u8>> {
    use std::io::Read;

    let mut decoder = ruzstd::decoding::StreamingDecoder::new(data)
        .map_err(|error| crate::LibreSvipError::msg(format!("zstd decompression failed: {error}")))?;
    let mut output = Vec::new();
    decoder
        .read_to_end(&mut output)
        .map_err(|error| crate::LibreSvipError::msg(format!("zstd decompression failed: {error}")))?;
    Ok(output)
}

/// Emits a zstd frame whose blocks are stored verbatim.
///
/// Layout: magic, frame header (single segment, 8 byte content size), one or
/// more raw blocks, the last one marked as such. Any zstd decoder accepts this;
/// it simply gains nothing from the compression step.
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
pub fn compress_stored(data: &[u8]) -> Vec<u8> {
    let mut output = Vec::with_capacity(data.len() + 16 + data.len() / MAX_BLOCK * 3);
    output.extend_from_slice(&MAGIC);
    // Frame_Header_Descriptor: FCS_flag = 3 (8 byte size), single segment,
    // no checksum, no dictionary.
    output.push(0b1110_0000);
    output.extend_from_slice(&(data.len() as u64).to_le_bytes());
    if data.is_empty() {
        output.extend_from_slice(&[0b0000_0001, 0x00, 0x00]);
        return output;
    }
    let mut chunks = data.chunks(MAX_BLOCK).peekable();
    while let Some(chunk) = chunks.next() {
        let last = chunks.peek().is_none();
        let header = ((chunk.len() as u32) << 3) | u32::from(last);
        output.extend_from_slice(&header.to_le_bytes()[..3]);
        output.extend_from_slice(chunk);
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stored_frames_round_trip_through_zstd() {
        for size in [0usize, 1, 1000, MAX_BLOCK, MAX_BLOCK + 1, MAX_BLOCK * 3 + 7] {
            let data: Vec<u8> = (0..size).map(|index| (index * 31 % 251) as u8).collect();
            let frame = compress_stored(&data);
            assert_eq!(&frame[..4], &MAGIC);
            let decoded = decompress(&frame).expect("stored frame must decode");
            assert_eq!(decoded, data, "round trip failed for {size} bytes");
        }
    }

    #[test]
    fn native_round_trip() {
        let data = vec![b'a'; 10_000];
        let frame = compress(&data, 3).expect("compress");
        assert_eq!(decompress(&frame).expect("decompress"), data);
    }
}
