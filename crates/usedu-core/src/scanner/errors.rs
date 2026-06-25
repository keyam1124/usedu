use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum ScannerError {
    #[error("TUI target is not a directory: {0}")]
    NotDirectory(PathBuf),
    #[error("scan cancelled")]
    Cancelled,
    #[error("scan resource limit reached: {0}")]
    ResourceLimitReached(&'static str),
}
