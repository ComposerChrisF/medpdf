# Bug Report: straight-alpha downsampling bleeds hidden color into visible edges (halo)

**Severity:** Medium-Low (visible quality defect on downsampled images with transparency)
**Component:** `medpdf-image` — `src/lib.rs:383-419` (max-DPI downsampling path)
**Category:** CODE BUG
**Verified:** 2026-07-16 deep review — reproduced with a measured pixel value (subagent scratch crate `scratchpad/bughunt/main.rs::test_alpha_halo`).

## Description

When an image with an alpha channel is downsampled (the `max_dpi` path), the color plane and the alpha plane are resized independently with Lanczos3.  PNG alpha is straight (un-premultiplied), so the RGB values under fully-transparent pixels are arbitrary — often black.  Resampling the color plane without premultiplying mixes that invisible color into edge pixels that remain substantially opaque, producing a dark halo along transparency edges.

Measured: a 600×600 image, left half opaque red, right half fully-transparent black, downsampled to 300 px — a boundary pixel whose SMask alpha is ≥ 200 has red = **241** instead of 255.

## Reproduction (test-ready)

1. Build a 600×600 RGBA image: columns 0-299 opaque red `(255,0,0,255)`, columns 300-599 transparent black `(0,0,0,0)`.
2. `add_image` with a destination box small enough that `max_dpi` forces downsampling to ~300 px wide.
3. Decode the written image XObject and its SMask.  Find a column where SMask alpha ≥ 200; assert the red channel is ≥ 250.  Currently it is ~241.

## Suggested fix

In the downsampling path, when `alpha_channel` is `Some`: premultiply RGB by alpha, resize both planes, then un-premultiply (`rgb = round(rgb * 255 / alpha)` for `alpha > 0`, else 0).

## Why the fix addresses the bug

Premultiplied space is the mathematically correct domain for resampling imagery with alpha: fully-transparent pixels contribute zero to the filter sum by construction, so hidden colors cannot bleed into visible output.  For images with no alpha the code path is unchanged.
