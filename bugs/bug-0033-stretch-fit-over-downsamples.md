# Bug Report: Stretch-fit downsampling over-downsamples the low-DPI axis

**Severity:** Medium — needless, silent quality loss (up to several×) for aspect-changing placements
**Component:** `medpdf-image` — `src/lib.rs:323-335` (`eff_dpi = eff_dpi_x.max(eff_dpi_y)`, one uniform scale)
**Category:** CODE BUG
**Verified:** 2026-07-16 deep review — reproduced live (image subagent, `bughunt/main.rs::test_stretch_over_downsample`): written XObject `/Width` confirmed 300 where 1000 was allowed.

## Description

For `ImageFit::Stretch` into an aspect-changing box, the two axes have different effective DPI, but the downsample scale is computed from the **worst** axis and applied uniformly.  Example: 1000×1000 px into a 720×72 pt box (10 in × 1 in) at `max_dpi = 300` — eff-DPI is 100 horizontally (already compliant) and 1000 vertically; the uniform scale 0.3 shrinks the image to 300×300, so the x-axis now renders at 30 DPI when the source had 100 and the cap allows 300.  A 3.3× resolution loss on one axis, silently.  Contain/Cover are unaffected (their axes share one DPI).

## Reproduction (test-ready)

1. 1000×1000 px PNG, `add_image` with `ImageFit::Stretch` into a 720×72 pt box, `max_dpi = 300`.
2. Read the written XObject: assert `/Width == 1000` and `/Height == 300` — today both are 300.

## Suggested fix

Scale each axis independently: `new_w = (px_w × (max_dpi / eff_dpi_x).min(1.0)).round().max(1.0)`, and the same for height; keep the early-out when neither axis exceeds the cap.

## Why the fix addresses the bug

Once Stretch decouples the axes, DPI is a per-axis property; clamping per axis is a no-op in the uniform case (Contain/Cover behavior stays bit-identical) and removes exactly the over-shrink on the compliant axis.
