# Feature Plan: Fix AES PDF Encryption (lopdf 0.39.0 Bug)

## Problem

PDFs encrypted via `medpdf::encrypt_document()` cannot be decrypted by **any** reader — not Adobe Acrobat, not `pdf-dump`, not even lopdf itself. Both AES-256 (V5) and AES-128 (V4) are affected. The old C# tool used RC4-128 which worked, but the Rust rewrite intentionally rejects RC4 as cryptographically broken.

### Reproduction

```bash
# AES-256
pdf-merger -o /tmp/test.pdf input.pdf "1" --owner-password "test123" --encryption-algorithm aes256
pdf-dump /tmp/test.pdf
# → "decryption error: the supplied password is incorrect"

# AES-128
pdf-merger -o /tmp/test.pdf input.pdf "1" --owner-password "test123" --encryption-algorithm aes128
pdf-dump /tmp/test.pdf
# → "decryption error: invalid ciphertext length"
```

### Impact

- **pdf-merger**: `--owner-password` / `--user-password` with `--encryption-algorithm aes256|aes128` produces unopenable PDFs
- **pdf-orchestrator**: `PDF_SEC_OwnerPassword` config variable produces unopenable PDFs
- The encryption "succeeds" (no error at write time), but the resulting file cannot be read

### Root Cause

`lopdf 0.39.0`'s `Document::encrypt()` implementation for AES crypt filters appears to produce malformed encryption dictionaries or incorrectly encrypted streams. This is a bug in lopdf, not in medpdf's calling code. lopdf 0.39.0 is the latest published version (as of Feb 2026).

## Proposed Fix — Investigation Path

### Option A: Patch lopdf (preferred if feasible)

1. Clone lopdf, write a round-trip test: encrypt → save → load → assert content matches
2. Identify the bug in `lopdf::encryption` (likely in `Aes256CryptFilter` or `Aes128CryptFilter`)
3. Submit a PR upstream; use a git patch dependency in the meantime:
   ```toml
   lopdf = { git = "https://github.com/<fork>/lopdf", branch = "fix-aes-encryption" }
   ```

### Option B: External encryption via qpdf

1. Skip lopdf encryption entirely
2. Save the PDF unencrypted to a temp file
3. Shell out to `qpdf` for encryption:
   ```
   qpdf --encrypt <user_pw> <owner_pw> 256 -- input.pdf output.pdf
   ```
4. Pros: battle-tested encryption, works with all viewers
5. Cons: external dependency, not pure Rust

### Option C: Use a different Rust PDF encryption crate

1. Evaluate alternatives (e.g., `pdf-rs`, `printpdf`, or implement AES encryption directly against the PDF spec)
2. Replace the `lopdf::Document::encrypt()` call in `medpdf::encrypt_document()`
3. Pros: pure Rust, no external dependency
4. Cons: significant implementation effort, must match PDF 2.0 spec precisely

### Option D: Temporary workaround — fall back to RC4-128

1. Re-enable RC4-128 as a compatibility option (lopdf V2/R3 encryption does work)
2. Mark it as deprecated/insecure in CLI help and warnings
3. Pros: immediate fix, zero new code
4. Cons: RC4 is cryptographically weak (though adequate for PDF DRM use cases)

## Recommended Approach

**Start with Option A** (investigate and patch lopdf). If the bug is straightforward, this is the cleanest fix. If lopdf's encryption architecture is fundamentally broken, fall back to **Option D** (RC4 as temporary workaround) while pursuing **Option B** (qpdf) or **Option C** (alternative crate) for a permanent solution.

## Files Affected

| File | Change |
|------|--------|
| `medpdf/src/pdf_encryption.rs` | Fix or replace encryption implementation |
| `medpdf/Cargo.toml` | Possibly update lopdf dependency or add alternatives |
| `pdf-merger Cargo.toml` | Same dependency changes |
| `pdf-orchestrator Cargo.toml` | Same dependency changes (path dep on medpdf) |

## Testing

- **Round-trip test**: Encrypt with medpdf → save → load with lopdf → verify content
- **Cross-viewer test**: Open encrypted PDF in Adobe Acrobat, macOS Preview, pdf-dump
- **Password variants**: empty user password + owner password, both passwords, empty both (should skip)
- **Permission verification**: Encrypted PDF respects permission flags in viewers

## Immediate Mitigation

Until this is fixed, users should set `PDF_SEC_DocumentSecurityLevel=none` in their config, or omit password variables entirely. The orchestrator and pdf-merger should log a warning if AES encryption is requested, noting the known issue.
