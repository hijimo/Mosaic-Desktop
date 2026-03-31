//! Text encoding detection and conversion utilities for shell output.
//!
//! Automatically detects legacy encodings (CP1251, CP866, Shift-JIS, GBK, etc.)
//! and decodes them to UTF-8. Falls back to lossy UTF-8 for unrecognizable bytes.

use chardetng::EncodingDetector;
use encoding_rs::{Encoding, IBM866, WINDOWS_1252};

/// Convert arbitrary bytes to UTF-8 with best-effort encoding detection.
pub fn bytes_to_string_smart(bytes: &[u8]) -> String {
    if bytes.is_empty() {
        return String::new();
    }
    if let Ok(s) = std::str::from_utf8(bytes) {
        return s.to_owned();
    }

    let encoding = detect_encoding(bytes);
    let (decoded, _, had_errors) = encoding.decode(bytes);
    if had_errors {
        String::from_utf8_lossy(bytes).into_owned()
    } else {
        decoded.into_owned()
    }
}

/// Windows-1252 byte values for smart punctuation that collide with IBM866 Cyrillic.
const WIN1252_PUNCT: [u8; 8] = [0x91, 0x92, 0x93, 0x94, 0x95, 0x96, 0x97, 0x99];

fn detect_encoding(bytes: &[u8]) -> &'static Encoding {
    let mut detector = EncodingDetector::new();
    detector.feed(bytes, true);
    let (encoding, _) = detector.guess_assess(None, true);

    // Coerce IBM866 → Windows-1252 when the high bytes are exclusively smart-punctuation
    // values mixed with ASCII. This prevents curly quotes from rendering as Cyrillic.
    if encoding == IBM866 && looks_like_win1252_punct(bytes) {
        return WINDOWS_1252;
    }
    encoding
}

fn looks_like_win1252_punct(bytes: &[u8]) -> bool {
    let mut saw_punct = false;
    let mut saw_ascii = false;
    for &b in bytes {
        if b >= 0xA0 {
            return false;
        }
        if (0x80..=0x9F).contains(&b) {
            if !WIN1252_PUNCT.contains(&b) {
                return false;
            }
            saw_punct = true;
        }
        if b.is_ascii_alphabetic() {
            saw_ascii = true;
        }
    }
    saw_punct && saw_ascii
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf8_passthrough() {
        assert_eq!(
            bytes_to_string_smart("Hello, 世界".as_bytes()),
            "Hello, 世界"
        );
    }

    #[test]
    fn cp1251_russian() {
        let bytes = b"\xCF\xF0\xE8\xE2\xE5\xF2"; // "Привет" in CP1251
        assert_eq!(bytes_to_string_smart(bytes), "Привет");
    }

    #[test]
    fn cp866_russian() {
        let bytes = b"\xAF\xE0\xA8\xAC\xA5\xE0"; // "пример" in CP866
        assert_eq!(bytes_to_string_smart(bytes), "пример");
    }

    #[test]
    fn win1252_smart_quotes() {
        let bytes = b"\x93\x94test"; // left/right double quotes + ASCII
        assert_eq!(bytes_to_string_smart(bytes), "\u{201C}\u{201D}test");
    }

    #[test]
    fn latin1_cafe() {
        assert_eq!(bytes_to_string_smart(b"caf\xE9"), "café");
    }

    #[test]
    fn preserves_ansi_sequences() {
        let bytes = b"\x1b[31mred\x1b[0m";
        assert_eq!(bytes_to_string_smart(bytes), "\x1b[31mred\x1b[0m");
    }

    #[test]
    fn fallback_to_lossy() {
        let bad = [0xFF, 0xFE, 0xFD];
        assert_eq!(bytes_to_string_smart(&bad), String::from_utf8_lossy(&bad));
    }

    #[test]
    fn empty_input() {
        assert_eq!(bytes_to_string_smart(b""), "");
    }
}
