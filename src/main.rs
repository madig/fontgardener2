use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
};

use clap::{CommandFactory, Parser, Subcommand};
use rayon::prelude::*;

use structs::Fontgarden;

mod errors;
mod filenames;
mod structs;
pub mod ufo;

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

        /// Sets to import into.
        #[arg(short = 's', long = "sets", value_name = "SET")]
        sets: Vec<String>,

        /// Sources to import.
        #[arg(required = true)]
        sources: Vec<PathBuf>,
    },
    Export {
        /// Fontgarden package path to export from.
        fontgarden_path: PathBuf,

        /// Directory to export into [default: current dir].
        output_dir: Option<PathBuf>,

        /// Sources to export glyphs for [default: all]
        #[arg(long = "source-name", value_name = "SOURCE_NAME")]
        source_names: Vec<String>,
    },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Import {
            fontgarden_path,
            sets,
            sources,
        } => {
            // To import from a UFO:
            // 1. Read UFOs to build an import_set of the union of all their glyphs in
            //    all layers.
            // 2. Make an all_glyphs_set out of all of the fontgarden glyphs (just read
            //    glyph metadata).
            // 3. Make a reference_set out of the fontgarden glyphs of the specified
            //    sets to import into and follow the glyphs everywhere to add components
            //    to the reference_set (or set to all_glyphs_set if no sets were
            //    specified). NOTE: Potentially multiple import sets means we can't
            //    assign a definite one to added glyphs.
            // 4. Now you have:
            //     1. added_glyphs_set = import_set - all_glyphs_set
            //     2. modified_glyphs_set = reference_set & import_set
            //     3. removed_glyphs_set = reference_set - import_set
            // 5. Ask the user to proceed if no sets to import were specified and
            //    removed_glyphs_set would therefore be the rest of the font.
            // 6. If the user gave a single set to import, assign new glyphs to that
            //    set. If there are multiple, guess (maybe look at the glyphs in the
            //    sets and guess based on script tags like `-arab` or `.loclTAML`).
            // 7. Read partial fontgarden with modified_glyphs_set (we want to overwrite
            //    layer data but keep layers not modified untouched), add new glyphs.
            // 8. Write out partial fontgarden by overwriting existing directory
            //    structure (will store added and modified glyphs), then do extra pass
            //    to delete removed glyphs (but only for the (parent) layers the import
            //    sources contain).
            //
            // TODO: think harder about glyph removal. Can the formula above remove
            // glyphs that we don't actually want to remove? What if we import the
            // upright set and delete some glyph there but we want to keep the italic
            // layers? Should glyph deletion mean deleting the layers from the import
            // sources? And the glyph dir is only deleted if it's empty?
            if sources.is_empty() {
                error_and_exit(
                    clap::error::ErrorKind::WrongNumberOfValues,
                    "must give at least one source to import",
                )
            }

            dbg!(&fontgarden_path);
            dbg!(&sets);
            dbg!(&sources);

            // 1.
            let sources = ufo::load_sources(&sources)?;
            let import_set = ufo::gather_glyph_set(&sources);

            // 2.
            let import_sets: HashSet<String> =
                HashSet::from_iter(sets.iter().map(|s| s.to_string()));
            let mut fontgarden = if fontgarden_path.exists() {
                Fontgarden::load(&fontgarden_path)?
            } else {
                Fontgarden::new()
            };
            let all_glyphs_set: HashSet<String> = fontgarden
                .glyphs
                .keys()
                .map(|name| name.to_string())
                .collect();
            let reference_set: HashSet<String> = if import_sets.is_empty() {
                all_glyphs_set.clone()
            } else {
                // 3.
                let mut reference_set: HashSet<String> = fontgarden
                    .glyphs
                    .iter()
                    .filter_map(|(name, glyph)| {
                        if import_sets.contains(glyph.set.as_deref().unwrap_or("Common")) {
                            Some(name.to_string())
                        } else {
                            None
                        }
                    })
                    .collect();

                reference_set.extend(fontgarden.follow_composites(&reference_set).into_iter());

                reference_set
            };

            // 4.
            let added_glyphs_set: HashSet<String> =
                import_set.difference(&all_glyphs_set).cloned().collect();
            let modified_glyphs_set: HashSet<String> =
                reference_set.intersection(&import_set).cloned().collect();
            let removed_glyphs_set: HashSet<String> =
                reference_set.difference(&import_set).cloned().collect();

            // 5.
            // TODO

            // 6.
            let definitive_set = if sets.len() == 1 {
                Some(sets.first().unwrap().to_string())
            } else {
                None
            };

            fontgarden.import_ufo_sources(&sources, definitive_set)?;

            // 7.
            fontgarden.remove_glyphs(&removed_glyphs_set, &sources.keys().cloned().collect());

            println!("Added glyphs: {added_glyphs_set:?}");
            println!("Modified glyphs: {modified_glyphs_set:?}");
            println!("Removed glyphs: {removed_glyphs_set:?}");

            // 8.
            fontgarden.save(&fontgarden_path)?;
        }
        Commands::Export {
            fontgarden_path,
            source_names,
            output_dir,
        } => {
            let fontgarden = Fontgarden::load(&fontgarden_path)?;
            let source_names: HashSet<&str> = source_names.iter().map(|s| s.as_str()).collect();
            let output_dir = output_dir.unwrap_or_else(|| PathBuf::from("."));
            command_export(&fontgarden, &source_names, &output_dir)?;
        }
    }

    Ok(())
}

