# Feature Plan: Multi-line watermark text in `add_text_params`

**Status:** Analysis / open decision (2026-07-23).  Prompted by bug-0032, where Chris
asked: _would supporting multi-line text require a medpdf API change?_  This document
answers that and proposes where the fix should live.  **No code has been written for
this feature.**

## Problem

`medpdf::add_text_params` renders exactly **one line** of text.  It does not interpret
`\n`, `\t`, or any other control character; there is no line-splitting anywhere in the
watermark path.  A control character therefore:

- **WinAnsi (simple) path** — is emitted as a raw byte in the literal string.  Bytes
  `0x00`–`0x1F` / `0x7F` are undefined positions in WinAnsiEncoding, so the character
  renders as **nothing** (silently dropped, no line break).
- **Composite (Type0) path** — has no glyph, so `encode_text_identity` returns
  `UnrepresentableText` — a **hard error**, confusingly reported as “unrepresentable
  text” for whitespace.

As of bug-0032 (v0.11.14) `add_text_params` **warns** (`log::warn!`, naming each control
character) when the text contains any, so a debugging reader can find the cause.  It does
**not** yet reject or render them — that decision was deferred pending this analysis.

Downstream, **pdf-maker documents and supports `\n` and `\t` escapes** in `--watermark
text=…` (`spec_types/parse.rs::unescape_text`), unescapes them to literal control
characters, and passes the string straight to `add_text_params` with `lossy_text = false`
and **one call per page** — no line-splitting of its own.  So pdf-maker’s documented
`\n`/`\t` watermark escapes currently produce a single concatenated line, not multiple
lines: the escapes are effectively non-functional today, in both tools.

## The core question: does this need a medpdf API change?

**No — not for the basic case.**  It splits cleanly into three tiers.  Only the last two
touch the public API.

### Tier 1 — `\n`-split, metrics-based leading, block alignment.  NO API change.

Everything Tier 1 needs is already reachable inside `add_text_params`:

| Concern | How it is satisfied without a new parameter |
|---|---|
| Split into lines | Split `text` on `\n` internally. |
| Line spacing (leading) | Derive from the embedded face metrics already loaded in `compute_text_metrics`: `leading = (ascender − descender + line_gap) × font_size / upem`, or a `font_size × 1.2` fallback for built-in/Hack fonts.  A sensible default, not a caller input. |
| **Horizontal** alignment (`h_align`) | Already per-line-shaped: `measure_text_width_with_face` measures one string; call it per line and compute `dx` per line.  `left`/`center`/`right` apply to each line independently — **free**, reusing today’s code. |
| **Vertical** alignment (`v_align`) | Reinterpret from “this line” to “the whole block”: compute block height `H = (N−1)·leading + line_height`, then offset the first baseline so the block’s top / center / bottom lands at `y`.  For `N = 1`, block ≡ line, so **existing single-line behavior is unchanged** (backward-compatible). |
| Rotation | The existing `cm` sets a rotated frame at `(x, y)`; place line _i_ at `(dx_i, dy_block − i·leading)` inside that frame.  Rotation composes for free. |
| Decorations (underline/strikeout) | Emit the rect per line using each line’s width.  More work, but no new input. |

Tier 1 is **purely internal to `add_text_params`** — the signature, `AddTextParams`, and
every builder method stay identical.  It is backward-compatible (single-line text is a
1-line block).  This is the answer to Chris’s question: **basic multi-line does not
require an API change.**

### Tier 2 — caller-controlled leading.  API change (breaking for pdf-orchestrator).

If the caller wants to _set_ the line spacing (rather than accept the metrics default),
that is a new `AddTextParams` field (e.g. `line_spacing: Option<f32>`).  Per
`medpdf-downstream-consumers`, **adding any field to `AddTextParams` breaks
pdf-orchestrator**, which constructs it with an _exhaustive_ struct literal (no
`..Default::default()`).  pdf-maker uses the builder form and tolerates new fields.  So
Tier 2 is a real (if additive) API change with one consumer to update — batch it into a
semver bump.

### Tier 3 — word-wrap and truncation.  API change (breaking for pdf-orchestrator).

Wrapping and truncation need a **box width** and a **mode**, which are new inputs:

- `wrap_width: Option<f32>` — width to wrap/truncate against (in the same units as
  position).  Without it there is no wrap.
- `overflow: Wrap | Truncate | TruncateEllipsis | Clip` — what to do past the width /
  past a max height.
- Optional `max_lines` / `max_height` for truncation.

The wrapping algorithm (greedy break on spaces; medpdf owns the metrics, so this is the
right home for it) and truncation/ellipsis logic are self-contained but non-trivial.
Same downstream caveat as Tier 2: new fields break pdf-orchestrator’s exhaustive literal.

`\t` (tab) is a Tier-3-adjacent concern: with no tab-stop model it has no meaning.
Options are expand-to-spaces (needs a tab-width, another field) or keep it warned/rejected.
Recommendation: leave `\t` unsupported (keep the bug-0032 warning) until there is a real
need; only `\n` gets meaning in Tier 1.

## Where should the fix live — medpdf or pdf-maker?

**medpdf**, for even the basic version.  Tier 1’s correctness depends on font metrics
(ascender/descender/line-gap for leading; ascent/x-height/cap-height for block vertical
alignment) that live in medpdf and are already computed in `compute_text_metrics`.  If
pdf-maker instead split on `\n` and called `add_text_params` once per line, it would have
to **reimplement medpdf’s metric math** to get consistent leading and block-level vertical
centering — exactly the “don’t make the consumer re-derive font metrics” anti-pattern.  A
pdf-maker-side stopgap (fixed `size × 1.2` leading, top-anchored only) is possible but
gives visibly worse vertical alignment and duplicates logic that will drift.

**Recommendation:** implement **Tier 1 in medpdf** (no API change, backward-compatible),
which makes pdf-maker’s existing `\n` escape “just work” with no pdf-maker change at all.
Defer Tiers 2–3 (caller leading, wrap, truncate) to a later, batched semver bump, since
their new `AddTextParams` fields break pdf-orchestrator’s exhaustive literal and should
land together with the pdf-orchestrator update.

## Interaction with the bug-0032 control-char decision

Tier 1 resolves the `\n` half of the deferred bug-0032 question: `\n` becomes a line
separator instead of a dropped byte / hard error.  The remaining control characters
(`\t`, and the rest of `0x00`–`0x1F` / `0x7F`) stay unsupported and keep the warning — or,
if we prefer, escalate to bug-0032’s option #1 (reject loudly).  Revisit that once Tier 1
lands; until then the warning is the agreed behavior.

## Why Not Python / a consumer-side hack

This is font-metric-driven text layout that medpdf already owns the primitives for.
Splitting it into pdf-maker (or a script) would duplicate medpdf’s metrics and produce
inconsistent leading and vertical alignment across the two tools — the same class of
drift `medpdf-downstream-consumers` exists to prevent.

## Consumers to notify (done 2026-07-23)

- **pdf-maker** — `bugs/` note: documented `\n`/`\t` watermark escapes do not render as
  multiple lines because medpdf renders a single line; points here.
- **pdf-orchestrator** — lighter note (Chris confirms it is unused there today), pointing
  here, so a future session knows the limitation before relying on `\n` in watermark text.
