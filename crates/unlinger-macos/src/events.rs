use std::collections::BTreeSet;
use std::error::Error;
use std::ffi::{CString, c_void};
use std::fmt::{Display, Formatter};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender, TrySendError};
use std::thread::{self, JoinHandle};
use std::time::Duration;

const DISPATCH_MEMORYPRESSURE_WARN: usize = 0x02;
const DISPATCH_MEMORYPRESSURE_CRITICAL: usize = 0x04;
const DISPATCH_PROC_EXIT: usize = 0x8000_0000;

const K_IO_MESSAGE_CAN_SYSTEM_SLEEP: u32 = 0xe000_0270;
const K_IO_MESSAGE_SYSTEM_WILL_SLEEP: u32 = 0xe000_0280;
const K_IO_MESSAGE_SYSTEM_HAS_POWERED_ON: u32 = 0xe000_0300;

type DispatchObject = *mut c_void;
type DispatchQueue = *mut c_void;
type DispatchSource = *mut c_void;
type IoNotificationPort = *mut c_void;
type IoObject = u32;
type IoConnect = u32;
type IoReturn = i32;

#[repr(C)]
struct DispatchSourceType {
    _private: [u8; 0],
}

#[link(name = "System")]
unsafe extern "C" {
    static _dispatch_source_type_memorypressure: DispatchSourceType;
    static _dispatch_source_type_proc: DispatchSourceType;

    fn dispatch_queue_create(
        label: *const libc::c_char,
        attribute: DispatchObject,
    ) -> DispatchQueue;
    fn dispatch_source_create(
        source_type: *const DispatchSourceType,
        handle: usize,
        mask: usize,
        queue: DispatchQueue,
    ) -> DispatchSource;
    fn dispatch_set_context(object: DispatchObject, context: *mut c_void);
    fn dispatch_source_set_event_handler_f(
        source: DispatchSource,
        handler: Option<unsafe extern "C" fn(*mut c_void)>,
    );
    fn dispatch_source_get_data(source: DispatchSource) -> usize;
    fn dispatch_activate(object: DispatchObject);
    fn dispatch_source_cancel(source: DispatchSource);
    fn dispatch_sync_f(
        queue: DispatchQueue,
        context: *mut c_void,
        work: Option<unsafe extern "C" fn(*mut c_void)>,
    );
    fn dispatch_release(object: DispatchObject);
}

#[link(name = "IOKit", kind = "framework")]
unsafe extern "C" {
    fn IORegisterForSystemPower(
        reference: *mut c_void,
        port: *mut IoNotificationPort,
        callback: Option<unsafe extern "C" fn(*mut c_void, IoObject, u32, *mut c_void)>,
        notifier: *mut IoObject,
    ) -> IoConnect;
    fn IONotificationPortSetDispatchQueue(port: IoNotificationPort, queue: DispatchQueue);
    fn IODeregisterForSystemPower(notifier: *mut IoObject) -> IoReturn;
    fn IONotificationPortDestroy(port: IoNotificationPort);
    fn IOAllowPowerChange(connection: IoConnect, notification_id: isize) -> IoReturn;
    fn IOServiceClose(connection: IoConnect) -> IoReturn;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum MemoryPressureLevel {
    Warning,
    Critical,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum RuntimeEvent {
    ProcessExited { pid: u32 },
    SystemWake,
    MemoryPressure { level: MemoryPressureLevel },
}

#[derive(Debug)]
pub enum EventMonitorError {
    Native(String),
    Stopped,
}

impl Display for EventMonitorError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Native(message) => write!(formatter, "native event monitor failed: {message}"),
            Self::Stopped => formatter.write_str("native event monitor stopped"),
        }
    }
}

impl Error for EventMonitorError {}

enum MonitorCommand {
    ReplaceProcessWatches {
        pids: BTreeSet<u32>,
        reply: SyncSender<Result<(), String>>,
    },
    Stop,
}

pub struct MacosEventMonitor {
    commands: SyncSender<MonitorCommand>,
    events: Receiver<RuntimeEvent>,
    thread: Option<JoinHandle<()>>,
}

