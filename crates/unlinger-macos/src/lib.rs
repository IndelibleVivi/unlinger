#[cfg(target_os = "macos")]
mod artifacts;

#[cfg(target_os = "macos")]
mod descriptors;

#[cfg(target_os = "macos")]
mod events;

#[cfg(target_os = "macos")]
mod version;

#[cfg(target_os = "macos")]
mod storage_residue;

#[cfg(target_os = "macos")]
mod task_sessions;

#[cfg(target_os = "macos")]
fn parse_procargs2(buffer: &[u8]) -> Option<Vec<String>> {
    parse_procargs2_task(buffer).map(|(arguments, _)| arguments)
}

#[cfg(target_os = "macos")]
fn parse_procargs2_task(buffer: &[u8]) -> Option<(Vec<String>, Option<String>)> {
    let argc_bytes: [u8; std::mem::size_of::<libc::c_int>()] = buffer
        .get(..std::mem::size_of::<libc::c_int>())?
        .try_into()
        .ok()?;
    let argc = libc::c_int::from_ne_bytes(argc_bytes);
    if argc < 0 {
        return None;
    }
    let argc = usize::try_from(argc).ok()?;
    let mut cursor = std::mem::size_of::<libc::c_int>();

    // KERN_PROCARGS2 starts with argc and the executable path, followed by
    // NUL padding before argv[0]. Once argv begins, every NUL terminates one
    // argument; consecutive NULs therefore represent legitimate empty argv
    // entries and must not be collapsed into padding.
    cursor += buffer.get(cursor..)?.iter().position(|byte| *byte == 0)? + 1;
    while buffer.get(cursor).is_some_and(|byte| *byte == 0) {
        cursor += 1;
    }

    let mut arguments = Vec::with_capacity(argc);
    while arguments.len() < argc && cursor < buffer.len() {
        let tail = buffer.get(cursor..)?;
        let end = tail.iter().position(|byte| *byte == 0)?;
        arguments.push(String::from_utf8_lossy(&tail[..end]).into_owned());
        cursor += end + 1;
    }
    if arguments.len() != argc {
        return None;
    }
    // Retain only the issued task selector. No other environment values leave
    // this native buffer or enter snapshots, history, IPC, or diagnostics.
    let session = buffer
        .get(cursor..)?
        .split(|byte| *byte == 0)
        .find_map(|entry| {
            let value = entry.strip_prefix(b"PLAYWRIGHT_CLI_SESSION=")?;
            let value = std::str::from_utf8(value).ok()?;
            unlinger_core::task_id_from_session(value).map(|_| value.to_owned())
        });
    Some((arguments, session))
}

