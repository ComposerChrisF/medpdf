# Bug: `find_font` treats a font file it cannot stat as a family name, and may embed a different system font in its place

**Severity:** Low — the output is a valid PDF in the wrong font, with no error.  It needs a font path the process cannot stat (a directory without search permission, a symlink loop, an I/O error) whose file stem names an installed font family.

**Type:** Code bug in this repo (`medpdf/src/pdf_font.rs`).

**Found:** 15-Nov-2026, while adopting the restriction lints (`disallowed_methods` on `Path::exists`); confirmed by a scratch program against the working tree.

## What Happens

`resolve_non_system_font` (`medpdf/src/pdf_font.rs:100–102`) decides whether the specifier is a file with `font_path.exists()`.  `exists()` answers `false` for every error, not only `NotFound`, so a file the process cannot stat is classed as “not a file”.  `find_font` and `find_font_with_style` then take the file stem as a family name and ask the system font source for it.  When a family of that name is installed, the call succeeds with that font.

Reproduction: copy `Arial.ttf` into a directory, `chmod 000` the directory, and call `find_font(<dir>/Arial.ttf)`.  It returns `Ok(FontPath::Memory(_, "ArialMT"))`, the system Arial, where reading the named file would have failed with `EACCES`.  Any file whose stem matches an installed family behaves the same, so a custom or licensed `Helvetica.ttf` in an unreadable directory silently becomes the system Helvetica.  When no family matches, the error names a missing font rather than the permission failure.

## The Rule It Breaks

A probe must not read “could not look” as “absent”.  The caller named a file; when the library cannot tell whether that file is there, the answer is unknown, and an unknown must surface as an error, never as a silent substitute.

## Fix Direction

Replace `exists()` with a probe that keeps errors: `fs::metadata(path)`, where `Ok` is a file, `NotFound` (with `fs::symlink_metadata` also `NotFound`) falls through to the family-name lookup as today, and any other error returns `MedpdfError` naming the path and the I/O error.  The family-name fallback for a path that is genuinely absent is unchanged.

## The Test That Pins It

A Unix test that puts a font file in a temp directory, removes the directory’s permissions, calls `find_font` on the file, and asserts that **the call returns an error and does not return a system font**; it restores the permissions before cleanup.  It must fail with the fix reverted (today it returns `Ok`).  The stem should name a family the test machine has, such as `Arial` on macOS, or the test skips when none is found.
