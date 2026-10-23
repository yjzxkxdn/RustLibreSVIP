//! Minimal ZIP reader/writer with WinZip AES (AE-1/AE-2) support.
//!
//! The `zip` crate in the workspace dependency list only handles the classic
//! ZIP layout (and no AES), while PocketSinger projects are AES-256 encrypted
//! archives produced by `zipctl`. This module implements just enough of the
//! container for `ps_project`:
//!
//! * reads the end-of-central-directory record and central directory,
//! * decrypts entries carrying the `0x9901` AES extra field (AE-2: password
//!   verification value and 10-byte HMAC-SHA1 authentication code),
//! * writes stored (uncompressed) AES-256 entries in the same layout `zipctl`
//!   produces (method 99, CRC 0, 7-byte AES extra field in both headers).
//!
//! Layout of one encrypted entry (WinZip AES):
//! `salt(8/12/16) | password-verification(2) | ciphertext | mac(10)`.

use libresvip_core::error::{LibreSvipError, Result};

use super::crypto::{aes256_ctr, md5_hex, pbkdf2_hmac_sha1};

const LOCAL_HEADER_SIGNATURE: u32 = 0x0403_4b50;
const CENTRAL_HEADER_SIGNATURE: u32 = 0x0201_4b50;
const EOCD_SIGNATURE: u32 = 0x0605_4b50;
const AES_EXTRA_ID: u16 = 0x9901;
/// Method id marking a WinZip AES entry.
pub const METHOD_AES: u16 = 99;
const METHOD_STORED: u16 = 0;
const METHOD_DEFLATE: u16 = 8;

/// One archive member.
#[derive(Clone, Debug)]
pub struct ZipEntry {
    pub name: String,
    pub data: Vec<u8>,
}

fn invalid(message: impl Into<String>) -> LibreSvipError {
    LibreSvipError::Validation(message.into())
}

fn read_u16(data: &[u8], offset: usize) -> Result<u16> {
    let slice = data
        .get(offset..offset + 2)
        .ok_or_else(|| invalid("truncated zip header"))?;
    Ok(u16::from_le_bytes([slice[0], slice[1]]))
}

fn read_u32(data: &[u8], offset: usize) -> Result<u32> {
    let slice = data
        .get(offset..offset + 4)
        .ok_or_else(|| invalid("truncated zip header"))?;
    Ok(u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]))
}

/// WinZip AES parameters from the `0x9901` extra field.
#[derive(Clone, Copy, Debug)]
struct AesExtra {
    strength: u8,
    actual_method: u16,
}

fn find_aes_extra(extra: &[u8]) -> Option<AesExtra> {
    let mut offset = 0usize;
    while offset + 4 <= extra.len() {
        let id = u16::from_le_bytes([extra[offset], extra[offset + 1]]);
        let size = u16::from_le_bytes([extra[offset + 2], extra[offset + 3]]) as usize;
        let body_start = offset + 4;
        let body = extra.get(body_start..body_start + size)?;
        if id == AES_EXTRA_ID && body.len() >= 7 {
            return Some(AesExtra {
                strength: body[4],
                actual_method: u16::from_le_bytes([body[5], body[6]]),
            });
        }
        offset = body_start + size;
    }
    None
}

fn key_length_for(strength: u8) -> Option<usize> {
    match strength {
        1 => Some(16),
        2 => Some(24),
        3 => Some(32),
        _ => None,
    }
}

fn salt_length_for(strength: u8) -> Option<usize> {
    match strength {
        1 => Some(8),
        2 => Some(12),
        3 => Some(16),
        _ => None,
    }
}

/// Derives `(key, mac_key, verification)` from the password and salt.
fn derive_key(
    password: &[u8],
    salt: &[u8],
    key_len: usize,
) -> (Vec<u8>, Vec<u8>, [u8; 2]) {
    let derived = pbkdf2_hmac_sha1(password, salt, 1000, key_len * 2 + 2);
    let key = derived[..key_len].to_vec();
    let mac_key = derived[key_len..key_len * 2].to_vec();
    let verification = [derived[key_len * 2], derived[key_len * 2 + 1]];
    (key, mac_key, verification)
}

