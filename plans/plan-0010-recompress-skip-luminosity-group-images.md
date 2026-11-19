# Plan: Keep Images Inside a Luminosity Soft-Mask Group Lossless in `recompress_images`

## Problem

`medpdf_image::recompress_images` skips an image that carries `/SMask` or `/Mask` (bug-0028), and since bug-0046 also skips any stream another stream references as `/SMask` or `/Mask`.  A soft mask can also come from the graphics state.  An ExtGState’s `/SMask << /Type /Mask /S /Luminosity /G 12 0 R >>` names a transparency-group Form XObject, and the luminosity of whatever that form paints becomes the alpha.  A gray FlateDecode image drawn inside that form is a mask in all but name, but nothing references it as `/SMask` or `/Mask`.  It passes every check, and when its id is among the caller’s it is JPEG-encoded, so the mask picks up JPEG artefacts.

The module docs promise that the caller picks the images (“the choice belongs to the caller, which knows what each image is”), but pdf-orchestrator’s `<ImportPdf recompressImages>` passes every copied object, so for that caller the premise does not hold.

Found by the review of the bug-0046 fix (rust-reviewer F1, confirmed by Fable from the source), 18-Nov-2026.  Not yet reproduced by a run.

## Proposed Change

Before the loop, also collect the image XObjects reachable from every soft-mask group form: each ExtGState `/SMask` dictionary’s `/G` form, its `/Resources /XObject` entries, and recursively any nested Form XObjects there.  Skip those ids as `mask_stream_ids` does today.  No API change.

## Implementation Notes

- ExtGState dictionaries can be inline in a page’s `/Resources` or indirect; scanning every dictionary in `doc.objects` for `/SMask` whose value is a dictionary (or a reference to one) with a `/G` key covers both without walking pages.
- Walk only the group form’s resource tree (`/XObject` entries, recursing into Form XObjects with a visited set against cycles).  Do not parse content streams: an image listed in the form’s resources but not drawn is over-skipped, which only keeps it lossless.
- An image shared between a mask group and ordinary page content is skipped too.  That is the conservative side, and is correct.
- Pin with the reproduction above: a 200×200 noisy 8-bit DeviceGray Flate image under such a group, passed alone at `quality=30, min_size=1`, must keep `/FlateDecode`; it must fail with the walk reverted.

## Why Not a Workaround

The only workaround is for every caller to work out which images feed soft-mask groups, which is the same walk.  pdf-orchestrator cannot do that without reimplementing PDF transparency semantics, so the walk belongs beside the existing mask skip.
