//! Zstandard compression.
//!
//! Native builds use the reference C implementation (`zstd`), which is what the
//! Python build links, so `.acep`/`.dspx` output stays byte-identical. `wasm32`
//! has no C toolchain here, so it uses `ruzstd`: a pure-Rust decoder that reads
//! every frame the C library produces, and its fast encoder.

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

#[cfg(target_arch = "wasm32")]
pub fn compress(data: &[u8], _level: i32) -> crate::Result<Vec<u8>> {
    // `ruzstd` implements a real (fast) encoder but hardcodes "no frame content
    // size" in the header (`TODO` in `encoding/frame_header.rs`). Declare it
    // anyway: decoders that need it up front (e.g. `ZstdDecompressor().decompress`
    // in Python, which is what the ACE Studio tooling is built on) reject the
    // frame otherwise.
    use ruzstd::encoding::{compress_to_vec, CompressionLevel};
    let frame = compress_to_vec(data, CompressionLevel::Fastest);
    Ok(with_content_size(&frame, data.len() as u64))
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

/// Declares the decompressed size in a zstd frame header that omits it
/// (RFC 8878 §3.1.1: `Frame_Content_Size_flag` was 0).
///
/// Frames stay valid either way, but some one-shot decoders refuse a frame
/// without the size, so encoding it makes the output interchangeable with the
/// one the C library produces. Frames that already carry a size, use a single
/// segment or a dictionary are returned unchanged.
pub fn with_content_size(frame: &[u8], content_size: u64) -> Vec<u8> {
    // magic (4) + frame header descriptor (1) + window descriptor (1)
    if frame.len() < 6 || frame[..4] != MAGIC || u64::from(content_size as u32) != content_size {
        return frame.to_vec();
    }
    let descriptor = frame[4];
    let fcs_flag = descriptor >> 6;
    let single_segment = (descriptor >> 5) & 1 == 1;
    let dictionary_flag = descriptor & 0b11;
    if fcs_flag != 0 || single_segment || dictionary_flag != 0 {
        return frame.to_vec();
    }
    let mut output = Vec::with_capacity(frame.len() + 4);
    output.extend_from_slice(&frame[..4]);
    output.push(descriptor | 0b1000_0000);
    output.extend_from_slice(&frame[5..6]);
    output.extend_from_slice(&(content_size as u32).to_le_bytes());
    output.extend_from_slice(&frame[6..]);
    output
}

#[cfg(test)]
mod tests {
    use super::*;


    #[test]
    fn native_round_trip() {
        let data = vec![b'a'; 10_000];
        let frame = compress(&data, 3).expect("compress");
        assert_eq!(decompress(&frame).expect("decompress"), data);
    }

    /// A frame without `Frame_Content_Size` (what `ruzstd` emits) still decodes
    /// after the size is patched in, and the patch is what one-shot decoders
    /// require.
    #[test]
    fn content_size_is_added_to_frames_that_omit_it() {
        // magic, descriptor (no FCS, no single segment, no dictionary),
        // window descriptor, then one raw block holding "hello".
        let mut frame = MAGIC.to_vec();
        frame.push(0b0000_0000);
        frame.push(0x00);
        frame.extend_from_slice(&[0x29, 0x00, 0x00]);
        frame.extend_from_slice(b"hello");
        assert_eq!(decompress(&frame).expect("plain frame decodes"), b"hello");

        let patched = with_content_size(&frame, 5);
        assert_eq!(patched[4] >> 6, 2, "FCS_flag must be 2");
        assert_eq!(&patched[6..10], &5u32.to_le_bytes());
        assert_eq!(decompress(&patched).expect("patched frame decodes"), b"hello");

        // Frames that already declare their size are left alone.
        let sized = compress(&b"hello"[..], 3).expect("compress");
        assert_eq!(with_content_size(&sized, 5), sized);
        // Non-zstd input is passed through.
        assert_eq!(with_content_size(b"nope", 4), b"nope");
    }
}

/// Inflates a zlib stream (`python -c "import zlib; zlib.compress(data, 9)"`),
/// used for the embedded phoneme dictionaries so the wasm data section stays
/// small; decompression is lazy and one-shot.
pub fn zlib_decompress(data: &[u8]) -> crate::Result<Vec<u8>> {
    use std::io::Read;
    let mut decoder = flate2::read::ZlibDecoder::new(data);
    let mut out = Vec::new();
    decoder
        .read_to_end(&mut out)
        .map_err(|error| crate::LibreSvipError::msg(format!("zlib decompression failed: {error}")))?;
    Ok(out)
}
