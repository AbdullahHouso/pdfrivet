//! Password protection: adding, changing and removing it.

mod common;

use common::{fixture, page_text, pdf, serial, temp_dir};
use rivet_core::{Allowed, ErrorCode, Protection};

const NOTHING: Allowed = Allowed {
    print: false,
    copy: false,
    edit: false,
    fill_forms: false,
    annotate: false,
};

#[test]
fn protects_with_passwords_and_restrictions() {
    let _serial = serial();
    let dir = temp_dir("protect");
    let out = dir.join("protected.pdf");
    let mut doc = pdf().open(&fixture("basic.pdf"), None).unwrap();
    doc.set_protection(Protection::Set {
        open_password: "sesame".into(),
        owner_password: "boss".into(),
        allowed: Allowed {
            print: true,
            ..NOTHING
        },
    })
    .unwrap();
    doc.save(&out).unwrap();
    // The open document is now the protected one.
    assert!(doc.properties().encrypted);

    let err = pdf().open(&out, None).err().unwrap();
    assert_eq!(err.code, ErrorCode::PasswordRequired);
    assert_eq!(
        pdf().open(&out, Some("nope")).err().unwrap().code,
        ErrorCode::WrongPassword
    );

    // With the open password: readable, but restricted.
    let mut reader = pdf().open(&out, Some("sesame")).unwrap();
    assert_eq!(reader.page_count(), 3);
    let info = reader.info().unwrap();
    assert!(!info.can_copy && !info.can_annotate && !info.can_assemble);
    assert!(reader.properties().permissions.print);
    assert!(reader.properties().needs_open_password);
    assert!(!reader.properties().can_change_protection);
    let err = reader.set_protection(Protection::Remove).unwrap_err();
    assert_eq!(err.code, ErrorCode::OwnerPasswordRequired);
    // Unlocking with the owner password allows changing it.
    assert!(!reader.unlock_owner("sesame").unwrap());
    assert!(reader.unlock_owner("boss").unwrap());
    assert!(reader.properties().can_change_protection);

    // Removing it: the file opens without a password, with its text.
    reader.set_protection(Protection::Remove).unwrap();
    let plain = dir.join("plain.pdf");
    reader.save(&plain).unwrap();
    let opened = pdf().open(&plain, None).unwrap();
    assert!(!opened.properties().encrypted);
    assert!(page_text(&opened, 0).contains("Page one"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn restricts_without_an_open_password() {
    let _serial = serial();
    let dir = temp_dir("restrict");
    let out = dir.join("restricted.pdf");
    let mut doc = pdf().open(&fixture("basic.pdf"), None).unwrap();
    doc.set_protection(Protection::Set {
        open_password: String::new(),
        owner_password: "boss".into(),
        allowed: Allowed {
            copy: true,
            ..NOTHING
        },
    })
    .unwrap();
    doc.save(&out).unwrap();

    let opened = pdf().open(&out, None).unwrap();
    let props = opened.properties();
    assert!(props.encrypted && !props.needs_open_password);
    assert!(opened.info().unwrap().can_copy);
    assert!(!props.permissions.print);
    assert!(page_text(&opened, 1).contains("Page two"));
    if std::env::var_os("KEEP").is_none() {
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[test]
fn changes_an_existing_password() {
    let _serial = serial();
    let dir = temp_dir("repassword");
    let out = dir.join("changed.pdf");
    // The fixture's password is also its owner password (pypdf's default).
    let mut doc = pdf().open(&fixture("password.pdf"), Some("rivet")).unwrap();
    assert!(doc.properties().can_change_protection);
    doc.set_protection(Protection::Set {
        open_password: "new".into(),
        owner_password: String::new(),
        allowed: Allowed {
            print: true,
            copy: true,
            edit: true,
            fill_forms: true,
            annotate: true,
        },
    })
    .unwrap();
    doc.save(&out).unwrap();
    assert_eq!(
        pdf().open(&out, Some("rivet")).err().unwrap().code,
        ErrorCode::WrongPassword
    );
    let opened = pdf().open(&out, Some("new")).unwrap();
    assert!(page_text(&opened, 0).contains("Page one"));
    let _ = std::fs::remove_dir_all(&dir);
}
