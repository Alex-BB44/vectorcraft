//! File → Export → Export As…: the format and Use Artboards (All or a range: one file per
//! artboard, or one page each in a PDF); without artboards the export covers the visible art. OK
//! picks the file, then the format's options dialog follows (PNG/JPEG/WebP/PNG-8/GIF Options, SVG
//! Options, Text Export Options, DXF and DWG Options, EPS Options, and Save PDF for artboards as
//! pages), else the file is written.

use std::sync::LazyLock;

use serde_json::{Value, json};
use vectorcraft_engine::cmd::fileio::{self, ArtboardPick, Format};

use super::{DialogSpec, dxf_options, eps_options, form, png_options, save_pdf, svg_options, text_export};
use crate::state::Dialog;
use crate::theme::Tokens;
use crate::{VectorcraftApp, io, widgets};

pub(super) const SPEC: DialogSpec =
    DialogSpec { heading: |_| tl!("Export As").into(), body, confirm, ok: Some("Export…"), min_width: 380.0, ..DialogSpec::FORM };

/// The formats Export As writes (the native format and `.ai` are Save As's).
static FORMATS: LazyLock<Vec<&'static Format>> =
    LazyLock::new(|| fileio::FORMATS.iter().filter(|f| f.write && !matches!(f.id, "vectorcraft" | "ai")).collect());
static LABELS: LazyLock<Vec<&'static str>> = LazyLock::new(|| FORMATS.iter().map(|f| f.label).collect());

/// Open the dialog on `format` (an id or extension, default PNG); `p` may preset `useArtboards`
/// and `range`.
pub fn open(app: &mut VectorcraftApp, format: Option<&str>, p: &Value) {
    let n = app.session.active().map_or(0, |s| s.doc.artboards.len());
    // A format this build can't write (DWG in the web app) stays named, with why, rather than
    // silently becoming PNG.
    let format = format.and_then(fileio::format).filter(|f| FORMATS.iter().any(|g| g.id == f.id) || not_here(f).is_some()).map_or("png", |f| f.id);
    let range = p.get("range").and_then(Value::as_str);
    app.ui.dialog = Some(Dialog::new(
        "exportAs",
        json!({
            "format": format,
            "useArtboards": p.get("useArtboards").and_then(Value::as_bool).unwrap_or(false),
            "all": range.is_none(),
            "range": range.map_or_else(|| if n > 1 { format!("1-{n}") } else { "1".into() }, str::to_string),
        }),
    ));
}

/// Why `f` can't be written in this build, when it is a format VectorCraft writes elsewhere (DWG
/// in the web app).
fn not_here(f: &Format) -> Option<String> {
    (!f.write && !f.read).then(|| fileio::writable_format(Some(f.id), None).err()).flatten()
}

/// The format the field names that can't be written here, and why.
fn unavailable(d: &Dialog) -> Option<(&'static str, String)> {
    let f = fileio::format(&d.str("format"))?;
    Some((f.label, not_here(f)?))
}

/// The chosen format (PNG when the field names none Export As writes).
fn chosen(d: &Dialog) -> Option<&'static Format> {
    let id = d.str("format");
    let find = |id: &str| FORMATS.iter().copied().find(|f| f.id == id);
    find(&id).or_else(|| find("png"))
}