impl MacosEventMonitor {
    pub fn start() -> Result<Self, EventMonitorError> {
        let (commands, command_receiver) = mpsc::sync_channel(1);
        let (event_sender, events) = mpsc::sync_channel(1);
        let (initialization_sender, initialization_receiver) = mpsc::sync_channel(1);
        let thread = thread::Builder::new()
            .name("unlinger-native-events".to_owned())
            .spawn(move || {
                monitor_thread(command_receiver, event_sender, initialization_sender);
            })
            .map_err(|error| EventMonitorError::Native(error.to_string()))?;
        match initialization_receiver.recv() {
            Ok(Ok(())) => Ok(Self {
                commands,
                events,
                thread: Some(thread),
            }),
            Ok(Err(message)) => {
                let _ = thread.join();
                Err(EventMonitorError::Native(message))
            }
            Err(_) => {
                let _ = thread.join();
                Err(EventMonitorError::Native(
                    "initialization channel closed".to_owned(),
                ))
            }
        }
    }

    pub fn replace_process_watches(
        &self,
        pids: impl IntoIterator<Item = u32>,
    ) -> Result<(), EventMonitorError> {
        let pids = pids
            .into_iter()
            .filter(|pid| *pid > 1 && *pid != std::process::id())
            .collect::<BTreeSet<_>>();
        let (reply, response) = mpsc::sync_channel(1);
        self.commands
            .send(MonitorCommand::ReplaceProcessWatches { pids, reply })
            .map_err(|_| EventMonitorError::Stopped)?;
        response
            .recv()
            .map_err(|_| EventMonitorError::Stopped)?
            .map_err(EventMonitorError::Native)
    }

    pub fn wait_timeout(
        &self,
        timeout: Duration,
    ) -> Result<Option<RuntimeEvent>, EventMonitorError> {
        match self.events.recv_timeout(timeout) {
            Ok(event) => Ok(Some(event)),
            Err(RecvTimeoutError::Timeout) => Ok(None),
            Err(RecvTimeoutError::Disconnected) => Err(EventMonitorError::Stopped),
        }
    }
}

