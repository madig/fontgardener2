use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
};

use clap::{Parser, Subcommand};
use errors::{SourceLoadError, SourceSaveError};
use glyphsinfo_rs::{self, GlyphData};
use rayon::prelude::*;
use structs::Fontgarden;

mod errors;
mod filenames;
mod structs;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    Import {
        /// Fontgarden package path to export from.
        fontgarden_path: PathBuf,

        /// Sources to import.
        #[arg(num_args = 1..)]
        sources: Vec<PathBuf>,
    },
    Export {
        /// Fontgarden package path to export from.
        fontgarden_path: PathBuf,

        /// Sources to export glyphs for [default: all]
        #[arg(long = "source-name", value_name = "SOURCE_NAME")]
        source_names: Vec<String>,

        /// Directory to export into [default: current dir].
        #[arg(long)]
        output_dir: Option<PathBuf>,
    },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Import {
            fontgarden_path,
            sources,
        } => {
            let fontgarden = import_ufos_into_fontgarden(&sources)?;
            fontgarden.save(&fontgarden_path)?;
        }
        Commands::Export {
            fontgarden_path,
            source_names,
            output_dir,
        } => {
            let fontgarden = Fontgarden::load(&fontgarden_path)?;
            let source_names: HashSet<&str> = source_names.iter().map(|s| s.as_str()).collect();
            let sources: HashMap<PathBuf, norad::Font> =
                export_ufos_from_fontgarden(&fontgarden, &source_names)?;
            let output_dir = output_dir.unwrap_or_else(|| PathBuf::from("."));
            std::fs::create_dir_all(&output_dir)?;
            sources
                .into_par_iter()
                .try_for_each(|(source_path, source)| source.save(&output_dir.join(source_path)))?;
        }
    }

    Ok(())
}

fn export_ufos_from_fontgarden(
    fontgarden: &Fontgarden,
    source_names: &HashSet<&str>,
) -> Result<HashMap<String, norad::Font>, SourceSaveError> {
    let mut ufos = HashMap::new();

    for (glyph_name, glyph) in fontgarden.glyphs.iter() {
        let ufo_glyph_name = norad::Name::new(glyph_name)
            .map_err(|e| SourceSaveError::UfoNamingError(glyph_name.clone(), e))?;
        for (layer_name, layer) in glyph.layers.iter().filter(|(layer_name, _)| {
            source_names.is_empty() || source_names.contains(layer_name.as_str())
        }) {
            match layer_name.split_once(".") {
                Some((base, suffix)) => {
                    let ufo: &mut norad::Font = ufos.entry(&*layer_name).or_default();
                    let ufo_glyph = convert_to_ufo_glyph(ufo_glyph_name.clone(), glyph);
                    let ufo_layer = ufo
                        .layers
                        .get_or_create_layer(&suffix)
                        .map_err(|e| SourceSaveError::UfoNamingError(suffix.into(), e))?;
                    ufo.layers.default_layer_mut().insert_glyph(ufo_glyph);
                }
                None => {
                    let ufo: &mut norad::Font = ufos.entry(&*layer_name).or_default();
                    let ufo_glyph = convert_to_ufo_glyph(ufo_glyph_name.clone(), glyph);
                    ufo.layers.default_layer_mut().insert_glyph(ufo_glyph);
                }
            }
        }
    }

    Ok(ufos)
}

fn convert_to_ufo_glyph(glyph_name: norad::Name, glyph: &structs::Glyph) -> norad::Glyph {
    todo!()
}

fn import_ufos_into_fontgarden(sources: &[PathBuf]) -> Result<Fontgarden, anyhow::Error> {
    let sources = load_sources(sources)?;
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

    Ok(fontgarden)
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

#[cfg(test)]
mod tests {
    use norad::Codepoints;

    use structs::{Glyph, OpenTypeCategory};

    use super::*;

    #[test]
    fn roundtrip_empty() {
        let fontgarden = Fontgarden::new();

        let fontgarden_path = tempfile::tempdir().unwrap();
        fontgarden.save(fontgarden_path.path()).unwrap();
        let roundtripped_fontgarden = Fontgarden::load(fontgarden_path.path()).unwrap();

        assert_eq!(fontgarden, roundtripped_fontgarden);
    }

    #[test]
    fn roundtrip_no_layers() {
        let mut fontgarden = Fontgarden::new();
        fontgarden.glyphs.insert(
            "a".into(),
            Glyph {
                codepoints: Codepoints::new(['a']),
                layers: HashMap::new(),
                opentype_category: OpenTypeCategory::Unassigned,
                postscript_name: Some("a".into()),
                set: None,
            },
        );
        fontgarden.glyphs.insert(
            "b".into(),
            Glyph {
                codepoints: Codepoints::new([]),
                layers: HashMap::new(),
                opentype_category: OpenTypeCategory::Base,
                postscript_name: None,
                set: Some("Test".into()),
            },
        );

        let fontgarden_path = tempfile::tempdir().unwrap();
        fontgarden.save(fontgarden_path.path()).unwrap();
        let roundtripped_fontgarden = Fontgarden::load(fontgarden_path.path()).unwrap();

        assert_eq!(fontgarden, roundtripped_fontgarden);
    }

    #[test]
    fn roundtrip() {
        let fontgarden = import_ufos_into_fontgarden(&[
            "testdata/mutatorSans/MutatorSansBoldCondensed.ufo/".into(),
            "testdata/mutatorSans/MutatorSansBoldWide.ufo/".into(),
            "testdata/mutatorSans/MutatorSansLightCondensed.ufo/".into(),
            "testdata/mutatorSans/MutatorSansLightWide.ufo/".into(),
        ])
        .unwrap();

        let fontgarden_path = tempfile::tempdir().unwrap();
        fontgarden.save(fontgarden_path.path()).unwrap();
        let roundtripped_fontgarden = Fontgarden::load(fontgarden_path.path()).unwrap();

        assert_eq!(fontgarden, roundtripped_fontgarden);
    }
}
