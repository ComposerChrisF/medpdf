# Bug Report: `pdf-test-visual` `run_with_timeout` can deadlock on large child output, then misreport a timeout

**Severity:** Low (internal test utility, `publish = false`; latent)
**Component:** `pdf-test-visual` — `src/lib.rs:214-270`
**Category:** CODE BUG — **status SUSPECTED: traced, not reproduced.**
**Verified:** 2026-07-16 deep review — code trace only (spec subagent); the pipe-buffer mechanism is standard OS behavior.

## Description

`run_with_timeout` pipes the child’s stdout/stderr but only reads them **after** `try_wait()` reports exit.  A child writing more than the OS pipe buffer (~64 KB) blocks on write forever; the polling loop then kills it at 30 s and reports `RasterizationFailed("timed out after 30s")` — a wrong diagnosis for a healthy-but-verbose run.  `pdftoppm`/`mutool` write image output to files, so normal runs emit little; the trigger is a failure case with verbose stderr (e.g. a badly damaged PDF producing thousands of per-object syntax warnings).

## Reproduction (when needed)

Substitute any command that writes > 64 KB to stderr before exiting; observe the spurious timeout after 30 s instead of an immediate return.

## Suggested fix

Drain the pipes concurrently with the wait: spawn one reader thread per pipe collecting into buffers (or use `child.wait_with_output()` guarded by a separate watchdog thread that kills on timeout).

## Why the fix addresses the bug

Concurrent draining removes the circular wait (child blocked writing, parent blocked polling), preserving both the output capture and the timeout feature.
