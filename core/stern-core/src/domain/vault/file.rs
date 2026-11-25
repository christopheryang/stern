// Copyright (c) 2026 Christopher Yang
// SPDX-License-Identifier: MIT

//! Stored files: the size policy and the metadata that travels with an
//! encrypted blob. Pure domain logic; the bytes themselves are encrypted
//! and stored by the infrastructure layer.

/// The largest file Stern will store, in bytes (1 MiB, shown as "1 MB").
pub const MAX_FILE_BYTES: u64 = 1024 * 1024;

/// Whether a file of `len` bytes is within the limit.
#[must_use]
pub const fn within_limit(len: u64) -> bool {
    len <= MAX_FILE_BYTES
}

/// Extensions Stern will not take, even when the bytes happen to be valid
/// text: these are formats whose content is not readable text, and Stern
/// keeps plain text only.
const REFUSED_EXTENSIONS: &[&str] = &[
    "pdf", "doc", "docx", "xls", "xlsx", "ppt", "pptx", "odt", "ods", "odp", "rtf", "png", "jpg",
    "jpeg", "gif", "bmp", "tif", "tiff", "webp", "heic", "ico", "zip", "gz", "tar", "7z", "rar",
    "dmg", "exe", "dll", "bin", "mp3", "mp4", "mov", "wav", "avi", "mkv", "sqlite", "db",
];

/// Why a file was turned away.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    /// More than [`MAX_FILE_BYTES`].
    TooBig,
    /// Its extension names a format that is not plain text.
    NotPlainText,
    /// Not valid UTF-8, or it holds a NUL byte.
    NotUtf8,
    /// Looks like UTF-16 text (a byte-order mark): readable, but not in a
    /// form Stern takes.
    Utf16,
}

impl Refusal {
    /// The plain sentence shown to the user.
    #[must_use]
    pub const fn message(self) -> &'static str {
        match self {
            Self::TooBig => "That file is too big. The limit is 1 MB.",
            Self::NotPlainText => {
                "Stern keeps plain text files only, like .txt, .md, .html, .csv or .json."
            }
            Self::NotUtf8 => {
                "That file isn't plain text Stern can read. Save it as a UTF-8 text file and try again."
            }
            Self::Utf16 => {
                "That text file is in an older format (UTF-16). Save it as a UTF-8 text file and try again."
            }
        }
    }
}

/// A file's extension, lowercased, if it has one.
fn extension(name: &str) -> Option<String> {
    name.rsplit_once('.')
        .map(|(_, ext)| ext.to_ascii_lowercase())
        .filter(|ext| !ext.is_empty())
}

/// Refuse a file by its name alone, before any of it is read.
pub fn check_name(name: &str) -> Result<(), Refusal> {
    match extension(name) {
        Some(ext) if REFUSED_EXTENSIONS.contains(&ext.as_str()) => Err(Refusal::NotPlainText),
        _ => Ok(()),
    }
}

const UTF8_BOM: &[u8] = &[0xEF, 0xBB, 0xBF];

/// Check the bytes of a file and return its text, without a byte-order
/// mark. Valid UTF-8 with no NUL byte is the only thing accepted; the
/// content decides, never the extension.
pub fn check_text(bytes: &[u8]) -> Result<&str, Refusal> {
    if !within_limit(u64::try_from(bytes.len()).unwrap_or(u64::MAX)) {
        return Err(Refusal::TooBig);
    }
    if bytes.starts_with(&[0xFF, 0xFE]) || bytes.starts_with(&[0xFE, 0xFF]) {
        return Err(Refusal::Utf16);
    }
    if bytes.contains(&0) {
        return Err(Refusal::NotUtf8);
    }
    let body = bytes.strip_prefix(UTF8_BOM).unwrap_or(bytes);
    std::str::from_utf8(body).map_err(|_| Refusal::NotUtf8)
}

/// How a file's bytes are stored and labelled.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileInfo {
    /// The type label shown to the user (e.g. `text/plain`).
    pub mime_type: String,
    /// Whether the bytes are valid UTF-8 text.
    pub is_text: bool,
}

/// Label `bytes` for display. New files are always text (see
/// [`check_text`]); this also labels entries stored as binary by earlier
/// versions. The content decides, never the file extension.
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
    fn limit_is_inclusive_at_one_mebibyte() {
        assert_eq!(MAX_FILE_BYTES, 1_048_576);
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
    fn binary_is_labelled_from_its_extension() {
        assert_eq!(
            classify("scan.PDF", &[0x25, 0x50, 0xff]).mime_type,
            "application/pdf"
        );
    }

    #[test]
    fn text_is_accepted_and_a_bom_is_removed() {
        assert_eq!(check_text("héllo".as_bytes()), Ok("héllo"));
        let mut with_bom = vec![0xEF, 0xBB, 0xBF];
        with_bom.extend_from_slice(b"hello");
        assert_eq!(check_text(&with_bom), Ok("hello"));
        assert_eq!(check_text(b""), Ok(""));
    }

    #[test]
    fn a_nul_byte_or_bad_utf8_is_refused() {
        assert_eq!(check_text(b"abc\0def"), Err(Refusal::NotUtf8));
        assert_eq!(check_text(&[0x41, 0xC3, 0x28]), Err(Refusal::NotUtf8));
    }

    #[test]
    fn utf16_is_named_as_such() {
        assert_eq!(check_text(&[0xFF, 0xFE, b'h', 0]), Err(Refusal::Utf16));
        assert_eq!(check_text(&[0xFE, 0xFF, 0, b'h']), Err(Refusal::Utf16));
    }

    #[test]
    fn too_much_text_is_refused() {
        let big = vec![b'a'; usize::try_from(MAX_FILE_BYTES).unwrap() + 1];
        assert_eq!(check_text(&big), Err(Refusal::TooBig));
        let exact = vec![b'a'; usize::try_from(MAX_FILE_BYTES).unwrap()];
        assert!(check_text(&exact).is_ok());
    }

    #[test]
    fn known_binary_extensions_are_refused_by_name() {
        for name in [
            "a.pdf",
            "A.DOCX",
            "scan.Png",
            "x.zip",
            "data.xlsx",
            "app.exe",
        ] {
            assert_eq!(check_name(name), Err(Refusal::NotPlainText), "{name}");
        }
        for name in [
            "a.txt",
            "page.html",
            "t.csv",
            "n.md",
            "noext",
            "log.",
            ".env",
            "x.json",
        ] {
            assert!(check_name(name).is_ok(), "{name}");
        }
    }

    #[test]
    fn every_refusal_has_a_plain_sentence() {
        for r in [
            Refusal::TooBig,
            Refusal::NotPlainText,
            Refusal::NotUtf8,
            Refusal::Utf16,
        ] {
            assert!(!r.message().is_empty());
            assert!(!r.message().contains("error"));
        }
    }
}
