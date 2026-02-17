mod fixtures;

use lopdf::encryption::Permissions;
use lopdf::{Object, StringFormat};
use medpdf::pdf_encryption::{encrypt_document, EncryptionAlgorithm, EncryptionParams};
use medpdf::{parse_permission_name, parse_permissions};

/// Helper: adds a trailer ID entry required by AES-128 (V4) encryption.
fn ensure_trailer_id(doc: &mut lopdf::Document) {
    if doc.trailer.get(b"ID").is_err() {
        let id_bytes = b"0123456789abcdef".to_vec();
        doc.trailer.set(
            "ID",
            Object::Array(vec![
                Object::String(id_bytes.clone(), StringFormat::Literal),
                Object::String(id_bytes, StringFormat::Literal),
            ]),
        );
    }
}

// ──────────────────────────────────────────────
// Permission parsing tests
// ──────────────────────────────────────────────

#[test]
fn test_parse_permission_name_valid() {
    assert_eq!(parse_permission_name("print").unwrap(), Permissions::PRINTABLE);
    assert_eq!(parse_permission_name("modify").unwrap(), Permissions::MODIFIABLE);
    assert_eq!(parse_permission_name("copy").unwrap(), Permissions::COPYABLE);
    assert_eq!(parse_permission_name("annotate").unwrap(), Permissions::ANNOTABLE);
    assert_eq!(parse_permission_name("fill").unwrap(), Permissions::FILLABLE);
    assert_eq!(
        parse_permission_name("accessibility").unwrap(),
        Permissions::COPYABLE_FOR_ACCESSIBILITY
    );
    assert_eq!(parse_permission_name("assemble").unwrap(), Permissions::ASSEMBLABLE);
    assert_eq!(
        parse_permission_name("print_hq").unwrap(),
        Permissions::PRINTABLE_IN_HIGH_QUALITY
    );
}

#[test]
fn test_parse_permission_name_aliases() {
    assert_eq!(parse_permission_name("printable").unwrap(), Permissions::PRINTABLE);
    assert_eq!(parse_permission_name("modifiable").unwrap(), Permissions::MODIFIABLE);
    assert_eq!(parse_permission_name("copyable").unwrap(), Permissions::COPYABLE);
    assert_eq!(parse_permission_name("annotable").unwrap(), Permissions::ANNOTABLE);
    assert_eq!(parse_permission_name("fillable").unwrap(), Permissions::FILLABLE);
    assert_eq!(
        parse_permission_name("copyable_for_accessibility").unwrap(),
        Permissions::COPYABLE_FOR_ACCESSIBILITY
    );
    assert_eq!(parse_permission_name("assemblable").unwrap(), Permissions::ASSEMBLABLE);
    assert_eq!(
        parse_permission_name("printable_in_high_quality").unwrap(),
        Permissions::PRINTABLE_IN_HIGH_QUALITY
    );
}

#[test]
fn test_parse_permission_name_case_insensitive() {
    assert_eq!(parse_permission_name("PRINT").unwrap(), Permissions::PRINTABLE);
    assert_eq!(parse_permission_name("Print").unwrap(), Permissions::PRINTABLE);
    assert_eq!(parse_permission_name("ALL").unwrap(), Permissions::all());
    assert_eq!(parse_permission_name("None").unwrap(), Permissions::empty());
}

#[test]
fn test_parse_permission_name_all_and_none() {
    assert_eq!(parse_permission_name("all").unwrap(), Permissions::all());
    assert_eq!(parse_permission_name("none").unwrap(), Permissions::empty());
}

#[test]
fn test_parse_permission_name_invalid() {
    assert!(parse_permission_name("bogus").is_err());
    assert!(parse_permission_name("").is_err());
    assert!(parse_permission_name("rc4").is_err());
}

#[test]
fn test_parse_permissions_multiple() {
    let names: Vec<String> = vec!["print".into(), "copy".into()];
    let perms = parse_permissions(&names).unwrap();
    assert!(perms.contains(Permissions::PRINTABLE));
    assert!(perms.contains(Permissions::COPYABLE));
    assert!(!perms.contains(Permissions::MODIFIABLE));
}

#[test]
fn test_parse_permissions_empty_returns_all() {
    let perms = parse_permissions(&[]).unwrap();
    assert_eq!(perms, Permissions::all());
}

#[test]
fn test_parse_permissions_invalid_entry() {
    let names: Vec<String> = vec!["print".into(), "invalid".into()];
    assert!(parse_permissions(&names).is_err());
}

// ──────────────────────────────────────────────
// EncryptionParams builder tests
// ──────────────────────────────────────────────

#[test]
fn test_encryption_params_defaults() {
    let params = EncryptionParams::new("user", "owner");
    assert_eq!(params.user_password, "user");
    assert_eq!(params.owner_password, "owner");
    assert_eq!(params.algorithm, EncryptionAlgorithm::Aes256);
    assert_eq!(params.permissions, Permissions::all());
}