impl Drop for MacosEventMonitor {
    fn drop(&mut self) {
        let _ = self.commands.send(MonitorCommand::Stop);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

struct EventSink {
    sender: SyncSender<RuntimeEvent>,
}

impl EventSink {
    fn send(&self, event: RuntimeEvent) {
        match self.sender.try_send(event) {
            Ok(()) | Err(TrySendError::Full(_)) | Err(TrySendError::Disconnected(_)) => {}
        }
    }
}

enum SourceKind {
    MemoryPressure,
    ProcessExit(u32),
}

struct SourceContext {
    source: DispatchSource,
    sink: *const EventSink,
    kind: SourceKind,
}

unsafe impl Send for SourceContext {}

struct SourceHandle {
    source: DispatchSource,
    context: Box<SourceContext>,
}

unsafe impl Send for SourceHandle {}

struct PowerContext {
    sink: *const EventSink,
    connection: AtomicU32,
}

unsafe impl Send for PowerContext {}

struct PowerRegistration {
    connection: IoConnect,
    port: IoNotificationPort,
    notifier: IoObject,
    _context: Box<PowerContext>,
}

unsafe impl Send for PowerRegistration {}

struct NativeState {
    queue: DispatchQueue,
    sink: Box<EventSink>,
    memory_pressure: Option<SourceHandle>,
    power: PowerRegistration,
    process_watches: Vec<SourceHandle>,
}

unsafe impl Send for NativeState {}

fn monitor_thread(
    commands: Receiver<MonitorCommand>,
    events: SyncSender<RuntimeEvent>,
    initialized: SyncSender<Result<(), String>>,
) {
    let mut state = match NativeState::new(events) {
        Ok(state) => {
            let _ = initialized.send(Ok(()));
            state
        }
        Err(message) => {
            let _ = initialized.send(Err(message));
            return;
        }
    };
    while let Ok(command) = commands.recv() {
        match command {
            MonitorCommand::ReplaceProcessWatches { pids, reply } => {
                let _ = reply.send(state.replace_process_watches(&pids));
            }
            MonitorCommand::Stop => break,
        }
    }
}

impl NativeState {
    fn new(events: SyncSender<RuntimeEvent>) -> Result<Self, String> {
        let label = CString::new("app.unlinger.native-events")
            .map_err(|error| format!("invalid dispatch queue label: {error}"))?;
        let queue = unsafe { dispatch_queue_create(label.as_ptr(), std::ptr::null_mut()) };
        if queue.is_null() {
            return Err("could not create serial dispatch queue".to_owned());
        }
        let sink = Box::new(EventSink { sender: events });
        let sink_pointer = (&raw const *sink).cast::<EventSink>();
        let memory_pressure = match create_source(
            &raw const _dispatch_source_type_memorypressure,
            0,
            DISPATCH_MEMORYPRESSURE_WARN | DISPATCH_MEMORYPRESSURE_CRITICAL,
            queue,
            sink_pointer,
            SourceKind::MemoryPressure,
        ) {
            Ok(source) => source,
            Err(error) => {
                unsafe { dispatch_release(queue) };
                return Err(error);
            }
        };
        let power = match PowerRegistration::new(queue, sink_pointer) {
            Ok(power) => power,
            Err(error) => {
                cancel_sources(queue, vec![memory_pressure]);
                unsafe { dispatch_release(queue) };
                return Err(error);
            }
        };
        Ok(Self {
            queue,
            sink,
            memory_pressure: Some(memory_pressure),
            power,
            process_watches: Vec::new(),
        })
    }

    fn replace_process_watches(&mut self, pids: &BTreeSet<u32>) -> Result<(), String> {
        let sink_pointer = (&raw const *self.sink).cast::<EventSink>();
        let mut replacements = Vec::with_capacity(pids.len());
        for pid in pids {
            let handle = match create_source(
                &raw const _dispatch_source_type_proc,
                usize::try_from(*pid).map_err(|_| format!("pid {pid} does not fit usize"))?,
                DISPATCH_PROC_EXIT,
                self.queue,
                sink_pointer,
                SourceKind::ProcessExit(*pid),
            ) {
                Ok(handle) => handle,
                Err(error) => {
                    cancel_sources(self.queue, replacements);
                    return Err(error);
                }
            };
            replacements.push(handle);
        }
        let old = std::mem::replace(&mut self.process_watches, replacements);
        cancel_sources(self.queue, old);
        Ok(())
    }
}

impl Drop for NativeState {
    fn drop(&mut self) {
        let watches = std::mem::take(&mut self.process_watches);
        cancel_sources(self.queue, watches);
        unsafe {
            IONotificationPortSetDispatchQueue(self.power.port, std::ptr::null_mut());
            let _ = IODeregisterForSystemPower(&raw mut self.power.notifier);
            dispatch_sync_f(self.queue, std::ptr::null_mut(), Some(dispatch_barrier));
            IONotificationPortDestroy(self.power.port);
            let _ = IOServiceClose(self.power.connection);
        }
        if let Some(memory) = self.memory_pressure.take() {
            cancel_sources(self.queue, vec![memory]);
        }
        unsafe { dispatch_release(self.queue) };
    }
}

impl PowerRegistration {
    fn new(queue: DispatchQueue, sink: *const EventSink) -> Result<Self, String> {
        let mut context = Box::new(PowerContext {
            sink,
            connection: AtomicU32::new(0),
        });
        let mut port = std::ptr::null_mut();
        let mut notifier = 0;
        let connection = unsafe {
            IORegisterForSystemPower(
                (&raw mut *context).cast::<c_void>(),
                &raw mut port,
                Some(power_callback),
                &raw mut notifier,
            )
        };
        if connection == 0 || port.is_null() || notifier == 0 {
            return Err("IORegisterForSystemPower returned no registration".to_owned());
        }
        context.connection.store(connection, Ordering::Release);
        unsafe { IONotificationPortSetDispatchQueue(port, queue) };
        Ok(Self {
            connection,
            port,
            notifier,
            _context: context,
        })
    }
}

fn create_source(
    source_type: *const DispatchSourceType,
    handle: usize,
    mask: usize,
    queue: DispatchQueue,
    sink: *const EventSink,
    kind: SourceKind,
) -> Result<SourceHandle, String> {
    let source = unsafe { dispatch_source_create(source_type, handle, mask, queue) };
    if source.is_null() {
        return Err("dispatch_source_create returned null".to_owned());
    }
    let mut context = Box::new(SourceContext { source, sink, kind });
    unsafe {
        dispatch_set_context(source, (&raw mut *context).cast::<c_void>());
        dispatch_source_set_event_handler_f(source, Some(source_callback));
        dispatch_activate(source);
    }
    Ok(SourceHandle { source, context })
}

fn cancel_sources(queue: DispatchQueue, sources: Vec<SourceHandle>) {
    for source in &sources {
        if !source.source.is_null() {
            unsafe { dispatch_source_cancel(source.source) };
        }
    }
    unsafe { dispatch_sync_f(queue, std::ptr::null_mut(), Some(dispatch_barrier)) };
    for source in sources {
        if !source.source.is_null() {
            unsafe { dispatch_release(source.source) };
        }
        drop(source.context);
    }
}

unsafe extern "C" fn dispatch_barrier(_context: *mut c_void) {}

unsafe extern "C" fn source_callback(context: *mut c_void) {
    if context.is_null() {
        return;
    }
    let context = unsafe { &*context.cast::<SourceContext>() };
    if context.sink.is_null() {
        return;
    }
    let sink = unsafe { &*context.sink };
    match context.kind {
        SourceKind::MemoryPressure => {
            let data = unsafe { dispatch_source_get_data(context.source) };
            sink.send(RuntimeEvent::MemoryPressure {
                level: memory_pressure_level(data),
            });
        }
        SourceKind::ProcessExit(pid) => sink.send(RuntimeEvent::ProcessExited { pid }),
    }
}

unsafe extern "C" fn power_callback(
    reference: *mut c_void,
    _service: IoObject,
    message_type: u32,
    message_argument: *mut c_void,
) {
    if reference.is_null() {
        return;
    }
    let context = unsafe { &*reference.cast::<PowerContext>() };
    if matches!(
        message_type,
        K_IO_MESSAGE_CAN_SYSTEM_SLEEP | K_IO_MESSAGE_SYSTEM_WILL_SLEEP
    ) {
        let connection = context.connection.load(Ordering::Acquire);
        if connection != 0 {
            let _ = unsafe { IOAllowPowerChange(connection, message_argument as isize) };
        }
    } else if message_type == K_IO_MESSAGE_SYSTEM_HAS_POWERED_ON && !context.sink.is_null() {
        unsafe { &*context.sink }.send(RuntimeEvent::SystemWake);
    }
}

fn memory_pressure_level(data: usize) -> MemoryPressureLevel {
    if data & DISPATCH_MEMORYPRESSURE_CRITICAL != 0 {
        MemoryPressureLevel::Critical
    } else {
        MemoryPressureLevel::Warning
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;
    use std::time::Instant;

    #[test]
    fn memory_pressure_mapping_prefers_critical() {
        assert_eq!(
            memory_pressure_level(DISPATCH_MEMORYPRESSURE_WARN),
            MemoryPressureLevel::Warning
        );
        assert_eq!(
            memory_pressure_level(DISPATCH_MEMORYPRESSURE_WARN | DISPATCH_MEMORYPRESSURE_CRITICAL),
            MemoryPressureLevel::Critical
        );
    }

    #[test]
    fn bounded_event_channel_coalesces_a_storm() {
        let (sender, receiver) = mpsc::sync_channel(1);
        let sink = EventSink { sender };

        sink.send(RuntimeEvent::SystemWake);
        sink.send(RuntimeEvent::MemoryPressure {
            level: MemoryPressureLevel::Critical,
        });

        assert_eq!(receiver.try_recv(), Ok(RuntimeEvent::SystemWake));
        assert!(receiver.try_recv().is_err());
    }

    #[test]
    fn owned_child_exit_wakes_the_native_monitor() {
        let monitor = MacosEventMonitor::start().expect("start native monitor");
        let mut child = Command::new("/bin/sleep")
            .arg("30")
            .spawn()
            .expect("spawn owned child");
        monitor
            .replace_process_watches([child.id()])
            .expect("watch owned child");
        child.kill().expect("kill owned child");
        child.wait().expect("reap owned child");

        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            let event = monitor
                .wait_timeout(remaining)
                .expect("wait for process exit");
            match event {
                Some(RuntimeEvent::ProcessExited { pid }) if pid == child.id() => break,
                Some(_) if Instant::now() < deadline => continue,
                _ => panic!("owned process exit event was not delivered"),
            }
        }
    }

    #[test]
    fn process_watch_replacement_filters_system_and_self_pids() {
        let monitor = MacosEventMonitor::start().expect("start native monitor");
        monitor
            .replace_process_watches([0, 1, std::process::id()])
            .expect("replace with empty safe set");
        assert_eq!(
            monitor
                .wait_timeout(Duration::from_millis(5))
                .expect("quiet monitor"),
            None
        );
    }
}
