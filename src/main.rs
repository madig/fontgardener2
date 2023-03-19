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
mod ufo;

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
            sources,
        } => {
            if sources.is_empty() {
                error_and_exit(
                    clap::error::ErrorKind::WrongNumberOfValues,
                    "must give at least one source to import",
                )
            }
            let mut fontgarden = if fontgarden_path.exists() {
                Fontgarden::load(&fontgarden_path)?
            } else {
                Fontgarden::new()
            };
            fontgarden.import_ufo_sources(&sources)?;
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
        let mut fontgarden = Fontgarden::new();
        fontgarden
            .import_ufo_sources(&[
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

    #[test]
    fn roundtrip_export_import() {
        let mut fontgarden = Fontgarden::new();
        fontgarden
            .import_ufo_sources(&[
                "testdata/mutatorSans/MutatorSansBoldCondensed.ufo/".into(),
                "testdata/mutatorSans/MutatorSansBoldWide.ufo/".into(),
                "testdata/mutatorSans/MutatorSansLightCondensed.ufo/".into(),
                "testdata/mutatorSans/MutatorSansLightWide.ufo/".into(),
            ])
            .unwrap();

        let export_dir = tempfile::tempdir().unwrap();

        command_export(&fontgarden, &HashSet::new(), export_dir.path()).unwrap();

        let mut roundtripped_fontgarden = Fontgarden::new();
        roundtripped_fontgarden
            .import_ufo_sources(&[
                export_dir.path().join("BoldCondensed.ufo"),
                export_dir.path().join("BoldWide.ufo"),
                export_dir.path().join("LightCondensed.ufo"),
                export_dir.path().join("LightWide.ufo"),
            ])
            .unwrap();

        assert_eq!(fontgarden, roundtripped_fontgarden);
    }
}
