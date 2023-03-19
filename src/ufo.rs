use std::{collections::HashMap, path::PathBuf};

use glyphsinfo_rs::GlyphData;

use crate::{
    errors::SourceLoadError,
    structs::{Fontgarden, Layer},
};

impl Fontgarden {
    pub fn import_ufo_sources(&mut self, sources: &[PathBuf]) -> Result<(), SourceLoadError> {
        let sources = load_sources(sources)?;
        let default_source = match sources.get("Regular") {
            Some(font) => font,
            None => sources.values().next().unwrap(),
        };

        let glyph_info = glyphsinfo_rs::GlyphData::default();

        // Todo: Remember which glyphs are present in a fontgarden already to only guess the
        // set of new arrivals.

        for (source_name, source) in &sources {
            for layer in source.iter_layers() {
                // Todo: think of another char or way to separate main from subordinate
                // layer, as '.' might be legitimately be used in a layer name.
                let layer_name = if std::ptr::eq(layer, source.layers.default_layer()) {
                    source_name.clone()
                } else if layer.name() == &"public.background" {
                    format!("{}.{}", &source_name, "background")
                } else {
                    format!("{}.{}", &source_name, layer.name())
                };

                for glyph in layer.iter() {
                    let mut fontgarden_glyph =
                        self.glyphs.entry(glyph.name().to_string()).or_default();

                    // Try and source codepoints for a glyph from the default source. Also
                    // try to guess which script (for set-determining purposes) a glyph
                    // belongs to, if it doesn't belong to one yet.
                    if std::ptr::eq(source, default_source)
                        && std::ptr::eq(layer, default_source.layers.default_layer())
                    {
                        fontgarden_glyph.codepoints = glyph.codepoints.clone();
                        if fontgarden_glyph.set.is_none() {
                            fontgarden_glyph.set = categorize_glyph(glyph, &glyph_info);
                        }
                    }
                    let fontgarden_layer: Layer = glyph.into();
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
                self.glyphs
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
                self.glyphs.entry(glyph.to_string()).and_modify(|g| {
                    g.opentype_category = name
                        .as_string()
                        .map(|n| n.parse().unwrap_or_default())
                        .unwrap_or_default()
                });
            }
        }

        Ok(())
    }
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