fn decrypt_aes_entry(
    data: &[u8],
    aes: AesExtra,
    password: &[u8],
) -> Result<Vec<u8>> {
    let key_len = key_length_for(aes.strength)
        .ok_or_else(|| invalid("unsupported WinZip AES strength"))?;
    let salt_len = salt_length_for(aes.strength)
        .ok_or_else(|| invalid("unsupported WinZip AES strength"))?;
    if data.len() < salt_len + 2 + 10 {
        return Err(invalid("truncated WinZip AES entry"));
    }
    let salt = &data[..salt_len];
    let verification = [data[salt_len], data[salt_len + 1]];
    let mac_start = data.len() - 10;
    let ciphertext = &data[salt_len + 2..mac_start];
    let mac = &data[mac_start..];
    let (key, mac_key, expected) = derive_key(password, salt, key_len);
    if verification != expected {
        return Err(invalid("bad password for ps_project archive"));
    }
    let mut key_array = [0u8; 32];
    key_array[..key_len].copy_from_slice(&key);
    let plain = aes256_ctr(&key_array, ciphertext);
    let computed = super::crypto::hmac_sha1_truncated(&mac_key, ciphertext, 10);
    if computed != mac {
        return Err(invalid("corrupted ps_project archive (HMAC mismatch)"));
    }
    Ok(plain)
}

fn decompress(data: &[u8], method: u16) -> Result<Vec<u8>> {
    match method {
        METHOD_STORED => Ok(data.to_vec()),
        METHOD_DEFLATE => {
            use std::io::Read;
            let mut decoder = flate2::read::DeflateDecoder::new(data);
            let mut output = Vec::new();
            decoder
                .read_to_end(&mut output)
                .map_err(|error| invalid(format!("deflate error: {error}")))?;
            Ok(output)
        }
        other => Err(invalid(format!("unsupported zip compression method {other}"))),
    }
}

/// Reads every entry of an archive, decrypting AES members with `password`.
pub fn read_archive(bytes: &[u8], password: &[u8]) -> Result<Vec<ZipEntry>> {
    if bytes.len() < 22 {
        return Err(invalid("not a zip archive"));
    }
    // Locate the end-of-central-directory record.
    let mut eocd_offset = None;
    let search_start = bytes.len().saturating_sub(65_557);
    for offset in (search_start..=bytes.len() - 4).rev() {
        if read_u32(bytes, offset)? == EOCD_SIGNATURE {
            eocd_offset = Some(offset);
            break;
        }
    }
    let eocd_offset = eocd_offset.ok_or_else(|| invalid("missing zip end record"))?;
    let entry_count = read_u16(bytes, eocd_offset + 10)? as usize;
    let central_offset = read_u32(bytes, eocd_offset + 16)? as usize;
    let mut entry_offset = central_offset;
    let mut entries = Vec::with_capacity(entry_count);
    for _ in 0..entry_count {
        if read_u32(bytes, entry_offset)? != CENTRAL_HEADER_SIGNATURE {
            return Err(invalid("invalid central directory"));
        }
        let flags = read_u16(bytes, entry_offset + 8)?;
        let method = read_u16(bytes, entry_offset + 10)?;
        let compressed_size = read_u32(bytes, entry_offset + 20)? as usize;
        let name_len = read_u16(bytes, entry_offset + 28)? as usize;
        let extra_len = read_u16(bytes, entry_offset + 30)? as usize;
        let comment_len = read_u16(bytes, entry_offset + 32)? as usize;
        let local_offset = read_u32(bytes, entry_offset + 42)? as usize;
        let name_start = entry_offset + 46;
        let name_bytes = bytes
            .get(name_start..name_start + name_len)
            .ok_or_else(|| invalid("truncated central directory"))?;
        let name = String::from_utf8_lossy(name_bytes).into_owned();
        let extra_start = name_start + name_len;
        let extra = bytes
            .get(extra_start..extra_start + extra_len)
            .ok_or_else(|| invalid("truncated central directory extra field"))?;
        let aes = find_aes_extra(extra);
        // Local header gives the real start of the entry data.
        if read_u32(bytes, local_offset)? != LOCAL_HEADER_SIGNATURE {
            return Err(invalid("invalid local file header"));
        }
        let local_name_len = read_u16(bytes, local_offset + 26)? as usize;
        let local_extra_len = read_u16(bytes, local_offset + 28)? as usize;
        let data_start = local_offset + 30 + local_name_len + local_extra_len;
        let raw = bytes
            .get(data_start..data_start + compressed_size)
            .ok_or_else(|| invalid("truncated zip entry data"))?;
        let data = if flags & 0x0001 != 0 {
            let aes = aes.ok_or_else(|| invalid("encrypted entry is not WinZip AES"))?;
            let plain = decrypt_aes_entry(raw, aes, password)?;
            decompress(&plain, aes.actual_method)?
        } else {
            decompress(raw, method)?
        };
        entries.push(ZipEntry { name, data });
        entry_offset = extra_start + extra_len + comment_len;
    }
    Ok(entries)
}

