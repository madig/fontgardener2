use thiserror::Error;

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
