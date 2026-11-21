//! Turning untrusted names into paths that stay inside a folder.
//!
//! File names come from NZBs, yEnc headers, torrent metadata and archive entries, all of which a
//! poster controls. Nothing they say may write outside the folder Fangst chose.

use std::path::{Path, PathBuf};

/// Names Windows reserves in every folder, with or without an extension.
const RESERVED: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// Reduces an untrusted name to one safe file name component, or `None` if nothing usable is left.
///
/// Directory parts are dropped (only the last component is kept), characters that are invalid on
/// Windows or are control characters become `_`, and trailing dots and spaces are trimmed because
/// Windows strips them silently, which could make two names collide.
#[must_use]
pub fn safe_file_name(untrusted: &str) -> Option<String> {
    let last = untrusted.rsplit(['/', '\\']).next().unwrap_or(untrusted);
    let cleaned: String = last
        .chars()
        .map(|c| {
            if c.is_control() || matches!(c, '<' | '>' | ':' | '"' | '|' | '?' | '*') {
                '_'
            } else {
                c
            }
        })
        .collect();
    let cleaned = cleaned.trim().trim_end_matches(['.', ' ']).to_owned();
    if cleaned.is_empty() || cleaned == "." || cleaned == ".." {
        return None;
    }
    let stem = cleaned.split('.').next().unwrap_or_default();
    if RESERVED.iter().any(|r| r.eq_ignore_ascii_case(stem)) {
        return Some(format!("_{cleaned}"));
    }
    // Long names fail on most file systems; 200 bytes leaves room for suffixes like ".1".
    if cleaned.len() > 200 {
        let mut end = 200;
        while !cleaned.is_char_boundary(end) {
            end -= 1;
        }
        return Some(cleaned[..end].to_owned());
    }
    Some(cleaned)
}

/// `root` joined with a safe version of `untrusted`, falling back to `fallback` when nothing usable is left.
#[must_use]
pub fn file_inside(root: &Path, untrusted: &str, fallback: &str) -> PathBuf {
    root.join(safe_file_name(untrusted).unwrap_or_else(|| fallback.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_ordinary_names() {
        assert_eq!(
            safe_file_name("Some.Show.S01E01.1080p.mkv").as_deref(),
            Some("Some.Show.S01E01.1080p.mkv")
        );
        assert_eq!(
            safe_file_name("名前 (1).rar").as_deref(),
            Some("名前 (1).rar")
        );
    }

    #[test]
    fn drops_directory_parts() {
        assert_eq!(
            safe_file_name("../../etc/passwd").as_deref(),
            Some("passwd")
        );
        assert_eq!(
            safe_file_name("..\\..\\Windows\\win.ini").as_deref(),
            Some("win.ini")
        );
        assert_eq!(safe_file_name("/abs/path.bin").as_deref(), Some("path.bin"));
        assert_eq!(safe_file_name("C:\\x\\y.bin").as_deref(), Some("y.bin"));
    }

    #[test]
    fn rejects_names_with_nothing_left() {
        for bad in ["", "..", ".", "dir/", "   ", "...", "a/.."] {
            assert_eq!(safe_file_name(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn replaces_characters_windows_rejects() {
        assert_eq!(
            safe_file_name("a:b*c?d\"e<f>g|h").as_deref(),
            Some("a_b_c_d_e_f_g_h")
        );
        assert_eq!(
            safe_file_name("tab\there\u{0}").as_deref(),
            Some("tab_here_")
        );
    }

    #[test]
    fn trims_trailing_dots_and_spaces() {
        assert_eq!(safe_file_name("name. . ").as_deref(), Some("name"));
    }

    #[test]
    fn prefixes_reserved_windows_names() {
        assert_eq!(safe_file_name("CON").as_deref(), Some("_CON"));
        assert_eq!(safe_file_name("nul.txt").as_deref(), Some("_nul.txt"));
        assert_eq!(
            safe_file_name("console.txt").as_deref(),
            Some("console.txt")
        );
    }

    #[test]
    fn truncates_long_names_on_a_char_boundary() {
        let long = "é".repeat(150);
        let safe = safe_file_name(&long).unwrap();
        assert!(safe.len() <= 200);
        assert!(safe.chars().all(|c| c == 'é'));
    }

    #[test]
    fn file_inside_never_leaves_the_root() {
        let root = Path::new("downloads");
        assert_eq!(file_inside(root, "../../x", "f"), root.join("x"));
        assert_eq!(
            file_inside(root, "..", "fallback.bin"),
            root.join("fallback.bin")
        );
    }
}