// ---------------------------------------------------------------------------
// Writing
// ---------------------------------------------------------------------------

pub(crate) fn dos_date_time() -> (u16, u16) {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    // Days since the Unix epoch converted to a civil UTC date.
    let days = (now / 86_400) as i64;
    let seconds = now % 86_400;
    let (year, month, day) = civil_from_days(days);
    let date = (((year - 1980).clamp(0, 127) as u16) << 9)
        | ((month as u16) << 5)
        | day as u16;
    let time = ((seconds / 3600) as u16) << 11
        | (((seconds % 3600) / 60) as u16) << 5
        | ((seconds % 60) / 2) as u16;
    (date, time)
}

/// Howard Hinnant's `civil_from_days`.
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { year + 1 } else { year };
    (year, month, day)
}

fn entropy_bytes(len: usize) -> Vec<u8> {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hash, Hasher};
    let mut output = Vec::with_capacity(len);
    let mut counter: u64 = 0;
    while output.len() < len {
        let state = RandomState::new();
        let mut hasher = state.build_hasher();
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0)
            .hash(&mut hasher);
        std::process::id().hash(&mut hasher);
        counter.hash(&mut hasher);
        output.extend_from_slice(&hasher.finish().to_le_bytes());
        counter += 1;
    }
    output.truncate(len);
    output
}

fn aes_extra_field(actual_method: u16) -> Vec<u8> {
    let mut extra = Vec::with_capacity(11);
    extra.extend_from_slice(&AES_EXTRA_ID.to_le_bytes());
    extra.extend_from_slice(&7u16.to_le_bytes());
    extra.extend_from_slice(&2u16.to_le_bytes()); // AE-2
    extra.extend_from_slice(b"AE");
    extra.push(3); // AES-256
    extra.extend_from_slice(&actual_method.to_le_bytes());
    extra
}

fn write_u16(output: &mut Vec<u8>, value: u16) {
    output.extend_from_slice(&value.to_le_bytes());
}

fn write_u32(output: &mut Vec<u8>, value: u32) {
    output.extend_from_slice(&value.to_le_bytes());
}

