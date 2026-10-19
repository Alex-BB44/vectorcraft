//! DXF and DWG export through the commands: options, artboards, selection, images, and the
//! formats that can't be written (PICT).

use serde_json::{Value, json};

use super::*;

fn session(width: f64, height: f64, artboards: usize) -> Session {
    let mut s = Session::new();
    s.execute("file.new", &json!({"width": width, "height": height, "artboards": artboards})).unwrap();
    s
}

fn b64(v: &Value) -> Vec<u8> {
    vectorcraft_format::base64_decode(v["dataBase64"].as_str().expect("dataBase64")).unwrap()
}

fn text(v: &Value) -> String {
    String::from_utf8(b64(v)).unwrap()
}

/// The value after header variable `var`.
fn header(dxf: &str, var: &str) -> Option<String> {
    let lines: Vec<&str> = dxf.lines().collect();
    let i = lines.iter().position(|l| *l == var)?;
    lines.get(i + 2).map(|s| s.to_string())
}

/// The entity types of a DXF, in order.
fn entities(dxf: &str) -> Vec<String> {
    let lines: Vec<&str> = dxf.lines().collect();
    let start = lines.windows(3).position(|w| w[1] == "ENTITIES" && w[0].trim() == "2").unwrap();
    let pairs: Vec<(&str, &str)> = lines[start + 2..].chunks(2).map(|p| (p[0].trim(), p[1])).collect();
    pairs.iter().take_while(|p| p.1 != "ENDSEC").filter(|p| p.0 == "0").map(|p| p.1.to_string()).collect()
}

/// Two artboards side by side, a rectangle on each.
fn two_boards() -> Session {
    let mut s = session(100.0, 100.0, 2);
    let second = s.doc().unwrap().doc.artboards[1].rect.x0;
    s.execute("paint.setStroke", &json!({"none": true})).unwrap();
    s.execute("shape.rectangle", &json!({"x": 10, "y": 10, "width": 30, "height": 30})).unwrap();
    s.execute("shape.rectangle", &json!({"x": second + 10.0, "y": 10, "width": 30, "height": 30})).unwrap();
    s
}

#[test]
fn export_writes_dxf_of_the_chosen_version() {
    let mut s = two_boards();
    let r = s.execute("document.export", &json!({"format": "dxf"})).unwrap();
    assert_eq!(r["format"], "dxf");
    let dxf = text(&r);
    assert_eq!(header(&dxf, "$ACADVER").as_deref(), Some("AC1032"));
    assert_eq!(header(&dxf, "$INSUNITS").as_deref(), Some("4"), "millimetres by default");
    assert_eq!(entities(&dxf), ["LWPOLYLINE", "HATCH", "LWPOLYLINE", "HATCH"], "the whole document, origin at the first artboard");
    for (v, acadver) in [("R12", "AC1009"), ("R14", "AC1014"), ("2000", "AC1015"), ("2004", "AC1018"), ("2013", "AC1027")] {
        let r = s.execute("document.exportDxf", &json!({"version": v})).unwrap();
        assert_eq!(header(&text(&r), "$ACADVER").as_deref(), Some(acadver), "{v}");
    }
    // A number reads as a version too; serialize gives the same bytes.
    assert_eq!(header(&text(&s.execute("document.exportDxf", &json!({"version": 2007})).unwrap()), "$ACADVER").as_deref(), Some("AC1021"));
    let ser = s.execute("document.serialize", &json!({"format": "dxf", "unit": "in"})).unwrap();
    assert_eq!(header(&text(&ser), "$INSUNITS").as_deref(), Some("1"));
}

