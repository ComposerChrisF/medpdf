// tests/font_path_unreadable_regression.rs
//
// Regression test for bugs/bug-0043: `find_font` decided whether its argument was
// a file with `Path::exists`, which answers false for every error, so a font file
// it could not stat was taken as a family name, and an installed family of that
// name was returned in its place.  A path that cannot be stat'ed is now an error.
#![cfg(unix)]

use medpdf::types::{FontStyle, FontWeight};
use medpdf::{FontPath, find_font, find_font_with_style};
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

/// An installed family on the test machine, or `None` (the test then skips).
fn installed_family() -> Option<&'static str> {
    [
        "Arial",
        "Helvetica",
        "DejaVu Sans",
        "Liberation Sans",
        "Noto Sans",
    ]
    .into_iter()
    .find(|name| find_font(Path::new(name)).is_ok())
}

#[test]
fn unreadable_font_path_is_an_error_not_a_system_font() {
    let Some(family) = installed_family() else {
        eprintln!("skipping: no known font family is installed");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let locked = dir.path().join("locked");
    std::fs::create_dir(&locked).unwrap();
    let font_file = locked.join(format!("{family}.ttf"));
    std::fs::write(&font_file, b"not a real font; only its stem matters").unwrap();
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();

    let stat_blocked = std::fs::metadata(&font_file).is_err();
    let plain = find_font(&font_file);
    let styled = find_font_with_style(&font_file, FontWeight::NORMAL, FontStyle::Normal);
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).unwrap();

    if !stat_blocked {
        eprintln!("skipping: a mode-000 directory does not block stat here (running as root?)");
        return;
    }
    for (name, result) in [("find_font", plain), ("find_font_with_style", styled)] {
        match result {
            Err(_) => {}
            Ok(FontPath::Path(p)) => panic!("{name} returned a path it could not read: {p:?}"),
            Ok(_) => panic!(
                "{name} replaced the unreadable {font_file:?} with the installed \
                 '{family}' family (bug-0043)"
            ),
        }
    }
}
