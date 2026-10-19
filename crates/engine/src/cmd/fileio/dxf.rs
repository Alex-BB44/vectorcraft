//! DXF and DWG export for every path (`document.export {format: dxf|dwg}`, `document.exportDxf`,
//! `document.exportDwg`, Export As → DXF/DWG Options): the options parsed from the params, one
//! drawing per artboard with Use Artboards, and the formats that can't be written (PICT) with what
//! to use instead. DWG is the DXF drawing converted by `vectorcraft-dwg` (2000 and later).

use std::sync::Arc;

use serde::Deserialize;
use serde_json::{Value, json};
use vectorcraft_cad::{ColorDepth, DxfOptions, DxfVersion, Preserve, RasterFormat};
use vectorcraft_doc::{Document, Unit};
use vectorcraft_geom::Rect;

use super::super::*;
use super::{ArtboardPick, Encoded, FormatOption};

const C: &str = "document.export";

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(
            "document.exportDxf",
            "Export DXF",
            [],
            None,
            "{path?, version?: R12|R13|R14|2000|2004|2007|2010|2013|2018 (default), unit?: mm (default; any unit name: pt, in, cm…), scale?: 1 (drawing units per unit), scaleLineweights?: false, colors?: 8|16|256|true (default; true colour needs 2004+, else 256), rasterFormat?: png (default)|jpeg (placed images, written next to the file), preserve?: appearance (default: type as outlines; strokes a CAD line can't draw as filled outlines; brush art)|editability (type as text; every stroke a line with its lineweight and dashes), alterPaths?: false (every stroke as its filled outline), outlineText?: false, selectedOnly?: false (the selected objects, in their layers), artboard?: 0 (the origin is its bottom-left corner), useArtboards?: true (one drawing per chosen artboard, artboards?/range?, holding the art over it: {stem}-{artboard}.dxf) | false (the origin at the art's bounds)} → {path, bytes, warnings, files?, linked?: [image files]}; no path → {dataBase64, …, linked?: [{name, dataBase64}]}. Layers become DXF layers (hidden: off; locked; non-printing: not plotted); paths become polylines and splines, fills their outlines (which laser and cutter software reads) and solid hatches. Same as document.export {format: dxf}",
            has_doc,
            export_dxf
        ),
        cmd!(
            "document.exportDwg",
            "Export DWG",
            [],
            None,
            "{path?, version?: 2000|2004|2007|2010|2013|2018 (default), …the other options of document.exportDxf} → as document.exportDxf. The drawing document.exportDxf writes, as a DWG file (desktop only: the web app answers an error); objects DWG can't hold in that version are left out, with a warning. Same as document.export {format: dwg}",
            has_doc,
            export_dwg
        ),
    ]
}

/// The DXF options `document.formats` lists.
pub(super) const OPTIONS: &[FormatOption] = &[
    super::ARTBOARD,
    super::ARTBOARDS,
    super::RANGE,
    super::USE_ARTBOARDS,
    FormatOption { name: "version", ty: "string", default: "\"2018\"", description: "R12, R13, R14, 2000, 2004, 2007, 2010, 2013 or 2018" },
    UNIT,
    SCALE,
    SCALE_LINEWEIGHTS,
    COLORS,
    RASTER_FORMAT,
    PRESERVE,
    ALTER_PATHS,
    OUTLINE_TEXT,
    SELECTED_ONLY,
];

/// The DWG options `document.formats` lists: the DXF ones, from version 2000.
pub(super) const DWG_OPTIONS: &[FormatOption] = &[
    super::ARTBOARD,
    super::ARTBOARDS,
    super::RANGE,
    super::USE_ARTBOARDS,
    FormatOption { name: "version", ty: "string", default: "\"2018\"", description: DWG_VERSIONS },
    UNIT,
    SCALE,
    SCALE_LINEWEIGHTS,
    COLORS,
    RASTER_FORMAT,
    PRESERVE,
    ALTER_PATHS,
    OUTLINE_TEXT,
    SELECTED_ONLY,
];

/// The versions DWG is written as (`vectorcraft_dwg::VERSIONS`).
const DWG_VERSIONS: &str = "2000, 2004, 2007, 2010, 2013 or 2018";

const UNIT: FormatOption = FormatOption {
    name: "unit",
    ty: "string",
    default: "\"mm\"",
    description: "the art unit that `scale` drawing units stand for (pt, in, mm, cm, m, ft…)",
};
const SCALE: FormatOption = FormatOption { name: "scale", ty: "number", default: "1", description: "drawing units per unit (1 mm = 1 unit)" };
const SCALE_LINEWEIGHTS: FormatOption =
    FormatOption { name: "scaleLineweights", ty: "boolean", default: "false", description: "lineweights grow with the scale" };