/// Writes a stored, AES-256 encrypted archive with the given members.
pub fn write_archive(entries: &[(&str, &[u8])], password: &[u8]) -> Result<Vec<u8>> {
    let (date, time) = dos_date_time();
    let mut local_headers: Vec<Vec<u8>> = Vec::new();
    let mut bodies: Vec<Vec<u8>> = Vec::new();
    for (name, data) in entries {
        let salt = entropy_bytes(16);
        let (key, mac_key, verification) = derive_key(password, &salt, 32);
        let mut key_array = [0u8; 32];
        key_array.copy_from_slice(&key);
        let ciphertext = aes256_ctr(&key_array, data);
        let mac = super::crypto::hmac_sha1_truncated(&mac_key, &ciphertext, 10);
        let mut body = Vec::with_capacity(16 + 2 + ciphertext.len() + 10);
        body.extend_from_slice(&salt);
        body.extend_from_slice(&verification);
        body.extend_from_slice(&ciphertext);
        body.extend_from_slice(&mac);
        let extra = aes_extra_field(METHOD_STORED);
        let name_bytes = name.as_bytes();
        let mut header = Vec::new();
        write_u32(&mut header, LOCAL_HEADER_SIGNATURE);
        write_u16(&mut header, 20);
        write_u16(&mut header, 0x0001);
        write_u16(&mut header, METHOD_AES);
        write_u16(&mut header, time);
        write_u16(&mut header, date);
        write_u32(&mut header, 0); // AE-2 keeps the CRC field at zero
        write_u32(&mut header, body.len() as u32);
        write_u32(&mut header, data.len() as u32);
        write_u16(&mut header, name_bytes.len() as u16);
        write_u16(&mut header, extra.len() as u16);
        header.extend_from_slice(name_bytes);
        header.extend_from_slice(&extra);
        local_headers.push(header);
        bodies.push(body);
    }
    let mut output = Vec::new();
    let mut offsets = Vec::with_capacity(entries.len());
    for (header, body) in local_headers.iter().zip(bodies.iter()) {
        offsets.push(output.len() as u32);
        output.extend_from_slice(header);
        output.extend_from_slice(body);
    }
    let central_offset = output.len() as u32;
    for (index, (name, data)) in entries.iter().enumerate() {
        let extra = aes_extra_field(METHOD_STORED);
        let name_bytes = name.as_bytes();
        let body_len = bodies[index].len() as u32;
        write_u32(&mut output, CENTRAL_HEADER_SIGNATURE);
        write_u16(&mut output, 20); // version made by (MS-DOS, as zipctl writes)
        write_u16(&mut output, 20);
        write_u16(&mut output, 0x0001);
        write_u16(&mut output, METHOD_AES);
        write_u16(&mut output, time);
        write_u16(&mut output, date);
        write_u32(&mut output, 0);
        write_u32(&mut output, body_len);
        write_u32(&mut output, data.len() as u32);
        write_u16(&mut output, name_bytes.len() as u16);
        write_u16(&mut output, extra.len() as u16);
        write_u16(&mut output, 0); // comment length
        write_u16(&mut output, 0); // disk number start
        write_u16(&mut output, 0); // internal attributes
        write_u32(&mut output, 0o600 << 16); // external attributes (zipctl)
        write_u32(&mut output, offsets[index]);
        output.extend_from_slice(name_bytes);
        output.extend_from_slice(&extra);
    }
    let central_size = output.len() as u32 - central_offset;
    write_u32(&mut output, EOCD_SIGNATURE);
    write_u16(&mut output, 0);
    write_u16(&mut output, 0);
    write_u16(&mut output, entries.len() as u16);
    write_u16(&mut output, entries.len() as u16);
    write_u32(&mut output, central_size);
    write_u32(&mut output, central_offset);
    write_u16(&mut output, 0);
    Ok(output)
}

/// `hashlib.md5(content).hexdigest()` used by the generator.
pub fn content_md5(data: &[u8]) -> String {
    md5_hex(data)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aes_archive_roundtrip() {
        let entries: [(&str, &[u8]); 2] = [
            ("config.json", b"{\"aceFileName\":\"ps_structure.ps\"}"),
            ("audio.bin", &[0u8, 1, 2, 3, 4, 5]),
        ];
        let bytes = write_archive(&entries, b"a022ab39cb3b7b1de92ee441978c9e08").unwrap();
        let read = read_archive(&bytes, b"a022ab39cb3b7b1de92ee441978c9e08").unwrap();
        assert_eq!(read.len(), 2);
        assert_eq!(read[0].name, "config.json");
        assert_eq!(read[0].data, entries[0].1);
        assert_eq!(read[1].name, "audio.bin");
        assert_eq!(read[1].data, entries[1].1);
        assert!(read_archive(&bytes, b"wrong-password").is_err());
    }
}
