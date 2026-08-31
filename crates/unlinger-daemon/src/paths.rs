use std::error::Error;
use std::ffi::{CStr, CString, OsString};
use std::fmt::{Display, Formatter};
use std::fs::File;
use std::io;
use std::mem::MaybeUninit;
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};

pub const LAUNCH_AGENT_LABEL: &str = "app.unlinger.daemon";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LocalPaths {
    pub application_support: PathBuf,
    pub binary_directory: PathBuf,
    pub cache_directory: PathBuf,
    pub runtime_directory: PathBuf,
    pub logs_directory: PathBuf,
    pub database: PathBuf,
    pub socket: PathBuf,
    pub daemon_binary: PathBuf,
    pub cli_binary: PathBuf,
    pub launch_agent: PathBuf,
    pub error_log: PathBuf,
    pub service_lock: PathBuf,
    pub daemon_lock: PathBuf,
}

impl LocalPaths {
    pub fn discover() -> Result<Self, PathError> {
        let uid = unsafe { libc::geteuid() };
        if uid == 0 {
            return Err(PathError::RootUser);
        }
        let home = effective_user_home(uid)?;
        Self::from_home(home)
    }

    pub fn from_home(home: impl Into<OsString>) -> Result<Self, PathError> {
        let home = PathBuf::from(home.into());
        if !home.is_absolute() {
            return Err(PathError::HomeNotAbsolute(home));
        }
        if home == Path::new("/") {
            return Err(PathError::HomeIsRoot);
        }
        let library = home.join("Library");
        let application_support = library.join("Application Support").join("Unlinger");
        let binary_directory = application_support.join("bin");
        let cache_directory = library.join("Caches").join("Unlinger");
        let runtime_directory = application_support.join("run");
        let logs_directory = library.join("Logs").join("Unlinger");
        Ok(Self {
            database: application_support.join("history.sqlite3"),
            socket: runtime_directory.join("unlingerd.sock"),
            daemon_binary: binary_directory.join("unlingerd"),
            cli_binary: binary_directory.join("unlinger"),
            launch_agent: library
                .join("LaunchAgents")
                .join(format!("{LAUNCH_AGENT_LABEL}.plist")),
            error_log: logs_directory.join("unlingerd.log"),
            service_lock: application_support.join("service.lock"),
            daemon_lock: runtime_directory.join("daemon.lock"),
            application_support,
            binary_directory,
            cache_directory,
            runtime_directory,
            logs_directory,
        })
    }
}

fn effective_user_home(uid: libc::uid_t) -> Result<OsString, PathError> {
    let recommended = unsafe { libc::sysconf(libc::_SC_GETPW_R_SIZE_MAX) };
    let mut buffer_length = if recommended > 0 {
        usize::try_from(recommended).unwrap_or(16 * 1024)
    } else {
        16 * 1024
    };

    loop {
        let mut record = MaybeUninit::<libc::passwd>::uninit();
        let mut result = std::ptr::null_mut();
        let mut buffer = vec![0_u8; buffer_length];
        let status = unsafe {
            libc::getpwuid_r(
                uid,
                record.as_mut_ptr(),
                buffer.as_mut_ptr().cast(),
                buffer.len(),
                &mut result,
            )
        };
        if status == libc::ERANGE {
            buffer_length = buffer_length
                .checked_mul(2)
                .ok_or(PathError::UserLookupFailed(libc::ERANGE))?;
            continue;
        }
        if status != 0 {
            return Err(PathError::UserLookupFailed(status));
        }
        if result.is_null() {
            return Err(PathError::HomeUnavailable);
        }

        let record = unsafe { record.assume_init() };
        if record.pw_dir.is_null() {
            return Err(PathError::HomeUnavailable);
        }
        let bytes = unsafe { CStr::from_ptr(record.pw_dir) }.to_bytes();
        if bytes.is_empty() {
            return Err(PathError::HomeUnavailable);
        }
        return Ok(OsString::from_vec(bytes.to_vec()));
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PathError {
    RootUser,
    UserLookupFailed(i32),
    HomeUnavailable,
    HomeNotAbsolute(PathBuf),
    HomeIsRoot,
}

impl Display for PathError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::RootUser => write!(formatter, "refusing local per-user paths for root"),
            Self::UserLookupFailed(status) => write!(
                formatter,
                "could not resolve the effective user's account (getpwuid_r status {status})"
            ),
            Self::HomeUnavailable => {
                write!(formatter, "effective user home directory is unavailable")
            }
            Self::HomeNotAbsolute(path) => {
                write!(
                    formatter,
                    "effective user home is not absolute: {}",
                    path.display()
                )
            }
            Self::HomeIsRoot => {
                write!(formatter, "effective user home must not be filesystem root")
            }
        }
    }
}

