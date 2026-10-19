//! DWG export through a DXF bridge.
//!
//! VectorCraft writes a DWG file by writing the drawing as DXF (`vectorcraft-cad`, every DXF option
//! applies) and converting those bytes with the `acadrust` crate (MPL-2.0, used unmodified as a
//! dependency), the bridge CADCraft's `crates/dwg` uses. Keeping the dependency behind this
//! byte-level API isolates it: nothing else in VectorCraft depends on its types. Native targets
//! only (acadrust memory-maps files); the web build answers an error.
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

/// The DWG versions written: the DXF version id that picks each (`vectorcraft_cad::DxfVersion`
/// ids) and its `$ACADVER` code. DWG starts at 2000: acadrust doesn't write older DWG.
pub const VERSIONS: &[(&str, &str)] =
    &[("2000", "AC1015"), ("2004", "AC1018"), ("2007", "AC1021"), ("2010", "AC1024"), ("2013", "AC1027"), ("2018", "AC1032")];

/// True when the bytes start like a DWG file (`AC10xx`).
pub fn is_dwg(bytes: &[u8]) -> bool {
    bytes.len() >= 6 && bytes.starts_with(b"AC10") && bytes.get(4..6).is_some_and(|v| v.iter().all(u8::is_ascii_digit))
}

/// A DWG file and what the conversion left out.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Dwg {
    pub bytes: Vec<u8>,
    /// Entities of the DXF the DWG writer can't write (types acadrust doesn't know), left out.
    pub left_out: usize,
    /// Non-graphical objects of the DXF left out for the same reason (dictionaries, records).
    pub left_out_objects: usize,
}

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use std::io::Cursor;
    use std::panic::AssertUnwindSafe;

    use super::Dwg;

    /// Oldest DWG version acadrust writes.
    const MIN: acadrust::DxfVersion = acadrust::DxfVersion::AC1015;

    /// Convert DXF bytes into a DWG file of the version the DXF declares (`$ACADVER`), 2000 or
    /// later.
    pub fn from_dxf(dxf: &[u8]) -> Result<Dwg, String> {
        // acadrust is third-party code: a panic anywhere in it (reading, the clean-up, writing) is
        // an error here, never a crash.
        std::panic::catch_unwind(AssertUnwindSafe(|| convert(dxf))).map_err(|_| "the DXF→DWG conversion failed".to_string())?
    }

    fn convert(dxf: &[u8]) -> Result<Dwg, String> {
        let mut doc = acadrust::DxfReader::from_reader(Cursor::new(dxf.to_vec())).and_then(|r| r.read()).map_err(|e| format!("DXF→DWG: {e}"))?;
        if doc.version < MIN {
            return Err(format!("DWG is written as version 2000 or later, not {:?}", doc.version));
        }
        let (left_out, left_out_objects) = drop_unwritable(&mut doc);
        let bytes = acadrust::DwgWriter::write_to_vec(&doc).map_err(|e| format!("DWG: {e}"))?;
        Ok(Dwg { bytes, left_out, left_out_objects })
    }

    /// Leave out what acadrust can't write: entities and objects it read from the DXF without
    /// knowing their type carry no DWG data to write (read from DXF, `raw_dwg_data` is always
    /// `None`), and acadrust 0.6.4 refuses the whole file for one of them. The limit is the
    /// writer's, not DWG's. `entities()` is acadrust's flat list of every entity, those inside
    /// block definitions included (test `unknown_entities_are_left_out_in_blocks_too`). Returns how
    /// many (entities, objects). References to a removed object
    /// (a dictionary entry) are not followed; VectorCraft's DXF has no objects acadrust doesn't
    /// know, so this guards a future DXF feature rather than a case that occurs today.
    fn drop_unwritable(doc: &mut acadrust::CadDocument) -> (usize, usize) {
        let version = doc.version;
        let entities: Vec<acadrust::Handle> = doc
            .entities()
            .filter_map(|e| match e {
                acadrust::entities::EntityType::Unknown(u) if u.raw_dwg_data.is_none() || u.dwg_source_version.is_some_and(|v| v != version) => {
                    Some(u.common.handle)
                }
                _ => None,
            })
            .collect();
        for h in &entities {
            doc.remove_entity(*h);
        }
        let objects: Vec<acadrust::Handle> = doc
            .objects
            .iter()
            .filter_map(|(h, o)| match o {
                acadrust::objects::ObjectType::Unknown { raw_dwg_data, raw_dwg_version, .. }
                    if raw_dwg_data.is_none() || raw_dwg_version.is_some_and(|v| v != version) =>
                {
                    Some(*h)
                }
                _ => None,
            })
            .collect();
        for h in &objects {
            doc.objects.remove(h);
        }
        (entities.len(), objects.len())
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub use native::from_dxf;

#[cfg(target_arch = "wasm32")]
pub fn from_dxf(_dxf: &[u8]) -> Result<Dwg, String> {
    Err("DWG can't be written in the web app: export DXF, or DWG from the desktop app".into())
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests;
