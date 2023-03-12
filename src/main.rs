use std::path::{Path, PathBuf};

use clap::Parser;
use glyphsinfo_rs::{self, GlyphData};
use structs::Fontgarden;

mod errors;
mod structs;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// UFO to import and write to /tmp.
    source: PathBuf,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    let ufo_source = norad::Font::load(&cli.source)?;
    let style_name = ufo_source
        .font_info
        .style_name
        .as_ref()
        .map(|s| s.to_string())
        .unwrap_or(String::from("Regular"));

    let mut fontgarden = Fontgarden::new();
    let glyph_info = glyphsinfo_rs::GlyphData::default();

    for layer in ufo_source.iter_layers() {
        let layer_name = if std::ptr::eq(layer, ufo_source.layers.default_layer()) {
            style_name.clone()
        } else if layer.name() == &"public.background" {
            format!("{}.{}", &style_name, "background")
        } else {
            format!("{}.{}", &style_name, layer.name())
        };

        for glyph in layer.iter() {
            let mut fontgarden_glyph = fontgarden
                .glyphs
                .entry(glyph.name().to_string())
                .or_default();

            fontgarden_glyph.codepoints = glyph.codepoints.clone();
            fontgarden_glyph.set = categorize_glyph(glyph, &glyph_info);
            let fontgarden_layer: structs::Layer = glyph.into();
            fontgarden_glyph
                .layers
                .insert(layer_name.clone(), fontgarden_layer);
        }
    }

    if let Some(names) = ufo_source
        .lib
        .get("public.postscriptNames")
        .and_then(|v| v.as_dictionary())
    {
        for (glyph, name) in names.iter() {
            fontgarden
                .glyphs
                .entry(glyph.to_string())
                .and_modify(|g| g.postscript_name = name.as_string().map(|n| n.to_string()));
        }
    }

    if let Some(names) = ufo_source
        .lib
        .get("public.openTypeCategories")
        .and_then(|v| v.as_dictionary())
    {
        for (glyph, name) in names.iter() {
            fontgarden.glyphs.entry(glyph.to_string()).and_modify(|g| {
                g.opentype_category = name
                    .as_string()
                    .map(|n| n.parse().unwrap_or_default())
                    .unwrap_or_default()
            });
        }
    }

    let file_name = Path::new("/tmp").join(cli.source.file_name().unwrap());
    fontgarden.save(&file_name)?;

    Ok(())
}

fn categorize_glyph(glyph: &norad::Glyph, glyph_info: &GlyphData) -> Option<String> {
    if let Some(unicode) = glyph.codepoints.iter().next() {
        return glyph_info
            .record_for_unicode(unicode)
            .and_then(|record| record.script.as_ref().map(|s| format!("{s:?}")));
    }
    if let Some(record) = glyph_info.record_for_name(glyph.name()) {
        return record.script.as_ref().map(|s| format!("{s:?}"));
    }
    if let Some((base_name, _)) = glyph.name().split_once('.') {
        return glyph_info
            .record_for_name(base_name)
            .and_then(|record| record.script.as_ref().map(|s| format!("{s:?}")));
    }
    None
}
