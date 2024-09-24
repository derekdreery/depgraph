use std::{io, path::PathBuf};
use thiserror::Error as ThisError;

/// Any error that can occur during build
///
/// One area for improvement is allowing more data to be present in an error, and possibly making
/// `BuildFailed` use a trait object (impl std::Error) to be more general. Suggestions welcome!
#[derive(Debug, ThisError)]
pub enum Error {
    /// Cyclic dependencies detected
    #[error("cyclic dependencies detected")]
    Cycle,
    /// Same file added more than once
    #[error("same file added more than once")]
    DuplicateFile,
    /// A file that should either be present or be crated during build is missing.
    #[error("a file that should either be present or be crated during build is missing")]
    MissingFile(PathBuf),
    /// The supplied build script returned an error
    #[error("the supplied build script returned an error")]
    BuildFailed(String),
    /// Generic I/O error
    #[error("I/O error")]
    Io(#[from] io::Error),
}

/// The ubiquitous crate result type
pub type DepResult<T> = Result<T, Error>;
