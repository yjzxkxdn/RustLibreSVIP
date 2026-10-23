//! Minimal stored-only ZIP writer that mirrors CPython's `zipfile` headers.
//!
//! The Rust `zip` crate writes a different "version made by" value and attribute
//! combination than `zipfile.writestr`, and the Vogen golden fixture compares
//! the archive bytes, so the container is assembled by hand.

/// DOS date/time used for regular entries (see `vog::model::write_archive`).
pub const DOS_DATE: u16 = 0x5D57; // 2026-10-23
pub const DOS_TIME: u16 = 0x0CBA; // 01:37:52

/// `zipfile.ZipInfo` defaults used by `ZipFile.mkdir` (1980-01-01 00:00:00).
pub const DOS_DATE_DEFAULT: u16 = 0x0021;

pub struct ZipEntry<'a> {
    pub name: &'a str,
    pub data: &'a [u8],
    pub external_attr: u32,
    pub dos_date: u16,
    pub dos_time: u16,
}

pub fn write_zip(entries: &[ZipEntry<'_>]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut central_entries: Vec<(usize, usize)> = Vec::new();
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
        central_entries.push((offset, entry.name.len()));
    }

    let central_start = out.len();
    for (entry, (offset, _)) in entries.iter().zip(central_entries.iter()) {
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
