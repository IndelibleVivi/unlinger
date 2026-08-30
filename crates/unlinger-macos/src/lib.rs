#[cfg(target_os = "macos")]
mod platform {
    use libproc::libproc::proc_pid::{pidinfo, pidpath};
    use libproc::libproc::task_info::TaskAllInfo;
    use libproc::processes::{ProcFilter, pids_by_type};
    use std::error::Error;
    use std::ffi::c_void;
    use std::fmt::{Display, Formatter};
    use std::fs;
    use std::mem::size_of;
    use std::os::unix::fs::MetadataExt;
    use std::time::{SystemTime, UNIX_EPOCH};
    use unlinger_core::{
        CleanupRuntime, CleanupSignal, ExecutableIdentity, ProcessIdentity, ProcessRecord,
        ProcessStatus, RuntimeFailure, SignalDisposition, Snapshot, SnapshotCoverage,
    };

    const CTL_KERN: libc::c_int = 1;
    const KERN_ARGMAX: libc::c_int = 8;
    const KERN_PROCARGS2: libc::c_int = 49;
    const STATUS_RUN: u32 = 2;
    const STATUS_SLEEP: u32 = 3;
    const STATUS_STOP: u32 = 4;
    const STATUS_ZOMBIE: u32 = 5;

    #[derive(Clone, Debug, Eq, PartialEq)]
    pub enum SnapshotError {
        Clock(String),
        ProcessList(String),
        Sysctl(String),
        ProcessLookup(String),
    }

