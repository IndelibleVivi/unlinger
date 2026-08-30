use std::error::Error;
use std::ffi::OsString;
use std::fmt::{Display, Formatter};
use std::path::PathBuf;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LocalPaths {
    pub database: PathBuf,
    pub socket: PathBuf,
}

impl LocalPaths {
    pub fn discover() -> Result<Self, PathError> {
        let home = std::env::var_os("HOME").ok_or(PathError::HomeUnavailable)?;
        Self::from_home(home)
    }

    pub fn from_home(home: impl Into<OsString>) -> Result<Self, PathError> {
        let home = PathBuf::from(home.into());
        if !home.is_absolute() {
            return Err(PathError::HomeNotAbsolute(home));
        }
        Ok(Self {
            database: home
                .join("Library")
                .join("Application Support")
                .join("Unlinger")
                .join("history.sqlite3"),
            socket: home
                .join("Library")
                .join("Caches")
                .join("Unlinger")
                .join("unlingerd.sock"),
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PathError {
    HomeUnavailable,
    HomeNotAbsolute(PathBuf),
}

impl Display for PathError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::HomeUnavailable => write!(formatter, "HOME is unavailable for local paths"),
            Self::HomeNotAbsolute(path) => {
                write!(formatter, "HOME is not absolute: {}", path.display())
            }
        }
    }
}

impl Error for PathError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn macos_defaults_separate_database_and_short_lived_socket() {
        let paths = LocalPaths::from_home("/Users/example").expect("absolute home");
        assert_eq!(
            paths.database,
            PathBuf::from("/Users/example/Library/Application Support/Unlinger/history.sqlite3")
        );
        assert_eq!(
            paths.socket,
            PathBuf::from("/Users/example/Library/Caches/Unlinger/unlingerd.sock")
        );
    }
}
