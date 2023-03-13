use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use clap::Parser;
use errors::SourceLoadError;
use glyphsinfo_rs::{self, GlyphData};
use structs::Fontgarden;

mod errors;
mod structs;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// Sources to import and write to /tmp.
    #[arg(num_args = 1..)]
    sources: Vec<PathBuf>,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    let sources = load_sources(&cli.sources)?;
    let default_source = match sources.get("Regular") {
        Some(font) => font,
        None => sources.values().next().unwrap(),
    };

    let mut fontgarden = Fontgarden::new();
    let glyph_info = glyphsinfo_rs::GlyphData::default();

    for (source_name, source) in &sources {
        for layer in source.iter_layers() {
            let layer_name = if std::ptr::eq(layer, source.layers.default_layer()) {
                source_name.clone()
            } else if layer.name() == &"public.background" {
                format!("{}.{}", &source_name, "background")
            } else {
                format!("{}.{}", &source_name, layer.name())
            };

            for glyph in layer.iter() {
                let mut fontgarden_glyph = fontgarden
                    .glyphs
                    .entry(glyph.name().to_string())
                    .or_default();

                if std::ptr::eq(source, default_source) {
                    fontgarden_glyph.codepoints = glyph.codepoints.clone();
                    fontgarden_glyph.set = categorize_glyph(glyph, &glyph_info);
                }
                let fontgarden_layer: structs::Layer = glyph.into();
                fontgarden_glyph
                    .layers
                    .insert(layer_name.clone(), fontgarden_layer);
            }
        }
    }

    if let Some(names) = default_source
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

    if let Some(names) = default_source
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

    let file_name = Path::new("/tmp/font.fontgarden");
    fontgarden.save(file_name)?;

    Ok(())
}

fn load_sources(sources: &[PathBuf]) -> Result<HashMap<String, norad::Font>, SourceLoadError> {
    let mut source_by_name = HashMap::new();
    for source_path in sources {
        let ufo_source = norad::Font::load(source_path)
            .map_err(|e| SourceLoadError::Ufo(source_path.clone(), e))?;
        let source_name = ufo_source
            .font_info
            .style_name
            .as_ref()
            .map(|s| s.to_string())
            .unwrap_or(String::from("Regular"));
        if source_by_name.contains_key(&source_name) {
            return Err(SourceLoadError::DuplicateLayerName(
                source_name,
                source_path.clone(),
            ));
        }
        source_by_name.insert(source_name, ufo_source);
    }
    Ok(source_by_name)
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
    // FIXME: This also categorizes danda-deva.loclBENG as Devanagari because the parent
    // is. Local variants should stay with their scripts if possible.
    if let Some((base_name, _)) = glyph.name().split_once('.') {
        return glyph_info
            .record_for_name(base_name)
            .and_then(|record| record.script.as_ref().map(|s| format!("{s:?}")));
    }
    None
}
