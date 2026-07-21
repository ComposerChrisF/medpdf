# Bug Report: AES-256-named tests silently became AES-128 tests when the default changed

**Severity:** Low (test-suite integrity; no shipped-code defect)
**Component:** `medpdf` — `tests/encryption_tests.rs`
**Category:** CODE BUG (test suite).  The library code is correct; the tests claim coverage that does not exist.
**Verified:** 2026-07-16 deep review — confirmed by reading the tests and by an empirical rasterization check of all three algorithms.

## Description

Commit `1b2dc09` (“Default encryption to AES-128 (lopdf AES-256 bug)”) moved `#[default]` on `EncryptionAlgorithm` from `Aes256` to `Aes128`.  Three tests rely on the default instead of passing `.algorithm(...)` explicitly, so they silently became AES-128 tests while keeping AES-256 names:

- `tests/encryption_tests.rs:172-179` — `test_encrypt_aes256`
- `tests/encryption_tests.rs:190-200` — `test_encrypt_decrypt_aes256_in_memory`
- `tests/encryption_tests.rs:251-268` — `test_encrypt_save_produces_valid_file`

Consequence: the AES-256 arm of `encrypt_document` (`src/pdf_encryption.rs:65-81`, including random file-key generation and `EncryptionVersion::V5` construction) has zero enabled coverage — its only dedicated test, `visual_encryption_aes256_preserves_rendering`, is `#[ignore]`d.

Context (re-verified during this review): the underlying lopdf AES-256 bug is still present in lopdf 0.42.  Rasterizing a watermarked page encrypted with each algorithm gives dark-pixel counts of 4150 (plain), 4150 (AES-128), 4150 (RC4-128), and **0 (AES-256 — blank page)**.  So the `#[ignore]` on the visual test and the AES-128 default remain correct; only the test names/coverage are wrong.

## Reproduction

Read the three tests: none passes `EncryptionAlgorithm::Aes256`.  `test_encrypt_aes256` therefore exercises AES-128.

## Suggested fix

1. Add `.algorithm(EncryptionAlgorithm::Aes256)` to the two `aes256`-named tests.  In-memory encrypt/decrypt round-trips work for AES-256 even though rendered output is corrupt, so they can stay enabled as construction/round-trip tests.
2. For `test_encrypt_save_produces_valid_file`, either state the default explicitly or rename to say it tests the default algorithm.
3. Leave the visual AES-256 test `#[ignore]`d until the upstream lopdf bug is fixed, with a comment naming the lopdf issue.

## Why the fix addresses the bug

Test names are load-bearing documentation.  Making the algorithm explicit restores real AES-256 construction coverage and prevents the next default flip from silently changing what the suite tests — exactly the “suspicious tests” class commit `17d161c` fixed elsewhere.
