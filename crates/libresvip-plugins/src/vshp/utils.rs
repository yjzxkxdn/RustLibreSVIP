//! Port of `libresvip.plugins.vshp.utils`.

/// `ansi2unicode` — try a list of legacy encodings, then fall back to a lossy
/// UTF-8 decode.
pub fn ansi2unicode(content: &[u8]) -> String {
    for encoding in [
        encoding_rs::UTF_8,
        encoding_rs::GBK,
        encoding_rs::SHIFT_JIS,
        encoding_rs::EUC_KR,
        encoding_rs::BIG5,
    ] {
        let (decoded, _, had_errors) = encoding.decode(content);
        if !had_errors {
            return decoded.into_owned();
        }
    }
    String::from_utf8_lossy(content).into_owned()
}

/// The bytes before the first NUL terminator.
pub fn until_nul(content: &[u8]) -> &[u8] {
    match content.iter().position(|byte| *byte == 0) {
        Some(index) => &content[..index],
        None => content,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_utf8_and_gbk() {
        assert_eq!(ansi2unicode("测试".as_bytes()), "测试");
        // 0xB2 0xE2 0xCA 0xD4 is "测试" in GBK.
        assert_eq!(ansi2unicode(&[0xB2, 0xE2, 0xCA, 0xD4]), "测试");
    }

    #[test]
    fn keeps_invalid_bytes_lossily() {
        // Not valid UTF-8 nor any of the legacy encodings on its own.
        let decoded = ansi2unicode(&[0xFF, 0xFE, 0xFF]);
        assert!(!decoded.is_empty());
    }

    #[test]
    fn splits_at_nul() {
        assert_eq!(until_nul(b"abc\0def"), b"abc");
        assert_eq!(until_nul(b"abc"), b"abc");
        assert_eq!(until_nul(b"\0abc"), b"");
    }
}