#[test]
fn use_artboards_writes_one_drawing_per_artboard() {
    let mut s = two_boards();
    let r = s.execute("document.export", &json!({"format": "dxf", "useArtboards": true})).unwrap();
    let files = r["files"].as_array().unwrap();
    let names: Vec<&str> = files.iter().map(|f| f["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["Untitled-1-Artboard-1.dxf", "Untitled-1-Artboard-2.dxf"]);
    for f in files {
        assert_eq!(entities(&text(f)), ["LWPOLYLINE", "HATCH"], "each holds the art over its artboard");
    }
    let r = s.execute("document.export", &json!({"format": "dxf", "useArtboards": true, "range": "2"})).unwrap();
    assert!(r.get("files").is_none());
    assert_eq!(entities(&text(&r)), ["LWPOLYLINE", "HATCH"]);
    // Without artboards: the origin at the art's bounds.
    let r = s.execute("document.export", &json!({"format": "dxf", "useArtboards": false, "unit": "pt"})).unwrap();
    let dxf = text(&r);
    assert_eq!(header(&dxf, "$LIMMIN").as_deref(), Some("0"));
    assert!(s.execute("document.export", &json!({"format": "dxf", "artboard": 5})).is_err());
    let e = s.execute("document.export", &json!({"format": "dxf", "range": "1-2"})).unwrap_err().to_string();
    assert!(e.contains("useArtboards"), "{e}");
}

#[test]
fn selected_only_exports_the_selection_in_its_layers() {
    let mut s = two_boards();
    s.execute("layer.new", &json!({"name": "Top"})).unwrap();
    s.execute("paint.setStroke", &json!({"color": "#000000"})).unwrap();
    let line = s.execute("shape.line", &json!({"x1": 0, "y1": 90, "x2": 90, "y2": 90})).unwrap()["id"].as_u64().unwrap();
    s.execute("select.set", &json!({"ids": [line]})).unwrap();
    let dxf = text(&s.execute("document.export", &json!({"format": "dxf", "selectedOnly": true})).unwrap());
    assert_eq!(entities(&dxf), ["LWPOLYLINE"]);
    assert!(dxf.contains("\n  2\nTop\n") && !dxf.contains("\n  2\nLayer 1\n"), "only the selection's layer");
    s.execute("select.none", &json!({})).unwrap();
    let e = s.execute("document.export", &json!({"format": "dxf", "selectedOnly": true})).unwrap_err().to_string();
    assert!(e.contains("select something"), "{e}");
    // Other formats take it too.
    s.execute("select.set", &json!({"ids": [line]})).unwrap();
    let svg = s.execute("document.serialize", &json!({"format": "svg", "selectedOnly": true})).unwrap()["text"].as_str().unwrap().to_string();
    assert_eq!((svg.matches("<path").count(), svg.matches("<rect").count()), (1, 0), "{svg}");
    assert!(svg.contains("id=\"Top\"") && !svg.contains("Layer"), "{svg}");
}

#[test]
fn placed_images_come_back_as_linked_files() {
    let mut s = super::tests_svg::image_session();
    let r = s.execute("document.export", &json!({"format": "dxf", "rasterFormat": "jpeg"})).unwrap();
    assert_eq!(entities(&text(&r)), ["IMAGE"]);
    let linked = r["linked"].as_array().unwrap();
    assert_eq!(linked.len(), 1);
    assert!(linked[0]["name"].as_str().unwrap().ends_with(".jpg"));
    assert_eq!(&b64(&linked[0])[..2], [0xFF, 0xD8]);
    let dir = std::env::temp_dir().join(format!("vc-dxf-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("plan.dxf").to_string_lossy().to_string();
    let r = s.execute("document.exportDxf", &json!({"path": path})).unwrap();
    assert_eq!(r["path"], path.as_str());
    let png = r["linked"][0].as_str().unwrap();
    assert!(png.ends_with(".png") && std::fs::read(png).unwrap().starts_with(b"\x89PNG"));
    assert!(std::fs::read_to_string(&path).unwrap().contains(&png.rsplit(['/', '\\']).next().unwrap().to_string()));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn bad_dxf_options_are_refused() {
    let mut s = two_boards();
    for p in [
        json!({"version": "2019"}),
        json!({"colors": 12}),
        json!({"scale": 0}),
        json!({"scale": -2}),
        json!({"unit": "furlong"}),
        json!({"rasterFormat": "gif"}),
        json!({"preserve": "both"}),
        json!({"alterPaths": "yes"}),
    ] {
        let e = s.execute("document.exportDxf", &p).unwrap_err().to_string();
        assert!(e.contains("DXF"), "{p}: {e}");
    }
    // Export for Screens writes images, SVG and PDF only.
    assert!(s.execute("document.exportForScreens", &json!({"formats": [{"format": "dxf"}]})).is_err());
}

#[test]
fn formats_list_dxf_and_what_can_not_be_written() {
    let mut s = Session::new();
    let r = s.execute("document.formats", &json!({})).unwrap();
    let dxf = r["formats"].as_array().unwrap().iter().find(|f| f["id"] == "dxf").unwrap().clone();
    // Read too since DXF import (tests_dxfimport).
    assert_eq!((dxf["read"].as_bool(), dxf["write"].as_bool()), (Some(true), Some(true)));
    for o in [
        "version",
        "unit",
        "scale",
        "scaleLineweights",
        "colors",
        "rasterFormat",
        "preserve",
        "alterPaths",
        "outlineText",
        "selectedOnly",
        "useArtboards",
    ] {
        assert!(dxf["options"].get(o).is_some(), "{o}");
    }
    assert!(r["writable"].as_array().unwrap().contains(&json!("dxf")));
    let unsupported: Vec<&str> = r["unsupported"].as_array().unwrap().iter().map(|u| u["id"].as_str().unwrap()).collect();
    assert_eq!(unsupported, ["pict"]);
    let dwg = r["formats"].as_array().unwrap().iter().find(|f| f["id"] == "dwg").unwrap();
    assert_eq!((dwg["read"].as_bool(), dwg["write"].as_bool()), (Some(false), Some(true)));
    assert!(dwg["options"]["version"]["description"].as_str().unwrap().starts_with("2000"));
    assert!(r["writable"].as_array().unwrap().contains(&json!("dwg")));
    assert!(!r["readable"].as_array().unwrap().contains(&json!("dwg")));
}

#[test]
fn pict_says_what_to_use_instead() {
    let mut s = two_boards();
    let e = s.execute("document.export", &json!({"format": "pict"})).unwrap_err().to_string();
    assert!(e.contains("PICT is a retired format"), "{e}");
    let e = s
        .execute("document.open", &json!({"name": "old.pct", "dataBase64": vectorcraft_format::base64_encode(&[0u8; 600])}))
        .unwrap_err()
        .to_string();
    assert!(e.contains("old.pct") && e.contains("PICT"), "{e}");
}

/// The DWG version code a file starts with.
fn acadver(dwg: &[u8]) -> &str {
    std::str::from_utf8(&dwg[..6]).unwrap()
}

#[test]
fn dwg_is_the_dxf_drawing_written_as_dwg() {
    let mut s = two_boards();
    // By format, by command and by extension; 2018 unless a version is asked for.
    for (cmd, p) in [
        ("document.export", json!({"format": "dwg"})),
        ("document.exportDwg", json!({})),
        ("document.export", json!({"format": "DWG", "version": "2000"})),
        ("document.exportDwg", json!({"version": 2010})),
    ] {
        let r = s.execute(cmd, &p).unwrap_or_else(|e| panic!("{cmd} {p}: {e}"));
        let want = match p["version"].to_string().trim_matches('"') {
            "2000" => "AC1015",
            "2010" => "AC1024",
            _ => "AC1032",
        };
        let bytes = b64(&r);
        assert!(vectorcraft_dwg::is_dwg(&bytes), "{cmd} {p}");
        assert_eq!(acadver(&bytes), want, "{cmd} {p}");
        assert_eq!(r["format"], "dwg", "{cmd} {p}");
    }
    let dir = std::env::temp_dir().join(format!("vc-dwg-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("plan.DWG").to_string_lossy().to_string();
    let r = s.execute("document.export", &json!({"path": path})).unwrap();
    assert_eq!(r["format"], "dwg");
    assert_eq!(acadver(&std::fs::read(&path).unwrap()), "AC1032");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn dwg_writes_one_drawing_per_artboard() {
    let mut s = two_boards();
    let r = s.execute("document.exportDwg", &json!({"useArtboards": true})).unwrap();
    let files = r["files"].as_array().unwrap();
    assert_eq!(files.len(), 2, "{r}");
    for f in files {
        assert!(f["name"].as_str().unwrap().ends_with(".dwg"), "{f}");
        assert!(vectorcraft_dwg::is_dwg(&b64(f)), "{f}");
    }
    let e = s.execute("document.exportDwg", &json!({"artboards": [0, 1]})).unwrap_err().to_string();
    assert!(e.contains("one origin"), "{e}");
}

#[test]
fn dwg_versions_start_at_2000() {
    let mut s = two_boards();
    for v in ["R12", "R13", "R14", "1999"] {
        let e = s.execute("document.exportDwg", &json!({"version": v})).unwrap_err().to_string();
        assert!(e.contains(&format!("DWG version `{v}`")) && e.contains("2000, 2004"), "{v}: {e}");
    }
    let e = s.execute("document.exportDwg", &json!({"scale": -1})).unwrap_err().to_string();
    assert!(e.contains("DWG scale"), "{e}");
    // The same versions are fine for DXF, and the dialog check tells them apart by format.
    assert!(s.execute("document.exportDxf", &json!({"version": "R12"})).is_ok());
    assert!(dxf::check(&json!({"format": "dxf", "version": "R12"})).is_ok());
    assert!(dxf::check(&json!({"format": "dwg", "version": "R12"})).is_err());
    assert!(dxf::check(&json!({"format": "DWG", "version": "R12"})).is_err(), "any case, as the export takes it");
    assert!(dxf::check(&json!({"format": ".dwg", "version": "R12"})).is_err(), "the extension names it too");
    assert!(dxf::check(&json!({"format": "dwg", "version": "2004"})).is_ok());
}

#[test]
fn dwg_keeps_placed_images_and_says_dwg_in_its_warnings() {
    let mut s = super::tests_svg::image_session();
    let r = s.execute("document.exportDwg", &json!({"version": "2000"})).unwrap();
    assert_eq!(acadver(&b64(&r)), "AC1015");
    assert_eq!(r["linked"].as_array().unwrap().len(), 1, "the image file the drawing links: {r}");
    let warnings: Vec<&str> = r["warnings"].as_array().unwrap().iter().map(|w| w.as_str().unwrap()).collect();
    assert!(!warnings.iter().any(|w| w.contains("left out")), "VectorCraft's DXF is written whole: {warnings:?}");
    // True colour on 2000: the DXF writer's warning, about the DWG.
    assert!(warnings.iter().any(|w| w.contains("true colour") && w.contains("DWG") && !w.contains("DXF")), "{warnings:?}");
}

#[test]
fn what_the_dwg_writer_leaves_out_is_counted_in_words() {
    assert_eq!(dxf::left_out_warning((0, 0), 1), None);
    assert_eq!(dxf::left_out_warning((1, 0), 1).unwrap(), "the DWG writer can't write 1 object of this drawing: left out");
    assert_eq!(dxf::left_out_warning((3, 0), 1).unwrap(), "the DWG writer can't write 3 objects of this drawing: left out");
    assert_eq!(dxf::left_out_warning((0, 1), 1).unwrap(), "the DWG writer can't write 1 non-graphical record of this drawing: left out");
    assert_eq!(
        dxf::left_out_warning((2, 5), 1).unwrap(),
        "the DWG writer can't write 2 objects and 5 non-graphical records of this drawing: left out"
    );
    assert_eq!(dxf::left_out_warning((2, 0), 3).unwrap(), "the DWG writer can't write 2 objects of these 3 drawings: left out");
}

#[test]
fn a_dwg_file_says_it_is_written_not_read() {
    let mut s = two_boards();
    let e = s
        .execute("document.open", &json!({"name": "site.dwg", "dataBase64": vectorcraft_format::base64_encode(b"AC1032\0\0")}))
        .unwrap_err()
        .to_string();
    assert!(e.contains("site.dwg") && e.contains("doesn't read DWG drawings"), "{e}");
    // DWG content under any name, a readable one too (it would reach that reader otherwise), and a
    // `.dwg` name whatever it holds; opening and placing alike.
    for (name, bytes) in [
        ("plan.bin", &b"AC1032\0\0"[..]),
        ("plan.dxf", b"AC1018\0\0"),
        ("plan.png", b"AC1015\0\0"),
        ("plan.docx", b"AC1024\0\0"),
        ("plan.txt", b"AC1027\0\0"),
        ("empty.DWG", b""),
    ] {
        for cmd in ["document.open", "file.place"] {
            let e = s.execute(cmd, &json!({"name": name, "dataBase64": vectorcraft_format::base64_encode(bytes)})).unwrap_err().to_string();
            assert!(e.contains(name) && e.contains("doesn't read DWG drawings") && e.contains("save it as DXF"), "{cmd} {name}: {e}");
        }
    }
    // Other write-only formats keep their own message.
    let e =
        s.execute("document.open", &json!({"name": "notes.txt", "dataBase64": vectorcraft_format::base64_encode(b"hi")})).unwrap_err().to_string();
    assert!(!e.contains("DWG"), "{e}");
}
