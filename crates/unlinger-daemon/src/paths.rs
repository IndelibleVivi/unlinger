use std::error::Error;
use std::ffi::OsString;
use std::fmt::{Display, Formatter};
use std::path::PathBuf;

pub const LAUNCH_AGENT_LABEL: &str = "app.unlinger.daemon";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LocalPaths {
    pub application_support: PathBuf,
    pub binary_directory: PathBuf,
    pub cache_directory: PathBuf,
    pub logs_directory: PathBuf,
    pub database: PathBuf,
    pub socket: PathBuf,
    pub daemon_binary: PathBuf,
    pub cli_binary: PathBuf,
    pub launch_agent: PathBuf,
    pub error_log: PathBuf,
    pub service_lock: PathBuf,
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
        let library = home.join("Library");
        let application_support = library.join("Application Support").join("Unlinger");
        let binary_directory = application_support.join("bin");
        let cache_directory = library.join("Caches").join("Unlinger");
        let logs_directory = library.join("Logs").join("Unlinger");
        Ok(Self {
            database: application_support.join("history.sqlite3"),
            socket: cache_directory.join("unlingerd.sock"),
            daemon_binary: binary_directory.join("unlingerd"),
            cli_binary: binary_directory.join("unlinger"),
            launch_agent: library
                .join("LaunchAgents")
                .join(format!("{LAUNCH_AGENT_LABEL}.plist")),
            error_log: logs_directory.join("unlingerd.log"),
            service_lock: application_support.join("service.lock"),
            application_support,
            binary_directory,
            cache_directory,
            logs_directory,
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
            paths.application_support,
            PathBuf::from("/Users/example/Library/Application Support/Unlinger")
        );
        assert_eq!(
            paths.database,
            PathBuf::from("/Users/example/Library/Application Support/Unlinger/history.sqlite3")
        );
        assert_eq!(
            paths.socket,
            PathBuf::from("/Users/example/Library/Caches/Unlinger/unlingerd.sock")
        );
        assert_eq!(
            paths.daemon_binary,
            PathBuf::from("/Users/example/Library/Application Support/Unlinger/bin/unlingerd")
        );
        assert_eq!(
            paths.cli_binary,
            PathBuf::from("/Users/example/Library/Application Support/Unlinger/bin/unlinger")
        );
        assert_eq!(
            paths.launch_agent,
            PathBuf::from("/Users/example/Library/LaunchAgents/app.unlinger.daemon.plist")
        );
        assert_eq!(
            paths.error_log,
            PathBuf::from("/Users/example/Library/Logs/Unlinger/unlingerd.log")
        );
        assert_eq!(
            paths.service_lock,
            PathBuf::from("/Users/example/Library/Application Support/Unlinger/service.lock")
        );
    }
}
