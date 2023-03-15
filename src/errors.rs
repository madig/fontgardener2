use std::path::PathBuf;

use thiserror::Error;

#[derive(Error, Debug)]
pub enum SourceLoadError {
    #[error("failed to load UFO source {0}")]
    Ufo(PathBuf, #[source] norad::error::FontLoadError),
    #[error("more than one source uses the same style name {0}, last seen in {1}")]
    DuplicateLayerName(String, PathBuf),
}

#[derive(Error, Debug)]
pub enum LoadError {
    #[error("failed to load {0} from disk")]
    Io(PathBuf, #[source] std::io::Error),
    #[error("a fontgarden must be a directory")]
    NotAFontgarden,
    #[error("cannot load set '{0}' as a glyph it contains is in a different set already: {1}")]
    DuplicateGlyphs(String, String),
    #[error("cannot load set '{0}' as the glyph {1} has (an) invalid codepoint(s): {2}")]
    InvalidCodepoints(
        String,
        String,
        String,
        #[source] Box<dyn std::error::Error + Send + Sync>,
    ),
    #[error("failed to save set data '{0}'")]
    LoadSetData(PathBuf, #[source] csv::Error),
}

#[derive(Error, Debug)]
pub enum SaveError {
    #[error("failed to remove target directory before overwriting")]
    Cleanup(#[source] std::io::Error),
    #[error("failed to create target fontgarden directory")]
    CreateDir(#[source] std::io::Error),
    #[error("failed to create directory for glyph {0}")]
    CreateGlyphDir(String, #[source] std::io::Error),
    #[error("failed to save glyph {0}, layer '{1}'")]
    SaveLayer(String, String, #[source] std::io::Error),
    #[error("failed to save JSON data for glyph {0}, layer '{1}'")]
    SaveLayerJson(String, String, #[source] serde_json::Error),
    #[error("failed to save set data '{0}'")]
    SaveSetData(String, #[source] csv::Error),
}
