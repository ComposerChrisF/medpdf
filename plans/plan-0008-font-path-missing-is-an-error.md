# Plan: A Font Specifier That Is Plainly a Path Must Exist

## Problem

`find_font` and `find_font_with_style` take one specifier that is either a file path or a family name, and they decide which by asking the filesystem: a path that is `NotFound` is treated as a family name, and its file stem is looked up among the installed fonts.  So a caller who names a file that is not there gets the installed family of the same name instead, with no error:

```
find_font("/Volumes/FontDisk/Arial.ttf")   # FontDisk unmounted, or the path mistyped
→ Ok(FontPath::Memory(_, "ArialMT"))       # the system Arial, silently
```

This is the silent substitution bug-0043 fixed, reached the other way.  bug-0043 closed the case of a path that _cannot be stat’ed_ (a permission error, a link to nothing), and kept the `NotFound` fallback on purpose, because the specifier grammar depends on it.  An unmounted volume is the canonical trigger (`positive-evidence-of-absence.md`: “It is not exotic; it is Tuesday”), and a custom or licensed font is exactly the file most likely to live on one.  The output is a valid PDF in the wrong font.

Found 15-Nov-2026 by the review of the bug-0043 fix.

## Proposed Change

Make the specifier grammar say which kind it is, so that a path the caller asserted is never reinterpreted:

- A specifier that **looks like a path** — it contains a path separator, or ends in a font extension (`.ttf`, `.otf`, `.ttc`, `.otc`) — is a caller-asserted file.  `NotFound` is an error naming the path, as the cli-exit-codes rule requires of a caller-asserted input (“missing is exit 1 … Never silent success”).
- Anything else (`Arial`, `Times New Roman`) is a family name, looked up as today, and is not stat’ed in the working directory at all.  That also removes the bug-0043 corner where a bare family name fails because the working directory cannot be searched.

Alternatively, an explicit form (`family:Arial`, `file:/path`) beside the inferred one, if inference proves too clever.

## Implementation Notes

- The change is in `resolve_non_system_font` (`medpdf/src/pdf_font.rs`) and its two callers.
- It is a contract change for both consumers.  pdf-maker and pdf-orchestrator pass user-supplied specifiers through: audit what their users write (config files, `--font` values, tests) for a family name that contains a `/` or an extension, or a relative path with neither, before landing.
- `FontPath` is unchanged.

## Why Not a Workaround

The decision of what a specifier means belongs to the one function that parses it; a consumer that pre-checks the path would be duplicating the grammar, and would still race the lookup.