#[test]
fn test_encryption_params_builder() {
    let params = EncryptionParams::new("u", "o")
        .algorithm(EncryptionAlgorithm::Aes128)
        .permissions(Permissions::PRINTABLE);
    assert_eq!(params.algorithm, EncryptionAlgorithm::Aes128);
    assert_eq!(params.permissions, Permissions::PRINTABLE);
}

// ──────────────────────────────────────────────
// Encrypt document tests (in-memory)
// ──────────────────────────────────────────────

#[test]
fn test_encrypt_aes256() {
    let mut doc = fixtures::create_pdf_with_pages(1);
    ensure_trailer_id(&mut doc);
    let params = EncryptionParams::new("user", "owner");
    encrypt_document(&mut doc, &params).unwrap();
    assert!(doc.is_encrypted());
}

#[test]
fn test_encrypt_aes128() {
    let mut doc = fixtures::create_pdf_with_pages(1);
    ensure_trailer_id(&mut doc);
    let params = EncryptionParams::new("user", "owner").algorithm(EncryptionAlgorithm::Aes128);
    encrypt_document(&mut doc, &params).unwrap();
    assert!(doc.is_encrypted());
}

#[test]
fn test_encrypt_decrypt_aes256_in_memory() {
    let mut doc = fixtures::create_pdf_with_pages(2);
    ensure_trailer_id(&mut doc);
    let params = EncryptionParams::new("secret", "admin");
    encrypt_document(&mut doc, &params).unwrap();
    assert!(doc.is_encrypted());

    doc.decrypt("secret").unwrap();
    assert_eq!(doc.get_pages().len(), 2);
}

#[test]
fn test_encrypt_decrypt_aes128_in_memory() {
    let mut doc = fixtures::create_pdf_with_pages(2);
    ensure_trailer_id(&mut doc);
    let params = EncryptionParams::new("view", "edit").algorithm(EncryptionAlgorithm::Aes128);
    encrypt_document(&mut doc, &params).unwrap();
    assert!(doc.is_encrypted());

    doc.decrypt("view").unwrap();
    assert_eq!(doc.get_pages().len(), 2);
}

#[test]
fn test_encrypt_empty_user_password() {
    let mut doc = fixtures::create_pdf_with_pages(1);
    ensure_trailer_id(&mut doc);
    let params = EncryptionParams::new("", "owner");
    encrypt_document(&mut doc, &params).unwrap();
    assert!(doc.is_encrypted());

    // Empty password should decrypt successfully
    doc.decrypt("").unwrap();
}

#[test]
fn test_encrypt_with_restricted_permissions() {
    let mut doc = fixtures::create_pdf_with_pages(1);
    ensure_trailer_id(&mut doc);
    let params = EncryptionParams::new("user", "owner").permissions(Permissions::PRINTABLE);
    encrypt_document(&mut doc, &params).unwrap();
    assert!(doc.is_encrypted());
}

#[test]
fn test_encrypt_multipage_document() {
    let mut doc = fixtures::create_pdf_with_pages(5);
    ensure_trailer_id(&mut doc);
    let params = EncryptionParams::new("pass", "admin");
    encrypt_document(&mut doc, &params).unwrap();
    assert!(doc.is_encrypted());

    doc.decrypt("pass").unwrap();
    assert_eq!(doc.get_pages().len(), 5);
}

// ──────────────────────────────────────────────
// Save encrypted document tests
// ──────────────────────────────────────────────

#[test]
fn test_encrypt_save_produces_valid_file() {
    let mut doc = fixtures::create_pdf_with_pages(2);
    ensure_trailer_id(&mut doc);
    let params = EncryptionParams::new("secret", "admin");
    encrypt_document(&mut doc, &params).unwrap();

    let tmp = tempfile::NamedTempFile::new().unwrap();
    doc.save(tmp.path()).unwrap();

    // Verify the file is non-empty and loadable
    let metadata = std::fs::metadata(tmp.path()).unwrap();
    assert!(metadata.len() > 0);

    // lopdf can load the encrypted file (objects are lazy-loaded)
    let reloaded = lopdf::Document::load(tmp.path()).unwrap();
    assert!(reloaded.is_encrypted());
}

#[test]
fn test_encrypt_aes128_save_produces_valid_file() {
    let mut doc = fixtures::create_pdf_with_pages(1);
    ensure_trailer_id(&mut doc);
    let params = EncryptionParams::new("user", "owner").algorithm(EncryptionAlgorithm::Aes128);
    encrypt_document(&mut doc, &params).unwrap();

    let tmp = tempfile::NamedTempFile::new().unwrap();
    doc.save(tmp.path()).unwrap();

    let reloaded = lopdf::Document::load(tmp.path()).unwrap();
    assert!(reloaded.is_encrypted());
}
