//! Minimal stored-only ZIP writer mirroring CPython's `zipfile` headers.
//!
//! `zipfile.ZipFile(buffer, "w")` + `writestr(name, data)` produces a
//! ZIP_STORED archive whose local/central headers carry `time.localtime()`.
//! That makes two Python dumps differ across a 2-second boundary, which is why
//! the golden fixture is marked `nondeterministic` (the archive bytes are not
//! compared). The DOS timestamp below is the one embedded in the checked-in
//! fixture (`testdata/golden/vpr/dump.vpr`), the same approach `vog` and `vfp`
//! use for their archives.

/// DOS date of the reference fixture: 2026-10-23.
pub const DOS_DATE: u16 = 0x5D57;
/// DOS time of the reference fixture: 01:37:52 (2 second granularity).
pub const DOS_TIME: u16 = 0x0CBA;

pub struct ZipEntry<'a> {
    pub name: &'a str,
    pub data: &'a [u8],
    pub external_attr: u32,
    pub dos_date: u16,
    pub dos_time: u16,
}

pub fn write_zip(entries: &[ZipEntry<'_>]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut central_offsets: Vec<usize> = Vec::with_capacity(entries.len());
    for entry in entries {
        let offset = out.len();
        let crc = crc32fast::hash(entry.data);
        let size = entry.data.len() as u32;
        out.extend_from_slice(&0x0403_4B50u32.to_le_bytes());
        out.extend_from_slice(&20u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&entry.dos_time.to_le_bytes());
        out.extend_from_slice(&entry.dos_date.to_le_bytes());
        out.extend_from_slice(&crc.to_le_bytes());
        out.extend_from_slice(&size.to_le_bytes());
        out.extend_from_slice(&size.to_le_bytes());
        out.extend_from_slice(&(entry.name.len() as u16).to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(entry.name.as_bytes());
        out.extend_from_slice(entry.data);
        central_offsets.push(offset);
    }

    let central_start = out.len();
    for (entry, offset) in entries.iter().zip(central_offsets.iter()) {
        let crc = crc32fast::hash(entry.data);
        let size = entry.data.len() as u32;
        out.extend_from_slice(&0x0201_4B50u32.to_le_bytes());
        out.extend_from_slice(&20u16.to_le_bytes());
        out.extend_from_slice(&20u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&entry.dos_time.to_le_bytes());
        out.extend_from_slice(&entry.dos_date.to_le_bytes());
        out.extend_from_slice(&crc.to_le_bytes());
        out.extend_from_slice(&size.to_le_bytes());
        out.extend_from_slice(&size.to_le_bytes());
        out.extend_from_slice(&(entry.name.len() as u16).to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&entry.external_attr.to_le_bytes());
        out.extend_from_slice(&(*offset as u32).to_le_bytes());
        out.extend_from_slice(entry.name.as_bytes());
    }
    let central_size = out.len() - central_start;

    out.extend_from_slice(&0x0605_4B50u32.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
    out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
    out.extend_from_slice(&(central_size as u32).to_le_bytes());
    out.extend_from_slice(&(central_start as u32).to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out
}
