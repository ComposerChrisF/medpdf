// tests/no_raw_stream_content_assignment.rs
//
// Guard test: forbid raw `<expr>.content = <bytes>` assignments in `src/`.
//
// lopdf exposes `Stream::content` as a public `Vec<u8>` field, so assigning to
// it directly compiles but bypasses `Stream::set_content` — the setter that
// keeps the dictionary's `/Length` in sync with the body. A stale `/Length`
// makes lopdf drop the stream body on reload (silent data loss); that exact bug
// lived in `pdf_overlay_helpers::modify_content_stream`.
//
// clippy's `disallowed-methods` can't catch public *field* access, so this
// source grep is the practical enforcement. Replace any flagged assignment with
// `stream.set_content(bytes)`.

use std::fs;
use std::path::{Path, PathBuf};

/// Returns the byte index of the first `//` line-comment marker, if any.
fn line_comment_start(line: &str) -> Option<usize> {
    line.find("//")
}

/// True if `code` contains a `.content` field assignment (`.content =`, but not
/// `.content ==` comparison). Operates on a comment-stripped line.
fn has_content_assignment(code: &str) -> bool {
    let bytes = code.as_bytes();
    let mut search_from = 0;
    while let Some(rel) = code[search_from..].find(".content") {
        let after = search_from + rel + ".content".len();
        // Skip whitespace following `.content`.
        let mut i = after;
        while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
            i += 1;
        }
        // An assignment is a single `=` not immediately followed by another `=`.
        if i < bytes.len() && bytes[i] == b'=' && !(i + 1 < bytes.len() && bytes[i + 1] == b'=') {
            return true;
        }
        search_from = after;
    }
    false
}

/// Recursively collects every `.rs` file under `dir`.
fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).expect("read src dir") {
        let path = entry.expect("dir entry").path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

#[test]
fn no_raw_stream_content_field_assignment_in_src() {
    let src_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    rust_files(&src_dir, &mut files);
    assert!(
        !files.is_empty(),
        "expected to find source files under src/"
    );

    let mut violations = Vec::new();
    for file in &files {
        let contents = fs::read_to_string(file).expect("read source file");
        for (lineno, line) in contents.lines().enumerate() {
            let code = match line_comment_start(line) {
                Some(idx) => &line[..idx],
                None => line,
            };
            if has_content_assignment(code) {
                violations.push(format!(
                    "{}:{}: {}",
                    file.display(),
                    lineno + 1,
                    line.trim()
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "Raw `.content =` stream-body assignment found — use `Stream::set_content` to keep \
         /Length in sync (see modify_content_stream):\n{}",
        violations.join("\n")
    );
}

// --- Self-tests for the matcher heuristic ---

#[test]
fn matcher_flags_assignment_but_not_reads_or_comparisons() {
    assert!(has_content_assignment("stream.content = bytes;"));
    assert!(has_content_assignment("foo.content=encode();"));
    assert!(!has_content_assignment("if a.content == b.content {"));
    assert!(!has_content_assignment("let n = stream.content.len();"));
    assert!(!has_content_assignment("stream.set_content(bytes);"));
}