impl Error for PathError {}

#[derive(Debug)]
pub struct DaemonInstanceLock {
    _file: File,
}

impl DaemonInstanceLock {
    pub fn acquire(path: impl AsRef<Path>) -> Result<Self, DaemonLockError> {
        let path = path.as_ref();
        let encoded = CString::new(path.as_os_str().as_bytes())
            .map_err(|_| DaemonLockError::InvalidPath(path.to_path_buf()))?;
        let descriptor = unsafe {
            libc::open(
                encoded.as_ptr(),
                libc::O_RDWR
                    | libc::O_CREAT
                    | libc::O_CLOEXEC
                    | libc::O_NONBLOCK
                    | no_follow_flag(),
                0o600 as libc::c_uint,
            )
        };
        if descriptor < 0 {
            return Err(DaemonLockError::io(
                "open daemon lock",
                path,
                io::Error::last_os_error(),
            ));
        }
        let file = unsafe { File::from_raw_fd(descriptor) };

        let mut metadata = MaybeUninit::<libc::stat>::uninit();
        if unsafe { libc::fstat(file.as_raw_fd(), metadata.as_mut_ptr()) } != 0 {
            return Err(DaemonLockError::io(
                "inspect daemon lock",
                path,
                io::Error::last_os_error(),
            ));
        }
        let metadata = unsafe { metadata.assume_init() };
        if metadata.st_mode & libc::S_IFMT != libc::S_IFREG {
            return Err(DaemonLockError::NotRegular(path.to_path_buf()));
        }

        let expected_uid = unsafe { libc::geteuid() };
        if metadata.st_uid != expected_uid {
            return Err(DaemonLockError::WrongOwner {
                path: path.to_path_buf(),
                expected_uid,
                actual_uid: metadata.st_uid,
            });
        }

        if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
            let source = io::Error::last_os_error();
            if matches!(source.raw_os_error(), Some(code) if code == libc::EWOULDBLOCK || code == libc::EAGAIN)
            {
                return Err(DaemonLockError::AlreadyHeld {
                    path: path.to_path_buf(),
                });
            }
            return Err(DaemonLockError::io("lock daemon instance", path, source));
        }

        if unsafe { libc::fchmod(file.as_raw_fd(), 0o600 as libc::mode_t) } != 0 {
            return Err(DaemonLockError::io(
                "protect daemon lock",
                path,
                io::Error::last_os_error(),
            ));
        }

        Ok(Self { _file: file })
    }
}

#[cfg(target_vendor = "apple")]
const fn no_follow_flag() -> libc::c_int {
    libc::O_NOFOLLOW_ANY
}

#[cfg(not(target_vendor = "apple"))]
const fn no_follow_flag() -> libc::c_int {
    libc::O_NOFOLLOW
}

#[derive(Debug)]
pub enum DaemonLockError {
    InvalidPath(PathBuf),
    Io {
        operation: &'static str,
        path: PathBuf,
        source: io::Error,
    },
    NotRegular(PathBuf),
    WrongOwner {
        path: PathBuf,
        expected_uid: libc::uid_t,
        actual_uid: libc::uid_t,
    },
    AlreadyHeld {
        path: PathBuf,
    },
}

impl DaemonLockError {
    fn io(operation: &'static str, path: &Path, source: io::Error) -> Self {
        Self::Io {
            operation,
            path: path.to_path_buf(),
            source,
        }
    }
}

impl Display for DaemonLockError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidPath(path) => {
                write!(
                    formatter,
                    "daemon lock path contains NUL: {}",
                    path.display()
                )
            }
            Self::Io {
                operation,
                path,
                source,
            } => write!(formatter, "{operation} {}: {source}", path.display()),
            Self::NotRegular(path) => write!(
                formatter,
                "daemon lock is not a regular file: {}",
                path.display()
            ),
            Self::WrongOwner {
                path,
                expected_uid,
                actual_uid,
            } => write!(
                formatter,
                "daemon lock {} is owned by uid {actual_uid}, expected uid {expected_uid}",
                path.display()
            ),
            Self::AlreadyHeld { path } => write!(
                formatter,
                "another daemon instance holds {}",
                path.display()
            ),
        }
    }
}