fn body(_: &mut VectorcraftApp, ui: &mut egui::Ui, d: &mut Dialog) -> bool {
    let t = Tokens::get(ui.ctx());
    let Some(f) = chosen(d) else { return false };
    let missing = unavailable(d);
    egui::Grid::new("export-as").num_columns(2).spacing([10.0, 8.0]).show(ui, |ui| {
        ui.label(egui::RichText::new(tl!("Format:")).color(t.text_dim));
        let shown = missing.as_ref().map_or(f.label, |(label, _)| label);
        if let Some(g) = widgets::dropdown(ui, "export-as-format", shown, &LABELS, 160.0).and_then(|i| FORMATS.get(i)) {
            d.fields.insert("format".into(), json!(g.id));
        }
        ui.end_row();
    });
    if let Some((_, why)) = &missing {
        ui.add_space(6.0);
        form::caption(ui, why);
    }
    ui.add_space(10.0);
    form::check(ui, d, "useArtboards", tl!("Use Artboards"));
    let use_artboards = d.bool("useArtboards");
    ui.add_enabled_ui(use_artboards, |ui| {
        ui.horizontal(|ui| {
            ui.add_space(22.0);
            let mut all = d.bool("all");
            if ui.radio_value(&mut all, true, tl!("All")).changed() | ui.radio_value(&mut all, false, tl!("Range:")).changed() {
                d.fields.insert("all".into(), json!(all));
            }
            let mut range = d.str("range");
            if ui.add_enabled(!all, egui::TextEdit::singleline(&mut range).desired_width(90.0)).changed() {
                d.fields.insert("range".into(), json!(range));
            }
        });
    });
    ui.add_space(8.0);
    let ext = f.extensions[0];
    let hint = match (use_artboards, f.id) {
        (true, "pdf") => tl!("One page per artboard.").to_string(),
        (true, _) => crate::i18n::fmt(tl!("One file per artboard: <name>-<artboard>.{ext}"), &[("ext", ext)]),
        (false, _) => tl!("The bounds of the visible art.").to_string(),
    };
    ui.label(egui::RichText::new(hint).color(t.text_dim).size(11.5));
    false
}

/// Check the artboard range, pick the file, then show the format's options or write the file.
fn confirm(app: &mut VectorcraftApp, d: &Dialog) -> Result<Value, String> {
    if let Some((_, why)) = unavailable(d) {
        return Err(why);
    }
    let f = chosen(d).ok_or("no format to export to")?;
    let use_artboards = d.bool("useArtboards");
    let all = d.bool("all");
    let range = d.str("range");
    if use_artboards && !all {
        // A bad range keeps the dialog open.
        let n = app.session.active().map_or(0, |s| s.doc.artboards.len());
        ArtboardPick { range: Some(range.clone()), ..Default::default() }.resolve(n)?;
    }
    // Nothing to export keeps the dialog open too (before the save dialog, not after it).
    if !use_artboards && app.session.active().is_some_and(|s| vectorcraft_render::encode::art_bounds(&s.doc).is_none()) {
        return Err("nothing to export: the document has no visible art (or turn on Use Artboards)".into());
    }
    app.ui.dialog = None;
    let path = io::target_path(app, None, f.extensions[0])?;
    let range = if all { "all".to_string() } else { range };
    match f.id {
        "svg" | "svgz" => {
            svg_options::open(app, svg_options::Mode::Export, Some(&path));
            if let Some(o) = app.ui.dialog.as_mut() {
                o.fields.extend([("useArtboards".into(), json!(use_artboards)), ("allArtboards".into(), json!(all)), ("range".into(), json!(range))]);
            }
            Ok(Value::Null)
        }
        // Artboards as pages go on in Save PDF; the art's bounds are written straight away.
        "pdf" if use_artboards => save_pdf::open(app, &json!({ "path": path, "range": (!all).then_some(range) })),
        "txt" => {
            text_export::open(app, &path);
            Ok(Value::Null)
        }
        _ => {
            let mut params = json!({ "format": f.id, "path": path, "useArtboards": use_artboards });
            if use_artboards {
                params["range"] = json!(range);
            }
            if f.raster {
                png_options::open(app, f, params);
                return Ok(Value::Null);
            }
            if matches!(f.id, "dxf" | "dwg") {
                dxf_options::open(app, &params);
                return Ok(Value::Null);
            }
            if f.id == "eps" {
                eps_options::open(app, &params);
                return Ok(Value::Null);
            }
            io::export(app, Some(f.id), Some(path), &params)
        }
    }
}
