# Feature Plan (pointer): Unicode Text Beyond WinAnsi

The full plan lives at `../pdf-maker/feature-plan-unicode-text.md` — it was filed against pdf-maker before the root cause was localized here.  **medpdf is the fix locus**: the single-byte WinAnsi text/embedding path lives in this crate, and both known consumers corrupt any non-CP1252 character (Hawaiian ‘okina U+02BB, kahakō ā ē ī ō ū, and everything else outside the codepage) to a silent `?` while reporting success.

Verified consumers affected (2026-07-10, empirically — same failure signature, `WinAnsiEncoding, CharRange: 32-255`):

- `pdf-maker` `--watermark`
- `pdf-orchestrator` `<AddText>` (which makes the production `.pdfOrch` pipeline latently broken for Hawaiian overlay text)

Shape of the fix (details in the full plan): Type0/CIDFontType2 embedding with Identity-H + ToUnicode CMap when text leaves CP1252; keep the single-byte fast path otherwise; built-in Standard-14 fonts fail loudly on unrepresentable text instead of substituting `?`.

Evidence and decision context: `~/Chris/App/Claude/okina-codepoint-verification.md`.