impl Error for DaemonLockError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::{MetadataExt, symlink};
    use std::process::Command;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

    struct TempDirectory(PathBuf);

    impl TempDirectory {
        fn new(label: &str) -> Self {
            let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "unlinger-paths-{label}-{}-{sequence}",
                std::process::id()
            ));
            fs::create_dir(&path).expect("create owned temporary directory");
            Self(fs::canonicalize(path).expect("canonicalize owned temporary directory"))
        }
    }

    impl Drop for TempDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn macos_defaults_keep_runtime_state_in_owner_private_application_support() {
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
            PathBuf::from("/Users/example/Library/Application Support/Unlinger/run/unlingerd.sock")
        );
        assert_eq!(
            paths.runtime_directory,
            PathBuf::from("/Users/example/Library/Application Support/Unlinger/run")
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
        assert_eq!(
            paths.daemon_lock,
            PathBuf::from("/Users/example/Library/Application Support/Unlinger/run/daemon.lock")
        );
    }

    #[test]
    fn local_path_test_seam_rejects_relative_and_root_homes() {
        assert_eq!(
            LocalPaths::from_home("relative"),
            Err(PathError::HomeNotAbsolute(PathBuf::from("relative")))
        );
        assert_eq!(LocalPaths::from_home("/"), Err(PathError::HomeIsRoot));
    }

    #[test]
    fn discover_ignores_poisoned_home_environment() {
        const CHILD_MARKER: &str = "UNLINGER_PATHS_POISONED_HOME_CHILD";
        if std::env::var_os(CHILD_MARKER).is_some() {
            if unsafe { libc::geteuid() } == 0 {
                assert_eq!(LocalPaths::discover(), Err(PathError::RootUser));
                return;
            }
            let paths = LocalPaths::discover().expect("resolve effective user's home");
            assert!(
                !paths
                    .application_support
                    .starts_with("/definitely-not-the-effective-users-home")
            );
            return;
        }

        let status = Command::new(std::env::current_exe().expect("current test executable"))
            .arg("discover_ignores_poisoned_home_environment")
            .arg("--nocapture")
            .env(CHILD_MARKER, "1")
            .env("HOME", "/definitely-not-the-effective-users-home")
            .status()
            .expect("run isolated poisoned-HOME assertion");
        assert!(status.success());
    }

    #[test]
    fn daemon_instance_lock_is_exclusive_for_its_lifetime() {
        let temporary = TempDirectory::new("exclusive");
        let path = temporary.0.join("daemon.lock");

        let first = DaemonInstanceLock::acquire(&path).expect("acquire first lock");
        assert_eq!(
            fs::metadata(&path).expect("lock metadata").mode() & 0o777,
            0o600
        );
        assert!(matches!(
            DaemonInstanceLock::acquire(&path),
            Err(DaemonLockError::AlreadyHeld { .. })
        ));
        drop(first);
        DaemonInstanceLock::acquire(&path).expect("reacquire after first lock drops");
    }

    #[test]
    fn daemon_instance_lock_rejects_symlinks_and_non_files() {
        let temporary = TempDirectory::new("types");
        let target = temporary.0.join("target.lock");
        fs::write(&target, b"").expect("create target file");
        let symlink_path = temporary.0.join("symlink.lock");
        symlink(&target, &symlink_path).expect("create lock symlink");
        assert!(DaemonInstanceLock::acquire(&symlink_path).is_err());

        let directory_path = temporary.0.join("directory.lock");
        fs::create_dir(&directory_path).expect("create wrong-type lock path");
        assert!(DaemonInstanceLock::acquire(&directory_path).is_err());
    }

    #[cfg(target_vendor = "apple")]
    #[test]
    fn daemon_instance_lock_rejects_a_symlinked_parent_component() {
        let temporary = TempDirectory::new("parent-symlink");
        let actual_directory = temporary.0.join("actual");
        fs::create_dir(&actual_directory).expect("create actual lock directory");
        let linked_directory = temporary.0.join("linked");
        symlink(&actual_directory, &linked_directory).expect("create parent symlink");

        assert!(DaemonInstanceLock::acquire(linked_directory.join("daemon.lock")).is_err());
        assert!(!actual_directory.join("daemon.lock").exists());
    }
}
