//! Export As → DXF Options and DWG Options, drawn headlessly.

use serde_json::json;

use super::tests_export::{app, frame, kind, names, set};
use super::*;

fn text(bytes: &[u8]) -> String {
    String::from_utf8(bytes.to_vec()).unwrap()
}

#[test]
fn export_as_dxf_shows_dxf_options_then_writes_and_remembers_them() {
    let (mut app, written) = app(1);
    app.run("file.exportAs", json!({"format": "dxf"})).unwrap();
    assert_eq!(kind(&app), Some("exportAs"));
    frame(&mut app);
    set(&mut app, "useArtboards", json!(true));
    confirm(&mut app).unwrap();
    assert_eq!(kind(&app), Some("dxfOptions"), "DXF Options follow");
    let d = app.ui.dialog.as_ref().unwrap();
    assert_eq!((d.str("path"), d.str("version"), d.str("preserve")), ("/out/Untitled-1.dxf".into(), "2018".into(), "appearance".into()));
    frame(&mut app);
    // Every control draws: R12 with true colour shows its note, Maximize Editability frees Outline Text.
    set(&mut app, "version", json!("R12"));
    set(&mut app, "preserve", json!("editability"));
    set(&mut app, "unit", json!("Inches"));
    frame(&mut app);
    set(&mut app, "colors", json!("256"));
    let r = confirm(&mut app).unwrap();
    assert!(app.ui.dialog.is_none());
    assert_eq!(r["path"], "/out/Untitled-1.dxf");
    let dxf = text(&written.borrow()[0].1);
    assert!(dxf.contains("$ACADVER\n  1\nAC1009"), "{dxf}");
    // The next DXF Options start from these choices, but not from this export's path.
    crate::menus::invoke(&mut app, "ui.dxfOptionsDialog", json!({}));
    let d = app.ui.dialog.as_ref().unwrap();
    assert_eq!((d.str("version"), d.str("unit"), d.str("colors")), ("R12".into(), "Inches".into(), "256".into()));
    assert!(!d.fields.contains_key("path"));
    assert!(app.ui.dxf_options.get("useArtboards").is_none());
}

#[test]
fn bad_dxf_options_keep_the_dialog_open() {
    let (mut app, written) = app(1);
    app.run("ui.dxfOptionsDialog", json!({"path": "/out/a.dxf"})).unwrap();
    set(&mut app, "scale", json!(-1));
    assert!(confirm(&mut app).is_err());
    assert_eq!(kind(&app), Some("dxfOptions"));
    set(&mut app, "scale", json!(10));
    // Export Selected Art Only with nothing selected.
    app.run("select.none", json!({})).unwrap();
    set(&mut app, "selectedOnly", json!(true));
    frame(&mut app);
    let e = confirm(&mut app).unwrap_err();
    assert!(e.contains("select something"), "{e}");
    assert!(written.borrow().is_empty());
    set(&mut app, "selectedOnly", json!(false));
    confirm(&mut app).unwrap();
    assert_eq!(names(&written), ["/out/a.dxf"]);
}

#[test]
fn export_as_dwg_shows_dwg_options_then_writes_dwg() {
    let (mut app, written) = app(1);
    // A DXF version used last that DWG isn't written as (R12): DWG Options start on the default,
    // the other options carry over.
    app.ui.dxf_options = json!({"version": "R12", "unit": "Inches"});
    app.run("file.exportAs", json!({"format": "dwg"})).unwrap();
    assert_eq!(app.ui.dialog.as_ref().unwrap().str("format"), "dwg");
    frame(&mut app);
    confirm(&mut app).unwrap();
    assert_eq!(kind(&app), Some("dxfOptions"), "DWG Options follow");
    let d = app.ui.dialog.as_ref().unwrap();
    assert_eq!(
        (d.str("format"), d.str("path"), d.str("version"), d.str("unit")),
        ("dwg".into(), "/out/Untitled-1.dwg".into(), "2018".into(), "Inches".into())
    );
    assert_eq!((dxf_options::SPEC.heading)(d), "DWG Options");
    frame(&mut app);
    set(&mut app, "version", json!("R14"));
    let e = confirm(&mut app).unwrap_err();
    assert!(e.contains("DWG version `R14`"), "{e}");
    assert_eq!(kind(&app), Some("dxfOptions"), "a version DWG isn't written as keeps the dialog open");
    set(&mut app, "version", json!("2000"));
    frame(&mut app);
    let r = confirm(&mut app).unwrap();
    assert_eq!(r["path"], "/out/Untitled-1.dwg");
    assert_eq!(names(&written), ["/out/Untitled-1.dwg"]);
    assert_eq!(&written.borrow()[0].1[..6], b"AC1015");
    // A version the params ask for is kept (OK then says what's wrong with it); "DWG" and ".dwg"
    // name the format too.
    for format in ["DWG", ".dwg"] {
        crate::menus::invoke(&mut app, "ui.dxfOptionsDialog", json!({"format": format, "version": "R12"}));
        let d = app.ui.dialog.as_ref().unwrap();
        assert_eq!((d.str("format"), d.str("version")), ("dwg".into(), "R12".into()), "{format}");
        assert!(confirm(&mut app).unwrap_err().contains("DWG version `R12`"), "{format}");
    }
    // DXF Options keep offering every DXF version.
    crate::menus::invoke(&mut app, "ui.dxfOptionsDialog", json!({"version": "R12"}));
    let d = app.ui.dialog.as_ref().unwrap();
    assert_eq!((d.str("format"), d.str("version")), ("dxf".into(), "R12".into()));
    assert_eq!((dxf_options::SPEC.heading)(d), "DXF Options");
}

#[test]
fn the_dxf_options_dialog_needs_a_document() {
    let mut app = VectorcraftApp::new(vectorcraft_engine::Session::new(), Default::default());
    assert!(app.run("ui.dxfOptionsDialog", json!({})).is_err());
    assert!(!crate::menus::enabled(&app, "ui.dxfOptionsDialog"));
}
