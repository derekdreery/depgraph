
/// Any error that can occur during build
///
/// One area for improvement is allowing more data to be present in an error, and possibly making
/// `BuildFailed` use a trait object (impl std::Error) to be more general. Suggestions welcome!
#[derive(Debug)]
pub enum Error {
    /// Cyclic dependencies detected
    Cycle,
    /// Same file added more than once
    DuplicateFile,
    /// A file that should either be present or be crated during build is missing.
    MissingFile(String),
    /// The supplied build script returned an error
    BuildFailed(String),
}

/// The ubiquitous crate result type
pub type DepResult<T> = Result<T, Error>;
