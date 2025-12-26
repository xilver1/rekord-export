//! Error types for rekordbox-core

use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("Binary format error: {0}")]
    BinRw(String),
    
    #[error("Audio decoding error: {0}")]
    AudioDecode(String),
    
    #[error("Analysis error: {0}")]
    Analysis(String),
    
    #[error("Invalid track: {0}")]
    InvalidTrack(String),
    
    #[error("Cache error: {0}")]
    Cache(String),
    
    #[error("Path error: {0}")]
    Path(String),
}

pub type Result<T> = std::result::Result<T, Error>;

impl From<binrw::Error> for Error {
    fn from(e: binrw::Error) -> Self {
        Error::BinRw(e.to_string())
    }
}
