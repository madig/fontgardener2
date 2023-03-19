use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
};

use glyphsinfo_rs::GlyphData;
use norad::Codepoints;

use crate::{
    errors::{SourceLoadError, SourceSaveError},
    structs::{Fontgarden, Glyph, Layer, OpenTypeCategory},
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

    pub fn export_ufo_sources(
        &self,
        source_names: &HashSet<&str>,
    ) -> Result<HashMap<String, norad::Font>, SourceSaveError> {
        let mut ufos: HashMap<String, norad::Font> = HashMap::new();

        let mut postscript_names = plist::Dictionary::new();
        let mut opentype_categories = plist::Dictionary::new();

        for (glyph_name, glyph) in self.glyphs.iter() {
            let ufo_glyph_name = norad::Name::new(glyph_name)
                .map_err(|e| SourceSaveError::GlyphNamingError(glyph_name.clone(), e))?;
            for (layer_name, layer) in glyph.layers.iter().filter(|(layer_name, _)| {
                source_names.is_empty() || source_names.contains(layer_name.as_str())
            }) {
                match layer_name.split_once('.') {
                    Some((base, suffix)) => {
                        let ufo: &mut norad::Font = ufos.entry(base.to_string()).or_default();
                        let ufo_glyph = layer.export_to_ufo_glyph(ufo_glyph_name.clone(), None)?;
                        ufo.layers
                            .get_or_create_layer(suffix)
                            .map_err(|e| SourceSaveError::GlyphNamingError(suffix.into(), e))?
                            .insert_glyph(ufo_glyph);
                    }
                    None => {
                        let ufo: &mut norad::Font = ufos.entry(layer_name.to_string()).or_default();
                        let ufo_glyph = layer
                            .export_to_ufo_glyph(ufo_glyph_name.clone(), Some(&glyph.codepoints))?;
                        ufo.layers.default_layer_mut().insert_glyph(ufo_glyph);

                        if let Some(postscript_name) = &glyph.postscript_name {
                            postscript_names
                                .insert(glyph_name.into(), postscript_name.clone().into());
                        }
                        if glyph.opentype_category != OpenTypeCategory::Unassigned {
                            let otc: String =
                                serde_json::to_string(&glyph.opentype_category).unwrap();
                            opentype_categories.insert(glyph_name.into(), otc.into());
                        }
                    }
                }
            }
        }

        for (source_name, source) in ufos.iter_mut() {
            source.font_info.style_name = Some(source_name.clone());
        }

        if !postscript_names.is_empty() {
            for source in ufos.values_mut() {
                source.lib.insert(
                    "public.postscriptNames".into(),
                    postscript_names.clone().into(),
                );
            }
        }

        if !opentype_categories.is_empty() {
            for source in ufos.values_mut() {
                source.lib.insert(
                    "public.openTypeCategories".into(),
                    opentype_categories.clone().into(),
                );
            }
        }

        Ok(ufos)
    }
}

impl Layer {
    pub fn export_to_ufo_glyph(
        &self,
        name: norad::Name,
        codepoints: Option<&Codepoints>,
    ) -> Result<norad::Glyph, SourceSaveError> {
        let mut ufo_glyph = norad::Glyph::new(&name);

        if let Some(codepoints) = codepoints {
            ufo_glyph.codepoints = codepoints.clone();
        }

        ufo_glyph.width = self.x_advance.unwrap_or_default();
        if let (Some(y_advance), Some(vertical_origin)) = (self.y_advance, self.vertical_origin) {
            ufo_glyph.height = y_advance;
            ufo_glyph
                .lib
                .insert("public.verticalOrigin".into(), vertical_origin.into());
        }

        ufo_glyph.anchors = self
            .anchors
            .iter()
            .map(|anchor| anchor.try_into())
            .collect::<Result<_, _>>()
            .map_err(|e| SourceSaveError::AnchorNamingError(name.to_string(), e))?;
        ufo_glyph.contours = self.contours.iter().map(|contour| contour.into()).collect();
        ufo_glyph.components = self
            .components
            .iter()
            .map(|component| component.try_into())
            .collect::<Result<_, _>>()
            .map_err(|e| SourceSaveError::ComponentNamingError(name.to_string(), e))?;

        Ok(ufo_glyph)
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

fn convert_fontgarden_layer_to_ufo_glyph(
    glyph: Option<&Glyph>,
    glyph_name: norad::Name,
    layer: &Layer,
) -> Result<norad::Glyph, SourceSaveError> {
    let mut ufo_glyph = norad::Glyph::new(&glyph_name);

    if let Some(glyph) = glyph {
        ufo_glyph.codepoints = glyph.codepoints.clone();
    }

    ufo_glyph.width = layer.x_advance.unwrap_or_default();
    if let (Some(y_advance), Some(vertical_origin)) = (layer.y_advance, layer.vertical_origin) {
        ufo_glyph.height = y_advance;
        ufo_glyph
            .lib
            .insert("public.verticalOrigin".into(), vertical_origin.into());
    }

    ufo_glyph.anchors = layer
        .anchors
        .iter()
        .map(|x| x.try_into())
        .collect::<Result<_, _>>()
        .map_err(|e| SourceSaveError::AnchorNamingError(glyph_name.to_string(), e))?;
    ufo_glyph.contours = layer.contours.iter().map(|l| l.into()).collect();
    ufo_glyph.components = layer
        .components
        .iter()
        .map(|x| x.try_into())
        .collect::<Result<_, _>>()
        .map_err(|e| SourceSaveError::ComponentNamingError(glyph_name.to_string(), e))?;

    Ok(ufo_glyph)
}