const COLORS: FormatOption = FormatOption {
    name: "colors",
    ty: "string",
    default: "\"true\"",
    description: "8, 16 or 256 indexed colours, or true colour (version 2004 and later)",
};
const RASTER_FORMAT: FormatOption = FormatOption {
    name: "rasterFormat",
    ty: "string",
    default: "\"png\"",
    description: "png or jpeg: the files placed images are written to, next to the drawing",
};
const PRESERVE: FormatOption = FormatOption {
    name: "preserve",
    ty: "string",
    default: "\"appearance\"",
    description: "appearance (type as outlines, strokes a CAD line can't draw as filled outlines) or editability (type as text, strokes as lines)",
};
const ALTER_PATHS: FormatOption = FormatOption {
    name: "alterPaths",
    ty: "boolean",
    default: "false",
    description: "Alter Paths for Appearance: every stroke as its filled outline",
};
const OUTLINE_TEXT: FormatOption =
    FormatOption { name: "outlineText", ty: "boolean", default: "false", description: "type as glyph outlines (always with preserve: appearance)" };
const SELECTED_ONLY: FormatOption =
    FormatOption { name: "selectedOnly", ty: "boolean", default: "false", description: "only the selected objects, in their layers" };

/// A format that can be neither opened nor written, and what to use instead.
#[derive(Clone, Copy, Debug)]
pub struct Unsupported {
    pub id: &'static str,
    pub label: &'static str,
    /// Lower-case extensions without the dot.
    pub extensions: &'static [&'static str],
    pub hint: &'static str,
}

/// Formats people ask for that VectorCraft doesn't read or write. An entry leaves when VectorCraft
/// comes to write or read its format (DWG did).
pub const UNSUPPORTED: &[Unsupported] = &[Unsupported {
    id: "pict",
    label: "PICT",
    extensions: &["pict", "pct", "pic"],
    hint: "PICT is a retired format and is not supported: convert the file to PDF or PNG first",
}];

/// The unsupported format an id or extension (any case, leading dot allowed) names.
pub fn unsupported(id_or_ext: &str) -> Option<&'static Unsupported> {
    let k = id_or_ext.trim_start_matches('.').to_ascii_lowercase();
    UNSUPPORTED.iter().find(|u| u.id == k || u.extensions.contains(&k.as_str()))
}

impl Unsupported {
    pub fn to_json(&self) -> Value {
        json!({ "id": self.id, "label": self.label, "extensions": self.extensions, "hint": self.hint })
    }
}

/// The DXF params, as they come (each checked when read).
#[derive(Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct Params {
    #[serde(flatten)]
    boards: ArtboardPick,
    version: Option<Value>,
    unit: Option<String>,
    scale: Option<f64>,
    scale_lineweights: Option<bool>,
    colors: Option<Value>,
    raster_format: Option<String>,
    preserve: Option<String>,
    alter_paths: Option<bool>,
    outline_text: Option<bool>,
}

