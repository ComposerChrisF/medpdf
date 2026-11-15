# Bug: pdf-test-visual reads a golden image it cannot stat as missing, writes a new one through it, and passes

**Severity:** Low — test infrastructure only (`publish = false`), and the silent case needs a golden that cannot be stat’ed but can be written, such as a symlink to nothing.  The cost is a visual regression test that passes without comparing anything.

**Type:** Code bug in this repo (`pdf-test-visual/src/lib.rs`).

**Found:** 15-Nov-2026, while adopting the restriction lints (`disallowed_methods` on `Path::exists`); confirmed by a scratch program against the working tree.

## What Happens

`handle_golden_creation` (`pdf-test-visual/src/lib.rs`, the `if !golden_path.exists()` branch) treats a golden it cannot see as a first run: it creates the parent directory, writes the actual image as the golden, and returns success, so `assert_images_exact` and `assert_images_ssim` pass without comparing.  `exists()` answers `false` for every error, not only `NotFound`.

Reproduction: make `golden/g.png` a symlink to a file that does not exist, and call `assert_images_exact(golden/g.png, png)`.  It prints “Created golden image”, returns `Ok(())`, and the link’s target now exists, holding the actual image.  The golden the link was meant to reach was never compared.  When the stat fails for a reason that also stops the write (`EACCES` on a parent, `ELOOP`), the write fails and the test fails loudly, so those cases are safe by accident.

## The Rule It Breaks

A probe that gates a write must answer three ways: present, absent (`NotFound` only, and for a link only when the link itself is absent too), or unknown.  Only “absent” may license the first-run write; “unknown” must fail the comparison, naming the path.

## Fix Direction

In `handle_golden_creation`, replace `exists()` with `fs::symlink_metadata(golden_path)`: `NotFound` is a first run as today; a link whose target is missing, or any other error, returns `VisualTestError::IoError` (or a new variant naming the path) instead of writing.  The `-actual` cleanup probes keep their current behavior, since a wrong `false` there only keeps a stale file.

## The Test That Pins It

A Unix test that makes the golden a symlink to a missing file in a temp directory, calls `assert_images_exact`, and asserts that **the call returns an error and the link’s target was not created**.  It must fail with the fix reverted (today the call returns `Ok` and the target appears).
