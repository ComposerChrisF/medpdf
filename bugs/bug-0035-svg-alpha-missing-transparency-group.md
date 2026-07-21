# Bug Report: SVG alpha applies per-element instead of to the SVG as a whole (missing transparency `/Group`)

**Severity:** Medium — visibly wrong rendering (double-darkened overlaps, seams) for any translucent SVG with overlapping elements
**Component:** `medpdf-image` — `src/svg.rs:354-364` (`extract_form_xobject` builds the form dict without `/Group`), `src/svg.rs:249-257` (ca/CA ExtGState)
**Category:** CODE BUG
**Verified:** 2026-07-16 deep review — structure confirmed live (image subagent probe: form dict keys are exactly `["Type", "Subtype", "BBox", "Resources", "Length"]`); the rendering consequence follows directly from PDF 32000-1 §11.6.6.

## Description

`DrawSvgParams::alpha(0.5)` is implemented as a constant-alpha ExtGState around the `Do` of a Form XObject.  Without `/Group << /S /Transparency >>` on the form, constant alpha applies to **each painting operator** inside the form: where elements overlap, the region composites twice and renders darker, with visible seams — not a uniform 50% fade of the artwork as a unit.  Additionally, any `/Group` the svg2pdf intermediate page declared (relevant for SVG blend modes and isolation) is dropped by `extract_form_xobject`.

## Reproduction (test-ready)

1. SVG with two overlapping circles; `add_svg` with `.alpha(0.5)`.
2. Structural assert: the Form XObject dict contains `/Group << /S /Transparency >>` — fails today.
3. Visual assert (pdf-test-visual): the lens-shaped overlap region has the same darkness as the non-overlap regions — fails today.

## Suggested fix

In `extract_form_xobject`, set `form_dict.set("Group", dictionary!{"S" => "Transparency"})`, carrying over the source page’s `/Group` when svg2pdf declared one.

## Why the fix addresses the bug

Making the form a transparency group is the PDF mechanism for treating composite artwork as a unit under constant alpha (what mutool/cairo emit); it is purely additive — with alpha = 1.0 rendering is unchanged.
