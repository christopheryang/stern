// Copyright (c) 2026 Christopher Yang
// SPDX-License-Identifier: MIT

//! Stored files: the size policy and the metadata that travels with an
//! encrypted blob. Pure domain logic; the bytes themselves are encrypted
//! and stored by the infrastructure layer.

/// The largest file Stern will store, in bytes (10 MB).
pub const MAX_FILE_BYTES: u64 = 10 * 1024 * 1024;

/// Whether a file of `len` bytes is within the limit.
#[must_use]
pub const fn within_limit(len: u64) -> bool {
    len <= MAX_FILE_BYTES
}

/// How a file's bytes are stored and labelled.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileInfo {
    /// The type label shown to the user (e.g. `text/plain`).
    pub mime_type: String,
    /// Whether the bytes are valid UTF-8 text.
    pub is_text: bool,
}

/// Decide how to store `bytes`. The content decides, never the file
/// extension: valid UTF-8 without a NUL byte is text; anything else is
/// binary.
#[must_use]
pub fn classify(name: &str, bytes: &[u8]) -> FileInfo {
    let is_text = !bytes.contains(&0) && std::str::from_utf8(bytes).is_ok();
    let mime_type = if is_text {
        "text/plain".to_string()
    } else {
        mime_from_extension(name).to_string()
    };
    FileInfo { mime_type, is_text }
}

/// A type label from the file name, for display only. Unknown or missing
/// extensions are `application/octet-stream`.
fn mime_from_extension(name: &str) -> &'static str {
    let ext = name
        .rsplit_once('.')
        .map(|(_, ext)| ext.to_ascii_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "pdf" => "application/pdf",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "csv" => "text/csv",
        "zip" => "application/zip",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limit_is_inclusive_at_ten_megabytes() {
        assert!(within_limit(MAX_FILE_BYTES));
        assert!(!within_limit(MAX_FILE_BYTES + 1));
    }

    #[test]
    fn utf8_text_is_text_whatever_the_extension() {
        let info = classify("notes.bin", "héllo wörld".as_bytes());
        assert!(info.is_text);
        assert_eq!(info.mime_type, "text/plain");
    }

    #[test]
    fn invalid_utf8_is_binary_even_with_a_txt_name() {
        let info = classify("notes.txt", &[0xff, 0xfe, 0x00, 0x41]);
        assert!(!info.is_text);
    }

    #[test]
    fn a_nul_byte_makes_text_binary() {
        assert!(!classify("a.txt", b"abc\0def").is_text);
    }

    #[test]
    fn binary_is_labelled_from_its_extension() {
        assert_eq!(
            classify("scan.PDF", &[0x25, 0x50, 0xff]).mime_type,
            "application/pdf"
        );
        assert_eq!(
            classify("photo", &[0xff, 0xd8]).mime_type,
            "application/octet-stream"
        );
    }
}
