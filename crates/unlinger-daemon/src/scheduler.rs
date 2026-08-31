use std::collections::BTreeSet;
use std::thread;
use std::time::{Duration, Instant};

use unlinger_macos::{EventMonitorError, MacosEventMonitor, RuntimeEvent};

const STOP_POLL_INTERVAL: Duration = Duration::from_millis(100);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ReconcileTrigger {
    Periodic,
    Runtime(RuntimeEvent),
    SourceFailed,
    StopRequested,
}

pub(crate) trait RuntimeEventSource {
    fn replace_process_watches(&self, pids: &BTreeSet<u32>) -> Result<(), EventMonitorError>;
    fn wait_timeout(&self, timeout: Duration) -> Result<Option<RuntimeEvent>, EventMonitorError>;
}

impl RuntimeEventSource for MacosEventMonitor {
    fn replace_process_watches(&self, pids: &BTreeSet<u32>) -> Result<(), EventMonitorError> {
        MacosEventMonitor::replace_process_watches(self, pids.iter().copied())
    }

    fn wait_timeout(&self, timeout: Duration) -> Result<Option<RuntimeEvent>, EventMonitorError> {
        MacosEventMonitor::wait_timeout(self, timeout)
    }
}

pub(crate) struct ReconciliationScheduler<S = MacosEventMonitor> {
    source: Option<S>,
    source_error: Option<String>,
    periodic_interval: Duration,
    periodic_deadline: Instant,
}

impl ReconciliationScheduler<MacosEventMonitor> {
    pub(crate) fn start(periodic_interval: Duration) -> Self {
        match MacosEventMonitor::start() {
            Ok(source) => Self::new(Some(source), periodic_interval, None),
            Err(error) => Self::new(None, periodic_interval, Some(error.to_string())),
        }
    }
}

impl<S: RuntimeEventSource> ReconciliationScheduler<S> {
    fn new(source: Option<S>, periodic_interval: Duration, source_error: Option<String>) -> Self {
        Self {
            source,
            source_error,
            periodic_interval,
            periodic_deadline: Instant::now() + periodic_interval,
        }
    }

    pub(crate) fn complete_cycle(&mut self, process_watches: &BTreeSet<u32>) {
        self.periodic_deadline = Instant::now() + self.periodic_interval;
        let Some(source) = self.source.as_ref() else {
            return;
        };
        if let Err(error) = source.replace_process_watches(process_watches) {
            self.source_error = Some(error.to_string());
            self.source = None;
        }
    }

    pub(crate) fn cycle_failed(&mut self) {
        self.periodic_deadline = Instant::now() + self.periodic_interval;
    }

    pub(crate) fn wait_for_trigger(
        &mut self,
        mut should_stop: impl FnMut() -> bool,
    ) -> ReconcileTrigger {
        loop {
            if should_stop() {
                return ReconcileTrigger::StopRequested;
            }
            let now = Instant::now();
            if now >= self.periodic_deadline {
                return ReconcileTrigger::Periodic;
            }
            let timeout = self
                .periodic_deadline
                .saturating_duration_since(now)
                .min(STOP_POLL_INTERVAL);
            let Some(source) = self.source.as_ref() else {
                thread::sleep(timeout);
                continue;
            };
            match source.wait_timeout(timeout) {
                Ok(Some(event)) => return ReconcileTrigger::Runtime(event),
                Ok(None) => {}
                Err(error) => {
                    self.source_error = Some(error.to_string());
                    self.source = None;
                    return ReconcileTrigger::SourceFailed;
                }
            }
        }
    }