#[cfg(target_os = "macos")]
mod platform {
    use crate::parse_procargs2_task;
    use libproc::libproc::file_info::{ListFDs, ProcFDType, pidfdinfo};
    use libproc::libproc::net_info::{SocketFDInfo, SocketInfoKind, TcpSIState};
    use libproc::libproc::proc_pid::{listpidinfo, pidinfo, pidpath};
    use libproc::libproc::task_info::TaskAllInfo;
    use libproc::processes::{ProcFilter, pids_by_type};
    use std::error::Error;
    use std::ffi::c_void;
    use std::fmt::{Display, Formatter};
    use std::fs::OpenOptions;
    use std::io::Read;
    use std::mem::size_of;
    use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
    use std::path::Path;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};
    use unlinger_core::{
        CleanupRuntime, CleanupSignal, ClockSample, ExecutableIdentity, ProcessIdentity,
        ProcessRecord, ProcessRuntimeFacts, ProcessStatus, RuntimeFailure, SignalDisposition,
        Snapshot, SnapshotCoverage, WaitOutcome, fingerprint_parts,
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
            let mut coverage = SnapshotCoverage::default();
            let mut processes = Vec::with_capacity(pids.len());

            for pid in pids {
                match read_live_process(pid, argmax) {
                    Ok(process) => {
                        coverage.listed_processes += 1;
                        coverage.inspected_processes += 1;
                        if process.arguments.is_none() {
                            coverage.arguments_unavailable += 1;
                        }
                        if !process.executable.is_complete() {
                            coverage.executable_identity_unavailable += 1;
                        }
                        if !process.runtime.descriptor_facts_complete {
                            coverage.descriptor_facts_unavailable += 1;
                        }
                        processes.push(process);
                    }
                    Err(ProcessReadFailure::GoneOrZombie) => {}
                    Err(ProcessReadFailure::Unreadable) => {
                        coverage.listed_processes += 1;
                        coverage.unreadable_processes += 1;
                    }
                }
            }
            processes.sort_by_key(ProcessRecord::pid);

            let observed_at_unix_millis = current_clock_sample()?.wall_unix_millis;

            Ok(Snapshot {
                observed_at_unix_millis,
                current_uid,
                processes,
                coverage,
            })
        }

        pub fn lookup(&self, pid: u32) -> Result<Option<ProcessRecord>, SnapshotError> {
            let argmax = kernel_argmax()?;
            match read_live_process(pid, argmax) {
                Ok(process) => Ok(Some(process)),
                Err(ProcessReadFailure::GoneOrZombie) => Ok(None),
                Err(ProcessReadFailure::Unreadable) => Err(SnapshotError::ProcessLookup(format!(
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

        fn lookup_process(&mut self, pid: u32) -> Result<Option<ProcessRecord>, RuntimeFailure> {
            self.snapshotter
                .lookup(pid)
                .map_err(|error| RuntimeFailure::new(error.to_string()))
        }

        fn clock_sample(&self) -> Result<ClockSample, RuntimeFailure> {
            current_clock_sample().map_err(|error| RuntimeFailure::new(error.to_string()))
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

        fn wait_until(
            &mut self,
            duration: Duration,
            should_stop: &mut dyn FnMut() -> bool,
        ) -> Result<WaitOutcome, RuntimeFailure> {
            if should_stop() {
                return Ok(WaitOutcome::Interrupted);
            }
            let duration_nanos = u64::try_from(duration.as_nanos())
                .map_err(|_| RuntimeFailure::new("wait duration overflowed u64 nanoseconds"))?;
            let started =
                continuous_nanos().map_err(|error| RuntimeFailure::new(error.to_string()))?;
            let deadline = started
                .checked_add(duration_nanos)
                .ok_or_else(|| RuntimeFailure::new("wait deadline overflowed u64 nanoseconds"))?;
            loop {
                if should_stop() {
                    return Ok(WaitOutcome::Interrupted);
                }
                let now =
                    continuous_nanos().map_err(|error| RuntimeFailure::new(error.to_string()))?;
                if now >= deadline {
                    return Ok(WaitOutcome::DeadlineReached);
                }
                let remaining = Duration::from_nanos(deadline - now);
                std::thread::sleep(remaining.min(Duration::from_millis(100)));
            }
        }

        fn freeze_artifact(
            &mut self,
            candidate: &unlinger_core::RuntimeArtifactCandidate,
        ) -> Result<unlinger_core::ArtifactFreeze, RuntimeFailure> {
            crate::artifacts::freeze(candidate)
        }

        fn remove_artifact_exact(
            &mut self,
            artifact: &unlinger_core::FrozenRuntimeArtifact,
        ) -> unlinger_core::ArtifactDisposition {
            crate::artifacts::remove_exact(artifact)
        }
    }

    fn current_clock_sample() -> Result<ClockSample, SnapshotError> {
        Ok(ClockSample {
            wall_unix_millis: current_unix_millis()?,
            continuous_millis: continuous_nanos()? / 1_000_000,
            boot_session_fingerprint: boot_session_fingerprint()?,
        })
    }

    fn current_unix_millis() -> Result<u64, SnapshotError> {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| SnapshotError::Clock(error.to_string()))?
            .as_millis()
            .try_into()
            .map_err(|_| SnapshotError::Clock("wall-clock milliseconds overflowed u64".to_owned()))
    }

    fn continuous_nanos() -> Result<u64, SnapshotError> {
        let mut value = libc::timespec {
            tv_sec: 0,
            tv_nsec: 0,
        };
        if unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC_RAW, &raw mut value) } != 0 {
            return Err(SnapshotError::Clock(format!(
                "continuous clock unavailable: {}",
                std::io::Error::last_os_error()
            )));
        }
        if value.tv_sec < 0 || !(0..1_000_000_000).contains(&value.tv_nsec) {
            return Err(SnapshotError::Clock(
                "continuous clock returned an invalid timespec".to_owned(),
            ));
        }
        let seconds = u64::try_from(value.tv_sec)
            .map_err(|_| SnapshotError::Clock("continuous seconds overflowed u64".to_owned()))?;
        let nanos = u64::try_from(value.tv_nsec)
            .map_err(|_| SnapshotError::Clock("continuous nanoseconds were negative".to_owned()))?;
        seconds
            .checked_mul(1_000_000_000)
            .and_then(|total| total.checked_add(nanos))
            .ok_or_else(|| SnapshotError::Clock("continuous nanoseconds overflowed u64".to_owned()))
    }

    fn boot_session_fingerprint() -> Result<String, SnapshotError> {
        let raw = boot_session_uuid().or_else(|_| boot_time_identity())?;
        Ok(fingerprint_parts([
            b"unlinger.boot-session.v1".as_slice(),
            raw.as_slice(),
        ]))
    }

    fn boot_session_uuid() -> Result<Vec<u8>, SnapshotError> {
        let name = b"kern.bootsessionuuid\0";
        let mut size = 0_usize;
        if unsafe {
            libc::sysctlbyname(
                name.as_ptr().cast(),
                std::ptr::null_mut(),
                &raw mut size,
                std::ptr::null_mut(),
                0,
            )
        } != 0
            || size <= 1
        {
            return Err(SnapshotError::Sysctl(format!(
                "kern.bootsessionuuid size unavailable: {}",
                std::io::Error::last_os_error()
            )));
        }
        let mut value = vec![0_u8; size];
        if unsafe {
            libc::sysctlbyname(
                name.as_ptr().cast(),
                value.as_mut_ptr().cast::<c_void>(),
                &raw mut size,
                std::ptr::null_mut(),
                0,
            )
        } != 0
        {
            return Err(SnapshotError::Sysctl(format!(
                "kern.bootsessionuuid unavailable: {}",
                std::io::Error::last_os_error()
            )));
        }
        value.truncate(size);
        while value.last() == Some(&0) {
            value.pop();
        }
        if value.is_empty() {
            Err(SnapshotError::Sysctl(
                "kern.bootsessionuuid was empty".to_owned(),
            ))
        } else {
            Ok(value)
        }
    }

    fn boot_time_identity() -> Result<Vec<u8>, SnapshotError> {
        let mut mib = [CTL_KERN, libc::KERN_BOOTTIME];
        let mut boot_time = libc::timeval {
            tv_sec: 0,
            tv_usec: 0,
        };
        let mut size = size_of::<libc::timeval>();
        if unsafe {
            libc::sysctl(
                mib.as_mut_ptr(),
                mib.len() as libc::c_uint,
                (&raw mut boot_time).cast::<c_void>(),
                &raw mut size,
                std::ptr::null_mut(),
                0,
            )
        } != 0
            || size != size_of::<libc::timeval>()
        {
            return Err(SnapshotError::Sysctl(format!(
                "kern.boottime unavailable: {}",
                std::io::Error::last_os_error()
            )));
        }
        if boot_time.tv_sec <= 0 || boot_time.tv_usec < 0 {
            return Err(SnapshotError::Sysctl(
                "kern.boottime returned an invalid timeval".to_owned(),
            ));
        }
        Ok(format!("{}:{}", boot_time.tv_sec, boot_time.tv_usec).into_bytes())
    }

    enum ProcessReadFailure {
        GoneOrZombie,
        Unreadable,
    }

    fn read_live_process(pid: u32, argmax: usize) -> Result<ProcessRecord, ProcessReadFailure> {
        match read_process(pid, argmax) {
            Ok(process) if process.status == ProcessStatus::Zombie => {
                Err(ProcessReadFailure::GoneOrZombie)
            }
            Ok(process) => Ok(process),
            Err(()) => match kern_process_status(pid) {
                Some(STATUS_ZOMBIE) => Err(ProcessReadFailure::GoneOrZombie),
                Some(_) => Err(ProcessReadFailure::Unreadable),
                None if !process_exists(pid) => Err(ProcessReadFailure::GoneOrZombie),
                None => Err(ProcessReadFailure::Unreadable),
            },
        }
    }

    /// Layout prefix from Darwin's documented `extern_proc`. `kinfo_proc`
    /// begins with this structure, and only `p_stat` is read.
    #[repr(C)]
    struct ExternProcStatusPrefix {
        p_un: [*mut c_void; 2],
        p_vmspace: *mut c_void,
        p_sigacts: *mut c_void,
        p_flag: libc::c_int,
        p_stat: libc::c_char,
    }

    fn kern_process_status(pid: u32) -> Option<u32> {
        let pid = libc::c_int::try_from(pid).ok()?;
        let mut mib = [CTL_KERN, libc::KERN_PROC, libc::KERN_PROC_PID, pid];
        let mut size = 0_usize;
        if unsafe {
            libc::sysctl(
                mib.as_mut_ptr(),
                mib.len() as libc::c_uint,
                std::ptr::null_mut(),
                &raw mut size,
                std::ptr::null_mut(),
                0,
            )
        } != 0
            || size == 0
        {
            return None;
        }

        let mut bytes = vec![0_u8; size];
        if unsafe {
            libc::sysctl(
                mib.as_mut_ptr(),
                mib.len() as libc::c_uint,
                bytes.as_mut_ptr().cast::<c_void>(),
                &raw mut size,
                std::ptr::null_mut(),
                0,
            )
        } != 0
        {
            return None;
        }

        let status_offset = std::mem::offset_of!(ExternProcStatusPrefix, p_stat);
        bytes
            .get(..size)?
            .get(status_offset)
            .copied()
            .map(u32::from)
    }

    pub(crate) fn is_confirmed_zombie(pid: u32) -> bool {
        kern_process_status(pid) == Some(STATUS_ZOMBIE)
    }

    fn read_process(pid: u32, argmax: usize) -> Result<ProcessRecord, ()> {
        let pid_i32 = i32::try_from(pid).map_err(|_| ())?;
        let info = pidinfo::<TaskAllInfo>(pid_i32, 0).map_err(|_| ())?;
        let path = pidpath(pid_i32).ok().filter(|path| !path.is_empty());
        let executable = path
            .as_deref()
            .and_then(|path| {
                OpenOptions::new()
                    .read(true)
                    .custom_flags(libc::O_NOFOLLOW_ANY | libc::O_CLOEXEC)
                    .open(path)
                    .ok()
            })
            .and_then(|file| file.metadata().ok())
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
        let parsed_arguments = process_arguments(pid_i32, argmax).ok();
        let task_session_facts_complete = parsed_arguments.is_some();
        let (arguments, task_session_name) = parsed_arguments
            .map_or((None, None), |(arguments, session)| {
                (Some(arguments), session)
            });
        let mut runtime = process_runtime_facts(pid_i32, info.pbsd.pbi_nfiles, &arguments);
        runtime.task_session_facts_complete = task_session_facts_complete;
        runtime.task_session_name = task_session_name;
        runtime.playwright_cli = arguments.as_deref().and_then(|arguments| {
            crate::task_sessions::collect(arguments, &runtime.unix_socket_fingerprints)
        });
        runtime.app_bundle = path
            .as_deref()
            .and_then(|path| crate::version::collect_app_bundle_version(Path::new(path)));
        runtime.crashpad_bundle = path
            .as_deref()
            .filter(|path| path.ends_with("/chrome_crashpad_handler"))
            .and_then(|path| crate::version::collect_crashpad_bundle_version(Path::new(path)));

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
            arguments,
            resident_memory_bytes: info.ptinfo.pti_resident_size,
            status: match info.pbsd.pbi_status {
                STATUS_RUN => ProcessStatus::Running,
                STATUS_SLEEP => ProcessStatus::Sleeping,
                STATUS_STOP => ProcessStatus::Stopped,
                STATUS_ZOMBIE => ProcessStatus::Zombie,
                other => ProcessStatus::Other(other),
            },
            runtime: ProcessRuntimeFacts {
                cpu_total_nanos: info
                    .ptinfo
                    .pti_total_user
                    .saturating_add(info.ptinfo.pti_total_system),
                ..runtime
            },
        })
    }

    fn process_runtime_facts(
        pid: i32,
        declared_file_count: u32,
        arguments: &Option<Vec<String>>,
    ) -> ProcessRuntimeFacts {
        let Some(fds) = crate::descriptors::read_complete_list(declared_file_count, |capacity| {
            listpidinfo::<ListFDs>(pid, capacity)
        }) else {
            return ProcessRuntimeFacts::default();
        };
        let mut facts = ProcessRuntimeFacts {
            open_file_descriptors: fds.len(),
            descriptor_facts_complete: true,
            ..ProcessRuntimeFacts::default()
        };
        for fd in fds {
            let ProcFDType::Socket = ProcFDType::from(fd.proc_fdtype) else {
                continue;
            };
            let Ok(socket) = pidfdinfo::<SocketFDInfo>(pid, fd.proc_fd) else {
                facts.descriptor_facts_complete = false;
                continue;
            };
            match SocketInfoKind::from(socket.psi.soi_kind) {
                SocketInfoKind::Tcp => {
                    let tcp = unsafe { socket.psi.soi_proto.pri_tcp };
                    let Some(port) = network_port(tcp.tcpsi_ini.insi_lport) else {
                        facts.descriptor_facts_complete = false;
                        continue;
                    };
                    match TcpSIState::from(tcp.tcpsi_state) {
                        TcpSIState::Listen => facts.tcp_listening_ports.push(port),
                        TcpSIState::Established => {
                            facts.tcp_established_local_ports.push(port);
                        }
                        _ => {}
                    }
                }
                SocketInfoKind::Un => {
                    let unix = unsafe { socket.psi.soi_proto.pri_un };
                    if unix.unsi_conn_so != 0 {
                        facts.connected_unix_sockets += 1;
                    }
                    let address = unsafe { unix.unsi_addr.ua_sun };
                    if let Some(path) =
                        decode_c_chars(&address.sun_path).filter(|path| !path.is_empty())
                    {
                        let fingerprint = fingerprint_parts([path.as_bytes()]);
                        if unix.unsi_conn_so != 0 {
                            facts
                                .connected_named_unix_socket_fingerprints
                                .push(fingerprint.clone());
                        }
                        facts.unix_socket_fingerprints.push(fingerprint);
                    }
                }
                _ => {}
            }
        }
        facts.tcp_listening_ports.sort_unstable();
        facts.tcp_listening_ports.dedup();
        facts.tcp_established_local_ports.sort_unstable();
        facts.tcp_established_local_ports.dedup();

        let has_debug_port = arguments
            .as_ref()
            .is_some_and(|arguments| has_flag(arguments, "--remote-debugging-port"));
        let requested_debug_port = arguments
            .as_ref()
            .and_then(|arguments| debug_port(arguments));
        facts.debug_transport_facts_complete =
            facts.descriptor_facts_complete && (!has_debug_port || requested_debug_port.is_some());
        if let Some(port) = requested_debug_port {
            facts.attached_debug_transport = facts.descriptor_facts_complete
                && facts.tcp_established_local_ports.contains(&port);
        }
        facts
    }

    fn network_port(raw: libc::c_int) -> Option<u16> {
        let narrowed = u16::try_from(raw & i32::from(u16::MAX)).ok()?;
        Some(u16::from_be(narrowed))
    }

    fn debug_port(arguments: &[String]) -> Option<u16> {
        let value = flag_value(arguments, "--remote-debugging-port")?;
        let configured = value.parse::<u16>().ok()?;
        if configured != 0 {
            return Some(configured);
        }
        let profile = Path::new(flag_value(arguments, "--user-data-dir")?);
        if !profile.is_absolute() {
            return None;
        }
        read_devtools_active_port(&profile.join("DevToolsActivePort"))
    }

    fn read_devtools_active_port(path: &Path) -> Option<u16> {
        let file = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW_ANY | libc::O_CLOEXEC)
            .open(path)
            .ok()?;
        let metadata = file.metadata().ok()?;
        if !metadata.file_type().is_file() || metadata.uid() != unsafe { libc::geteuid() } {
            return None;
        }
        let mut bytes = Vec::new();
        file.take(64).read_to_end(&mut bytes).ok()?;
        let first_line = bytes.split(|byte| *byte == b'\n').next()?;
        std::str::from_utf8(first_line).ok()?.parse::<u16>().ok()
    }

    fn has_flag(arguments: &[String], name: &str) -> bool {
        arguments
            .iter()
            .any(|argument| argument == name || argument.starts_with(&format!("{name}=")))
    }

    fn flag_value<'a>(arguments: &'a [String], name: &str) -> Option<&'a str> {
        for (index, argument) in arguments.iter().enumerate() {
            if argument == name {
                return arguments.get(index + 1).map(String::as_str);
            }
            if let Some(value) = argument.strip_prefix(&format!("{name}=")) {
                return Some(value);
            }
        }
        None
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

    fn process_arguments(
        pid: libc::c_int,
        argmax: usize,
    ) -> Result<(Vec<String>, Option<String>), SnapshotError> {
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
        parse_procargs2_task(&buffer)
            .ok_or_else(|| SnapshotError::Sysctl(format!("malformed KERN_PROCARGS2 for pid {pid}")))
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
        use crate::parse_procargs2;
        use std::time::Instant;

        struct OwnedZombie {
            controller_pid: libc::pid_t,
            zombie_pid: libc::pid_t,
        }

        extern "C" fn retain_exited_child(_: libc::c_int) {}

        impl OwnedZombie {
            fn spawn() -> Self {
                let mut pipe = [-1; 2];
                assert_eq!(unsafe { libc::pipe(pipe.as_mut_ptr()) }, 0);
                let controller_pid = unsafe { libc::fork() };
                assert!(controller_pid >= 0, "fork an owned zombie controller");
                if controller_pid == 0 {
                    unsafe {
                        libc::close(pipe[0]);
                        let mut action = std::mem::zeroed::<libc::sigaction>();
                        action.sa_sigaction = retain_exited_child as *const () as usize;
                        libc::sigemptyset(&raw mut action.sa_mask);
                        action.sa_flags = 0;
                        if libc::sigaction(libc::SIGCHLD, &raw const action, std::ptr::null_mut())
                            != 0
                        {
                            libc::_exit(126);
                        }
                        let zombie_pid = libc::fork();
                        if zombie_pid == 0 {
                            libc::_exit(0);
                        }
                        let bytes = zombie_pid.to_ne_bytes();
                        let _ = libc::write(pipe[1], bytes.as_ptr().cast(), bytes.len());
                        libc::close(pipe[1]);
                        loop {
                            libc::pause();
                        }
                    }
                }
                unsafe { libc::close(pipe[1]) };
                let mut bytes = [0_u8; size_of::<libc::pid_t>()];
                let mut offset = 0;
                while offset < bytes.len() {
                    let read = unsafe {
                        libc::read(
                            pipe[0],
                            bytes[offset..].as_mut_ptr().cast(),
                            bytes.len() - offset,
                        )
                    };
                    assert!(read > 0, "read owned zombie PID from controller");
                    offset += usize::try_from(read).expect("positive pipe read length");
                }
                unsafe { libc::close(pipe[0]) };
                let zombie_pid = libc::pid_t::from_ne_bytes(bytes);
                assert!(zombie_pid > 0, "controller forked an owned zombie");
                Self {
                    controller_pid,
                    zombie_pid,
                }
            }

            fn pid(&self) -> u32 {
                u32::try_from(self.zombie_pid).expect("owned zombie PID")
            }

            fn wait_until_zombie(&self) {
                let deadline = Instant::now() + Duration::from_secs(2);
                loop {
                    let kern_status = kern_process_status(self.pid());
                    if kern_status == Some(STATUS_ZOMBIE) {
                        return;
                    }
                    if Instant::now() >= deadline {
                        let task_status = i32::try_from(self.pid())
                            .ok()
                            .and_then(|pid| pidinfo::<TaskAllInfo>(pid, 0).ok())
                            .map(|info| info.pbsd.pbi_status);
                        panic!(
                            "owned child never reached zombie state: kern_status={kern_status:?}, task_status={task_status:?}, process_exists={}",
                            process_exists(self.pid())
                        );
                    }
                    std::thread::sleep(Duration::from_millis(10));
                }
            }
        }

        impl Drop for OwnedZombie {
            fn drop(&mut self) {
                let mut status = 0;
                unsafe {
                    libc::kill(self.controller_pid, libc::SIGKILL);
                    libc::waitpid(self.controller_pid, &raw mut status, 0);
                }
                let deadline = Instant::now() + Duration::from_secs(2);
                while process_exists(self.pid()) && Instant::now() < deadline {
                    std::thread::sleep(Duration::from_millis(10));
                }
            }
        }

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
        fn procargs_retains_only_a_valid_task_selector() {
            let mut bytes = 1_i32.to_ne_bytes().to_vec();
            bytes.extend_from_slice(b"/bin/tool\0\0tool\0TOKEN=synthetic-secret\0PLAYWRIGHT_CLI_SESSION=unlinger-0123456789abcdef0123456789abcdef\0HOME=/synthetic/private\0");
            let (arguments, session) = crate::parse_procargs2_task(&bytes).unwrap();
            assert_eq!(arguments, ["tool"]);
            assert_eq!(
                session.as_deref(),
                Some("unlinger-0123456789abcdef0123456789abcdef")
            );
            let mut bytes = 1_i32.to_ne_bytes().to_vec();
            bytes.extend_from_slice(b"/bin/tool\0\0tool\0PLAYWRIGHT_CLI_SESSION=default\0");
            assert_eq!(crate::parse_procargs2_task(&bytes).unwrap().1, None);
        }

        #[test]
        fn rejects_truncated_procargs2() {
            let mut bytes = 2_i32.to_ne_bytes().to_vec();
            bytes.extend_from_slice(b"/bin/tool\0\0tool\0");
            assert_eq!(parse_procargs2(&bytes), None);
        }

        #[test]
        fn clock_sample_is_continuous_and_redacts_the_boot_identity() {
            let first = current_clock_sample().expect("first clock sample");
            let second = current_clock_sample().expect("second clock sample");

            assert!(second.continuous_millis >= first.continuous_millis);
            assert_eq!(
                second.boot_session_fingerprint,
                first.boot_session_fingerprint
            );
            assert_eq!(first.boot_session_fingerprint.len(), 16);
            assert!(
                first
                    .boot_session_fingerprint
                    .bytes()
                    .all(|byte| byte.is_ascii_hexdigit())
            );
        }

        #[test]
        fn wait_observes_a_drain_request_without_sleeping() {
            let mut runtime = MacosRuntime::new();
            let mut stop = || true;

            assert_eq!(
                runtime
                    .wait_until(Duration::from_secs(60), &mut stop)
                    .expect("interruptible wait"),
                WaitOutcome::Interrupted
            );
        }

        #[test]
        fn a_confirmed_zombie_is_not_projected_as_a_live_unreadable_process() {
            use libproc::libproc::bsd_info::BSDInfo;

            let child = OwnedZombie::spawn();
            child.wait_until_zombie();
            let pid = i32::try_from(child.pid()).expect("owned zombie pid fits i32");

            if let Ok(info) = pidinfo::<TaskAllInfo>(pid, 0) {
                assert_eq!(info.pbsd.pbi_status, STATUS_ZOMBIE);
            }
            if let Ok(info) = pidinfo::<BSDInfo>(pid, 0) {
                assert_eq!(info.pbi_status, STATUS_ZOMBIE);
            }
            assert!(process_exists(child.pid()));

            assert!(
                MacosSnapshotter::new()
                    .lookup(child.pid())
                    .expect("lookup owned zombie")
                    .is_none()
            );
            assert!(matches!(
                read_live_process(child.pid(), kernel_argmax().expect("kernel argmax")),
                Err(ProcessReadFailure::GoneOrZombie)
            ));

            let snapshot = MacosSnapshotter::new()
                .capture()
                .expect("capture live table");
            assert!(
                snapshot
                    .processes
                    .iter()
                    .all(|process| process.pid() != child.pid())
            );
            assert_eq!(
                snapshot.coverage.listed_processes,
                snapshot.coverage.inspected_processes + snapshot.coverage.unreadable_processes
            );
            assert_eq!(
                snapshot.processes.len(),
                snapshot.coverage.inspected_processes
            );
        }

        #[test]
        fn stale_descriptor_hint_does_not_omit_an_owned_debug_connection() {
            use std::net::{TcpListener, TcpStream};
            // Other tests deliberately churn descriptors. Run this
            // native assertion in its own test-owned, signal-free child.
            const CHILD: &str = "UNLINGER_OWNED_DESCRIPTOR_PROBE";
            if std::env::var_os(CHILD).is_none() {
                let output = std::process::Command::new(std::env::current_exe().unwrap())
                    .args(["--exact", "platform::tests::stale_descriptor_hint_does_not_omit_an_owned_debug_connection", "--nocapture"])
                    .env(CHILD, "1")
                    .output().expect("run isolated descriptor probe");
                assert!(
                    output.status.success(),
                    "{}",
                    String::from_utf8_lossy(&output.stderr)
                );
                assert!(
                    String::from_utf8_lossy(&output.stdout)
                        .contains("owned-descriptor-probe-passed")
                );
                return;
            }

            let files: Vec<_> = (0..130)
                .map(|_| std::fs::File::open("/dev/null").expect("owned test descriptor"))
                .collect();
            let listener = TcpListener::bind("127.0.0.1:0").expect("owned listener");
            let port = listener.local_addr().unwrap().port();
            let _client = TcpStream::connect(("127.0.0.1", port)).expect("owned client");
            let (_server, _) = listener.accept().expect("owned server");
            let arguments = Some(vec![format!("--remote-debugging-port={port}")]);
            let facts = process_runtime_facts(std::process::id() as i32, 1, &arguments);
            assert!(facts.descriptor_facts_complete);
            assert!(facts.open_file_descriptors >= files.len());
            assert!(facts.tcp_established_local_ports.contains(&port));
            assert!(facts.attached_debug_transport);
            println!("owned-descriptor-probe-passed");
        }

        #[test]
        fn captures_transient_cpu_and_debug_socket_facts() {
            use std::io::{Read, Write};
            use std::net::{TcpListener, TcpStream};

            let listener = TcpListener::bind("127.0.0.1:0").expect("bind listener");
            let port = listener.local_addr().expect("listener address").port();
            let mut client = TcpStream::connect(("127.0.0.1", port)).expect("connect client");
            let (mut server, _) = listener.accept().expect("accept client");
            client.write_all(b"x").expect("write client byte");
            let mut byte = [0_u8; 1];
            server.read_exact(&mut byte).expect("read client byte");

            let process = MacosSnapshotter::new()
                .lookup(std::process::id())
                .expect("lookup current process")
                .expect("current process exists");

            assert!(process.runtime.cpu_total_nanos > 0);
            assert!(process.runtime.descriptor_facts_complete);
            assert!(process.runtime.open_file_descriptors >= 3);
            assert!(process.runtime.tcp_listening_ports.contains(&port));
            assert!(process.runtime.tcp_established_local_ports.contains(&port));
        }
    }
}

#[cfg(target_os = "macos")]
pub use events::{EventMonitorError, MacosEventMonitor, MemoryPressureLevel, RuntimeEvent};

#[cfg(target_os = "macos")]
pub use platform::{MacosRuntime, MacosSnapshotter, SnapshotError};

#[cfg(target_os = "macos")]
pub use storage_residue::{inspect_code_sign_clone_root, observe_chrome_code_sign_clones};

#[cfg(not(target_os = "macos"))]
compile_error!("unlinger-macos currently supports only macOS");