    impl Display for SnapshotError {
        fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
            match self {
                Self::Clock(message) => write!(formatter, "clock error: {message}"),
                Self::ProcessList(message) => write!(formatter, "process-list error: {message}"),
                Self::Sysctl(message) => write!(formatter, "sysctl error: {message}"),
                Self::ProcessLookup(message) => {
                    write!(formatter, "process lookup error: {message}")
                }
            }
        }
    }

    impl Error for SnapshotError {}

    #[derive(Clone, Debug, Default)]
    pub struct MacosSnapshotter;

    impl MacosSnapshotter {
        #[must_use]
        pub fn new() -> Self {
            Self
        }

        pub fn capture(&self) -> Result<Snapshot, SnapshotError> {
            let current_uid = unsafe { libc::geteuid() };
            let pids = pids_by_type(ProcFilter::ByUID { uid: current_uid })
                .map_err(|error| SnapshotError::ProcessList(error.to_string()))?;
            let argmax = kernel_argmax()?;
            let mut coverage = SnapshotCoverage {
                listed_processes: pids.len(),
                ..SnapshotCoverage::default()
            };
            let mut processes = Vec::with_capacity(pids.len());

            for pid in pids {
                match read_process(pid, argmax) {
                    Ok(process) => {
                        coverage.inspected_processes += 1;
                        if process.arguments.is_none() {
                            coverage.arguments_unavailable += 1;
                        }
                        if !process.executable.is_complete() {
                            coverage.executable_identity_unavailable += 1;
                        }
                        processes.push(process);
                    }
                    Err(()) => coverage.unreadable_processes += 1,
                }
            }
            processes.sort_by_key(ProcessRecord::pid);

            let observed_at_unix_millis = current_unix_millis()?;

            Ok(Snapshot {
                observed_at_unix_millis,
                current_uid,
                processes,
                coverage,
            })
        }

        pub fn lookup(&self, pid: u32) -> Result<Option<ProcessRecord>, SnapshotError> {
            let argmax = kernel_argmax()?;
            match read_process(pid, argmax) {
                Ok(process) => Ok(Some(process)),
                Err(()) if !process_exists(pid) => Ok(None),
                Err(()) => Err(SnapshotError::ProcessLookup(format!(
                    "native metadata unavailable for live pid {pid}"
                ))),
            }
        }
    }

    #[derive(Clone, Debug, Default)]
    pub struct MacosRuntime {
        snapshotter: MacosSnapshotter,
    }

    impl MacosRuntime {
        #[must_use]
        pub fn new() -> Self {
            Self {
                snapshotter: MacosSnapshotter::new(),
            }
        }
    }

    impl CleanupRuntime for MacosRuntime {
        fn snapshot(&mut self) -> Result<Snapshot, RuntimeFailure> {
            self.snapshotter
                .capture()
                .map_err(|error| RuntimeFailure::new(error.to_string()))
        }

        fn now_unix_millis(&self) -> Result<u64, RuntimeFailure> {
            current_unix_millis().map_err(|error| RuntimeFailure::new(error.to_string()))
        }

        fn signal_exact(
            &mut self,
            identity: &ProcessIdentity,
            signal: CleanupSignal,
        ) -> SignalDisposition {
            let current_uid = unsafe { libc::geteuid() };
            if current_uid == 0 || is_self_or_ancestor(identity.pid) {
                return SignalDisposition::Rejected;
            }
            let process = match self.snapshotter.lookup(identity.pid) {
                Ok(Some(process)) => process,
                Ok(None) => return SignalDisposition::AlreadyExited,
                Err(_) => return SignalDisposition::Rejected,
            };
            if process.uid != current_uid {
                return SignalDisposition::Rejected;
            }
            if !identity.exact_match(&process.identity) {
                return SignalDisposition::IdentityMismatch;
            }
            let Ok(pid) = libc::pid_t::try_from(identity.pid) else {
                return SignalDisposition::Rejected;
            };
            let raw_signal = match signal {
                CleanupSignal::Term => libc::SIGTERM,
                CleanupSignal::Kill => libc::SIGKILL,
            };
            let result = unsafe { libc::kill(pid, raw_signal) };
            if result == 0 {
                SignalDisposition::Delivered
            } else if std::io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH) {
                SignalDisposition::AlreadyExited
            } else {
                SignalDisposition::Rejected
            }
        }

        fn wait(&mut self, duration: std::time::Duration) {
            std::thread::sleep(duration);
        }
    }

    fn current_unix_millis() -> Result<u64, SnapshotError> {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| SnapshotError::Clock(error.to_string()))?
            .as_millis()
            .try_into()
            .map_err(|_| SnapshotError::Clock("wall-clock milliseconds overflowed u64".to_owned()))
    }

    fn read_process(pid: u32, argmax: usize) -> Result<ProcessRecord, ()> {
        let pid_i32 = i32::try_from(pid).map_err(|_| ())?;
        let info = pidinfo::<TaskAllInfo>(pid_i32, 0).map_err(|_| ())?;
        let path = pidpath(pid_i32).ok().filter(|path| !path.is_empty());
        let executable = path
            .as_deref()
            .and_then(|path| fs::metadata(path).ok())
            .map_or_else(ExecutableIdentity::default, |metadata| ExecutableIdentity {
                device: Some(metadata.dev()),
                inode: Some(metadata.ino()),
                size: Some(metadata.size()),
                modified_unix_nanos: Some(
                    i128::from(metadata.mtime()) * 1_000_000_000
                        + i128::from(metadata.mtime_nsec()),
                ),
            });
        let started_at_unix_micros = info
            .pbsd
            .pbi_start_tvsec
            .saturating_mul(1_000_000)
            .saturating_add(info.pbsd.pbi_start_tvusec);
        let name = decode_c_chars(&info.pbsd.pbi_name)
            .filter(|name| !name.is_empty())
            .or_else(|| decode_c_chars(&info.pbsd.pbi_comm))
            .unwrap_or_else(|| format!("pid-{pid}"));
        let tty_device = (!matches!(info.pbsd.e_tdev, 0 | u32::MAX)).then_some(info.pbsd.e_tdev);

        Ok(ProcessRecord {
            identity: ProcessIdentity {
                pid,
                started_at_unix_micros,
                executable_device: executable.device,
                executable_inode: executable.inode,
            },
            parent_pid: info.pbsd.pbi_ppid,
            process_group_id: info.pbsd.pbi_pgid,
            uid: info.pbsd.pbi_uid,
            tty_device,
            name,
            executable_path: path,
            executable,
            arguments: process_arguments(pid_i32, argmax).ok(),
            resident_memory_bytes: info.ptinfo.pti_resident_size,
            status: match info.pbsd.pbi_status {
                STATUS_RUN => ProcessStatus::Running,
                STATUS_SLEEP => ProcessStatus::Sleeping,
                STATUS_STOP => ProcessStatus::Stopped,
                STATUS_ZOMBIE => ProcessStatus::Zombie,
                other => ProcessStatus::Other(other),
            },
        })
    }

    fn process_exists(pid: u32) -> bool {
        let Ok(pid) = libc::pid_t::try_from(pid) else {
            return false;
        };
        let result = unsafe { libc::kill(pid, 0) };
        result == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
    }

    fn is_self_or_ancestor(target_pid: u32) -> bool {
        let current_pid = unsafe { libc::getpid() };
        let Ok(mut cursor) = u32::try_from(current_pid) else {
            return true;
        };
        let mut seen = std::collections::BTreeSet::new();
        loop {
            if cursor == target_pid {
                return true;
            }
            if cursor <= 1 || !seen.insert(cursor) {
                return false;
            }
            let Ok(cursor_i32) = i32::try_from(cursor) else {
                return true;
            };
            let Ok(info) = pidinfo::<TaskAllInfo>(cursor_i32, 0) else {
                return true;
            };
            cursor = info.pbsd.pbi_ppid;
        }
    }

    fn kernel_argmax() -> Result<usize, SnapshotError> {
        let mut mib = [CTL_KERN, KERN_ARGMAX];
        let mut argmax: libc::c_int = 0;
        let mut size = size_of::<libc::c_int>();
        let result = unsafe {
            libc::sysctl(
                mib.as_mut_ptr(),
                mib.len() as libc::c_uint,
                (&raw mut argmax).cast::<c_void>(),
                &raw mut size,
                std::ptr::null_mut(),
                0,
            )
        };
        if result != 0 || argmax <= 0 {
            return Err(SnapshotError::Sysctl(
                std::io::Error::last_os_error().to_string(),
            ));
        }
        usize::try_from(argmax)
            .map_err(|_| SnapshotError::Sysctl("kern.argmax does not fit usize".to_owned()))
    }

    fn process_arguments(pid: libc::c_int, argmax: usize) -> Result<Vec<String>, SnapshotError> {
        let mut mib = [CTL_KERN, KERN_PROCARGS2, pid];
        let mut buffer = vec![0_u8; argmax];
        let mut size = buffer.len();
        let result = unsafe {
            libc::sysctl(
                mib.as_mut_ptr(),
                mib.len() as libc::c_uint,
                buffer.as_mut_ptr().cast::<c_void>(),
                &raw mut size,
                std::ptr::null_mut(),
                0,
            )
        };
        if result != 0 {
            return Err(SnapshotError::Sysctl(
                std::io::Error::last_os_error().to_string(),
            ));
        }
        buffer.truncate(size);
        parse_procargs2(&buffer)
            .ok_or_else(|| SnapshotError::Sysctl(format!("malformed KERN_PROCARGS2 for pid {pid}")))
    }

    fn parse_procargs2(buffer: &[u8]) -> Option<Vec<String>> {
        let argc_bytes: [u8; size_of::<libc::c_int>()] =
            buffer.get(..size_of::<libc::c_int>())?.try_into().ok()?;
        let argc = libc::c_int::from_ne_bytes(argc_bytes);
        if argc < 0 {
            return None;
        }
        let argc = usize::try_from(argc).ok()?;
        let mut cursor = size_of::<libc::c_int>();

        cursor += buffer.get(cursor..)?.iter().position(|byte| *byte == 0)? + 1;
        while buffer.get(cursor).is_some_and(|byte| *byte == 0) {
            cursor += 1;
        }

        let mut arguments = Vec::with_capacity(argc);
        while arguments.len() < argc && cursor < buffer.len() {
            let tail = buffer.get(cursor..)?;
            let end = tail.iter().position(|byte| *byte == 0)?;
            let value = String::from_utf8_lossy(&tail[..end]).into_owned();
            arguments.push(value);
            cursor += end + 1;
            while buffer.get(cursor).is_some_and(|byte| *byte == 0) {
                cursor += 1;
            }
        }
        (arguments.len() == argc).then_some(arguments)
    }

    fn decode_c_chars<const N: usize>(bytes: &[libc::c_char; N]) -> Option<String> {
        let bytes = bytes.iter().map(|byte| *byte as u8).collect::<Vec<_>>();
        let end = bytes
            .iter()
            .position(|byte| *byte == 0)
            .unwrap_or(bytes.len());
        String::from_utf8(bytes[..end].to_vec()).ok()
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn parses_procargs2_without_environment_tail() {
            let mut bytes = 3_i32.to_ne_bytes().to_vec();
            bytes.extend_from_slice(b"/bin/tool\0\0tool\0--flag\0value\0SYNTHETIC_ENV=redacted\0");
            assert_eq!(
                parse_procargs2(&bytes),
                Some(vec![
                    "tool".to_owned(),
                    "--flag".to_owned(),
                    "value".to_owned()
                ])
            );
        }

        #[test]
        fn rejects_truncated_procargs2() {
            let mut bytes = 2_i32.to_ne_bytes().to_vec();
            bytes.extend_from_slice(b"/bin/tool\0\0tool\0");
            assert_eq!(parse_procargs2(&bytes), None);
        }
    }
}

#[cfg(target_os = "macos")]
pub use platform::{MacosRuntime, MacosSnapshotter, SnapshotError};

#[cfg(not(target_os = "macos"))]
compile_error!("unlinger-macos currently supports only macOS");