    pub(crate) fn take_source_error(&mut self) -> Option<String> {
        self.source_error.take()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    use std::sync::Mutex;

    struct FakeSource {
        replies: Mutex<VecDeque<Result<Option<RuntimeEvent>, EventMonitorError>>>,
        watches: Mutex<Vec<BTreeSet<u32>>>,
    }

    impl FakeSource {
        fn with_replies(
            replies: impl IntoIterator<Item = Result<Option<RuntimeEvent>, EventMonitorError>>,
        ) -> Self {
            Self {
                replies: Mutex::new(replies.into_iter().collect()),
                watches: Mutex::new(Vec::new()),
            }
        }
    }

    impl RuntimeEventSource for FakeSource {
        fn replace_process_watches(&self, pids: &BTreeSet<u32>) -> Result<(), EventMonitorError> {
            self.watches
                .lock()
                .expect("watches lock")
                .push(pids.clone());
            Ok(())
        }

        fn wait_timeout(
            &self,
            _timeout: Duration,
        ) -> Result<Option<RuntimeEvent>, EventMonitorError> {
            self.replies
                .lock()
                .expect("replies lock")
                .pop_front()
                .unwrap_or(Ok(None))
        }
    }

    #[test]
    fn process_exit_event_wakes_before_periodic_deadline() {
        let source = FakeSource::with_replies([Ok(Some(RuntimeEvent::ProcessExited { pid: 91 }))]);
        let mut scheduler =
            ReconciliationScheduler::new(Some(source), Duration::from_secs(60), None);

        assert_eq!(
            scheduler.wait_for_trigger(|| false),
            ReconcileTrigger::Runtime(RuntimeEvent::ProcessExited { pid: 91 })
        );
    }

    #[test]
    fn system_wake_requests_an_immediate_full_scan() {
        let source = FakeSource::with_replies([Ok(Some(RuntimeEvent::SystemWake))]);
        let mut scheduler =
            ReconciliationScheduler::new(Some(source), Duration::from_secs(60), None);

        assert_eq!(
            scheduler.wait_for_trigger(|| false),
            ReconcileTrigger::Runtime(RuntimeEvent::SystemWake)
        );
    }

    #[test]
    fn memory_pressure_accelerates_only_scheduling() {
        let event = RuntimeEvent::MemoryPressure {
            level: unlinger_macos::MemoryPressureLevel::Critical,
        };
        let source = FakeSource::with_replies([Ok(Some(event))]);
        let mut scheduler =
            ReconciliationScheduler::new(Some(source), Duration::from_secs(60), None);

        assert_eq!(
            scheduler.wait_for_trigger(|| false),
            ReconcileTrigger::Runtime(event)
        );
        assert_eq!(scheduler.periodic_interval, Duration::from_secs(60));
    }

    #[test]
    fn event_source_failure_falls_back_to_periodic_scans() {
        let source = FakeSource::with_replies([Err(EventMonitorError::Stopped)]);
        let mut scheduler =
            ReconciliationScheduler::new(Some(source), Duration::from_millis(2), None);

        assert_eq!(
            scheduler.wait_for_trigger(|| false),
            ReconcileTrigger::SourceFailed
        );
        assert!(scheduler.source.is_none());
        assert!(
            scheduler
                .take_source_error()
                .is_some_and(|error| error.contains("stopped"))
        );
        assert_eq!(
            scheduler.wait_for_trigger(|| false),
            ReconcileTrigger::Periodic
        );
    }

    #[test]
    fn shutdown_interrupts_the_wait_without_a_scan() {
        let source = FakeSource::with_replies([]);
        let mut scheduler =
            ReconciliationScheduler::new(Some(source), Duration::from_secs(60), None);

        assert_eq!(
            scheduler.wait_for_trigger(|| true),
            ReconcileTrigger::StopRequested
        );
    }

    #[test]
    fn completed_cycle_replaces_exact_candidate_watches() {
        let source = FakeSource::with_replies([]);
        let mut scheduler =
            ReconciliationScheduler::new(Some(source), Duration::from_secs(60), None);
        let watches = BTreeSet::from([23, 41]);

        scheduler.complete_cycle(&watches);

        assert_eq!(
            scheduler
                .source
                .as_ref()
                .expect("source")
                .watches
                .lock()
                .expect("watches lock")
                .as_slice(),
            &[watches]
        );
    }

    #[test]
    fn failed_cycle_defers_retry_to_the_periodic_deadline() {
        let source = FakeSource::with_replies([]);
        let mut scheduler =
            ReconciliationScheduler::new(Some(source), Duration::from_secs(60), None);
        scheduler.periodic_deadline = Instant::now();

        scheduler.cycle_failed();

        assert!(scheduler.periodic_deadline > Instant::now());
    }
}
