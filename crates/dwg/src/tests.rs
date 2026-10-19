// Test helpers outside `#[test]` functions unwrap too (clippy.toml allows it only inside them).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::io::Cursor;

use super::*;

/// A minimal ASCII DXF of `acadver` holding one LINE and one CIRCLE on layer `Art`.
fn dxf(acadver: &str) -> Vec<u8> {
    let pairs: &[(&str, &str)] = &[
        ("0", "SECTION"),
        ("2", "HEADER"),
        ("9", "$ACADVER"),
        ("1", acadver),
        ("0", "ENDSEC"),
        ("0", "SECTION"),
        ("2", "ENTITIES"),
        ("0", "LINE"),
        ("8", "Art"),
        ("10", "0.0"),
        ("20", "0.0"),
        ("30", "0.0"),
        ("11", "100.0"),
        ("21", "50.0"),
        ("31", "0.0"),
        ("0", "CIRCLE"),
        ("8", "Art"),
        ("10", "50.0"),
        ("20", "25.0"),
        ("30", "0.0"),
        ("40", "10.0"),
        ("0", "ENDSEC"),
        ("0", "EOF"),
    ];
    pairs.iter().flat_map(|(c, v)| format!("{c:>3}\r\n{v}\r\n").into_bytes()).collect()
}

/// The entity type names of a DWG file, as acadrust reads it back.
fn read_back(dwg: &[u8]) -> (acadrust::DxfVersion, Vec<String>) {
    let doc = acadrust::DwgReader::from_stream(Cursor::new(dwg.to_vec())).read().unwrap();
    let mut kinds: Vec<String> = doc.entities().map(|e| format!("{e:?}").split(['(', ' ', '{']).next().unwrap_or_default().to_string()).collect();
    kinds.sort();
    (doc.version, kinds)
}

#[test]
fn writes_the_version_the_dxf_declares() {
    for (id, acadver) in VERSIONS {
        let out = from_dxf(&dxf(acadver)).unwrap_or_else(|e| panic!("{id}: {e}"));
        assert!(is_dwg(&out.bytes), "{id}");
        assert_eq!(out.bytes.get(0..6), Some(acadver.as_bytes()), "{id}");
        assert_eq!((out.left_out, out.left_out_objects), (0, 0), "{id}");
        let (version, kinds) = read_back(&out.bytes);
        assert_eq!(format!("{version:?}"), *acadver, "{id}");
        assert_eq!(kinds, ["Circle", "Line"], "{id}");
    }
}

/// [`dxf`] of AC1032 with one entity of a type no DXF reader knows in ENTITIES and one inside a
/// BLOCK.
fn dxf_with_unknown_entities() -> Vec<u8> {
    let pairs: &[(&str, &str)] = &[
        ("0", "SECTION"),
        ("2", "HEADER"),
        ("9", "$ACADVER"),
        ("1", "AC1032"),
        ("0", "ENDSEC"),
        ("0", "SECTION"),
        ("2", "BLOCKS"),
        ("0", "BLOCK"),
        ("8", "0"),
        ("2", "Mark"),
        ("70", "0"),
        ("10", "0.0"),
        ("20", "0.0"),
        ("30", "0.0"),
        ("3", "Mark"),
        ("0", "ZZ_MADE_UP"),
        ("8", "0"),
        ("10", "1.0"),
        ("20", "1.0"),
        ("0", "LINE"),
        ("8", "0"),
        ("10", "0.0"),
        ("20", "0.0"),
        ("11", "5.0"),
        ("21", "5.0"),
        ("0", "ENDBLK"),
        ("8", "0"),
        ("0", "ENDSEC"),
        ("0", "SECTION"),
        ("2", "ENTITIES"),
        ("0", "ZZ_MADE_UP"),
        ("8", "Art"),
        ("10", "2.0"),
        ("20", "2.0"),
        ("0", "LINE"),
        ("8", "Art"),
        ("10", "0.0"),
        ("20", "0.0"),
        ("11", "100.0"),
        ("21", "50.0"),
        ("0", "ENDSEC"),
        ("0", "EOF"),
    ];
    pairs.iter().flat_map(|(c, v)| format!("{c:>3}\r\n{v}\r\n").into_bytes()).collect()
}

#[test]
fn unknown_entities_are_left_out_in_blocks_too() {
    let out = from_dxf(&dxf_with_unknown_entities()).unwrap();
    assert_eq!(out.left_out, 2, "the one in ENTITIES and the one in the block");
    let (_, kinds) = read_back(&out.bytes);
    assert!(kinds.iter().filter(|k| *k == "Line").count() >= 1, "{kinds:?}");
    assert!(!kinds.iter().any(|k| k.contains("Unknown")), "{kinds:?}");
}

#[test]
fn older_dxf_versions_are_refused() {
    for acadver in ["AC1009", "AC1012", "AC1014"] {
        let e = from_dxf(&dxf(acadver)).unwrap_err();
        assert!(e.contains("2000 or later"), "{acadver}: {e}");
    }
}

#[test]
fn garbage_is_an_error_not_a_crash() {
    assert!(from_dxf(b"").is_err());
    assert!(from_dxf(b"this is not a DXF file").is_err());
    assert!(from_dxf(&dxf("AC1032")[..40]).is_err());
}

#[test]
fn detects_magic() {
    assert!(is_dwg(b"AC1032\0\0\0\0"));
    assert!(!is_dwg(b"  0\r\nSECTION"));
    assert!(!is_dwg(b"AC10"));
    assert!(is_dwg(b"AC1015"), "the six-byte signature alone");
}