/// A string or number param as text (`2018` and `"2018"` alike).
pub(crate) fn text(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// `field` of a `kind` (DXF or DWG) export read by `read`, or an error listing what it may be.
fn choice<T>(kind: &str, field: &str, v: Option<String>, read: impl Fn(&str) -> Option<T>, default: T, allowed: &str) -> Result<T> {
    match v {
        None => Ok(default),
        Some(s) => read(&s).ok_or_else(|| bad(C, format!("{kind} {field} `{s}`: {allowed}"))),
    }
}

/// The DXF options in `p` (the region and crop are set per drawing) and the artboard choice; with
/// `dwg`, the versions DWG is written as.
fn parse(p: &Value, dwg: bool) -> Result<(DxfOptions, ArtboardPick)> {
    let kind = if dwg { "DWG" } else { "DXF" };
    let q: Params = if p.is_object() { Params::deserialize(p).map_err(|e| bad(C, format!("{kind} options: {e}")))? } else { Params::default() };
    let d = DxfOptions::default();
    let scale = q.scale.unwrap_or(d.scale);
    if !(scale.is_finite() && vectorcraft_cad::SCALE_RANGE.contains(&scale)) {
        return Err(bad(C, format!("{kind} scale must be a positive number up to {}, not {scale}", vectorcraft_cad::SCALE_RANGE.end())));
    }
    let version = if dwg {
        let read = |s: &str| DxfVersion::from_id(s).filter(|v| *v >= DxfVersion::R2000);
        choice(kind, "version", q.version.as_ref().map(text), read, d.version, DWG_VERSIONS)?
    } else {
        choice(kind, "version", q.version.as_ref().map(text), DxfVersion::from_id, d.version, "R12, R13, R14, 2000, 2004, 2007, 2010, 2013 or 2018")?
    };
    let opts = DxfOptions {
        version,
        unit: choice(kind, "unit", q.unit, Unit::named, d.unit, "a unit such as mm, cm, in or pt")?,
        scale,
        scale_lineweights: q.scale_lineweights.unwrap_or(d.scale_lineweights),
        colors: choice(kind, "colors", q.colors.as_ref().map(text), ColorDepth::from_id, d.colors, "8, 16, 256 or true")?,
        raster: choice(kind, "rasterFormat", q.raster_format, RasterFormat::from_id, d.raster, "png or jpeg")?,
        preserve: choice(kind, "preserve", q.preserve, Preserve::from_id, d.preserve, "appearance or editability")?,
        alter_paths: q.alter_paths.unwrap_or(d.alter_paths),
        outline_text: q.outline_text.unwrap_or(d.outline_text),
        ..d
    };
    Ok((opts, q.boards))
}

/// Check the DXF options in `p` without exporting (dialogs keep themselves open on an error);
/// `format: dwg` checks them as DWG options.
pub fn check(p: &Value) -> std::result::Result<(), String> {
    let dwg = p.get("format").and_then(Value::as_str).and_then(super::format).is_some_and(|f| f.id == "dwg");
    parse(p, dwg).map(|_| ()).map_err(|e| e.to_string())
}

/// Encode `doc` as DXF, or with `dwg` as DWG: one drawing whose origin is the chosen artboard's
/// bottom-left corner (default the first), or with `use_artboards` one drawing per chosen artboard
/// (default all) holding the art over it.
pub(super) fn encode(doc: &Document, p: &Value, use_artboards: Option<bool>, dwg: bool) -> Result<Encoded> {
    let (opts, pick) = parse(p, dwg)?;
    let n = doc.artboards.len();
    let rect = |b: usize| doc.artboards.get(b).map(|a| a.rect).ok_or_else(|| bad(C, format!("no artboard {}", b + 1)));
    let drawings: Vec<(Option<usize>, Rect, bool)> = match use_artboards {
        Some(true) => {
            let boards = pick.resolve(n).map_err(|e| bad(C, e))?.unwrap_or_else(|| (0..n).collect());
            boards.into_iter().map(|b| rect(b).map(|r| (Some(b), r, true))).collect::<Result<_>>()?
        }
        _ if n == 0 => vec![(None, doc.art_bounds().unwrap_or(Rect::ZERO), false)],
        _ => {
            let b = match pick.resolve(n).map_err(|e| bad(C, e))?.as_deref() {
                None => 0,
                Some([b]) => *b,
                Some(_) => return Err(bad(C, "a DXF or DWG drawing has one origin: pass useArtboards: true for one drawing per artboard")),
            };
            vec![(Some(b), rect(b)?, false)]
        }
    };
    let mut enc = Encoded::default();
    // What the DWG writer left out, over every drawing: (entities, other objects).
    let mut left_out = (0, 0);
    for (board, region, crop) in drawings {
        let out = vectorcraft_cad::export(doc, &DxfOptions { region, crop, ..opts.clone() }).map_err(|e| bad(C, e))?;
        let mut warnings = out.warnings;
        let bytes = if dwg {
            let d = vectorcraft_dwg::from_dxf(&out.bytes).map_err(|e| bad(C, e))?;
            left_out.0 += d.left_out;
            left_out.1 += d.left_out_objects;
            // The DXF writer's version warnings ("true colour needs DXF 2004 or later") hold for
            // the DWG made from it; only that phrase is reworded, never names a warning quotes.
            warnings = warnings.into_iter().map(|w| w.replacen("needs DXF ", "needs DWG ", 1)).collect();
            d.bytes
        } else {
            out.bytes
        };
        enc.files.push((board, bytes));
        for w in warnings {
            if !enc.warnings.contains(&w) {
                enc.warnings.push(w);
            }
        }
        for img in out.images {
            if !enc.linked.iter().any(|l| l.name == img.name) {
                enc.linked.push(vectorcraft_svg::LinkedImage { name: img.name, bytes: Arc::new(img.bytes) });
            }
        }
    }
    if enc.files.is_empty() {
        return Err(bad(C, "the document has no artboard"));
    }
    if let Some(w) = left_out_warning(left_out, enc.files.len()) {
        enc.warnings.push(w);
    }
    Ok(enc)
}

/// The warning for what the DWG writer left out of `drawings` files: (entities, other objects, in
/// all of them), else none.
pub(super) fn left_out_warning((entities, objects): (usize, usize), drawings: usize) -> Option<String> {
    let n = |k: usize, one: &str, many: &str| format!("{k} {}", if k == 1 { one } else { many });
    let what = match (entities, objects) {
        (0, 0) => return None,
        (e, 0) => n(e, "object", "objects"),
        (0, o) => n(o, "non-graphical record", "non-graphical records"),
        (e, o) => format!("{} and {}", n(e, "object", "objects"), n(o, "non-graphical record", "non-graphical records")),
    };
    let of = if drawings > 1 { format!("these {drawings} drawings") } else { "this drawing".to_string() };
    Some(format!("the DWG writer can't write {what} of {of}: left out"))
}

fn export_dxf(s: &mut Session, p: &Value) -> Result<Value> {
    export_as(s, p, "dxf")
}

fn export_dwg(s: &mut Session, p: &Value) -> Result<Value> {
    export_as(s, p, "dwg")
}

fn export_as(s: &mut Session, p: &Value, format: &str) -> Result<Value> {
    let mut q = if p.is_object() { p.clone() } else { json!({}) };
    q["format"] = json!(format);
    super::export::export(s, &q)
}
