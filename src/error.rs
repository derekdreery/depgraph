
/// Any error that can occur during build
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

pub type DepResult<T> = Result<T, Error>;