fn command_export(
    fontgarden: &Fontgarden,
    source_names: &HashSet<&str>,
    output_dir: &Path,
) -> Result<(), anyhow::Error> {
    let sources: HashMap<String, norad::Font> = fontgarden.export_ufo_sources(source_names)?;

    std::fs::create_dir_all(output_dir)?;
    sources
        .into_par_iter()
        .try_for_each(|(source_name, source)| {
            source.save(output_dir.join(source_name).with_extension("ufo"))
        })?;

    Ok(())
}

fn error_and_exit(kind: clap::error::ErrorKind, message: impl std::fmt::Display) -> ! {
    let mut cmd = Cli::command();
    cmd.error(kind, message).exit();
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
    fn roundtrip_save_load() {
        let sources = ufo::load_sources(&[
            "testdata/mutatorSans/MutatorSansBoldCondensed.ufo/".into(),
            "testdata/mutatorSans/MutatorSansBoldWide.ufo/".into(),
            "testdata/mutatorSans/MutatorSansLightCondensed.ufo/".into(),
            "testdata/mutatorSans/MutatorSansLightWide.ufo/".into(),
        ])
        .unwrap();
        let mut fontgarden = Fontgarden::new();
        fontgarden.import_ufo_sources(&sources, None).unwrap();

        let fontgarden_path = tempfile::tempdir().unwrap();
        fontgarden.save(fontgarden_path.path()).unwrap();
        let roundtripped_fontgarden = Fontgarden::load(fontgarden_path.path()).unwrap();

        assert_eq!(fontgarden, roundtripped_fontgarden);
    }

    #[test]
    fn roundtrip_export_import() {
        let sources = ufo::load_sources(&[
            "testdata/mutatorSans/MutatorSansBoldCondensed.ufo/".into(),
            "testdata/mutatorSans/MutatorSansBoldWide.ufo/".into(),
            "testdata/mutatorSans/MutatorSansLightCondensed.ufo/".into(),
            "testdata/mutatorSans/MutatorSansLightWide.ufo/".into(),
        ])
        .unwrap();
        let mut fontgarden = Fontgarden::new();
        fontgarden.import_ufo_sources(&sources, None).unwrap();

        let export_dir = tempfile::tempdir().unwrap();

        command_export(&fontgarden, &HashSet::new(), export_dir.path()).unwrap();

        let roundtripped_sources = ufo::load_sources(&[
            export_dir.path().join("BoldCondensed.ufo"),
            export_dir.path().join("BoldWide.ufo"),
            export_dir.path().join("LightCondensed.ufo"),
            export_dir.path().join("LightWide.ufo"),
        ])
        .unwrap();
        let mut roundtripped_fontgarden = Fontgarden::new();
        roundtripped_fontgarden
            .import_ufo_sources(&roundtripped_sources, None)
            .unwrap();

        assert_eq!(fontgarden, roundtripped_fontgarden);
    }
}
