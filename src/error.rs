use thiserror::Error;

#[derive(Debug, Error)]
pub enum RasitopError {
    #[error("rasitop requires macOS (detected OS: {0})")]
    UnsupportedOs(String),
    #[error("no CPU frequencies found")]
    MissingCpuFrequencies,
}
