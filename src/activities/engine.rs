//! Platform-independent scheduling and provider lifecycle contracts.
//! Providers own their OS resources; the engine owns presentation ordering only.
use std::cmp::Reverse;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProviderState {
    Stopped,
    Starting,
    Running,
    Suspended,
    Failed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProviderEvent {
    Start,
    Ready,
    Suspend,
    Resume,
    Fail,
    Stop,
}

impl ProviderState {
    /// Invalid events leave the state unchanged and return false. Stop is idempotent;
    /// failed providers must release resources (Stop) before restarting.
    pub fn apply(&mut self, event: ProviderEvent) -> bool {
        use ProviderEvent::*;
        use ProviderState::*;
        let next = match (*self, event) {
            (_, Stop) => Stopped,
            (Stopped, Start) => Starting,
            (Starting, Ready) => Running,
            (Running, Suspend) => Suspended,
            (Suspended, Resume) => Running,
            (Starting | Running | Suspended, Fail) => Failed,
            _ => return false,
        };
        *self = next;
        true
    }
}

/// Boundary for future worker adapters. Implementations must release resources on stop,
/// avoid blocking the UI thread, and publish snapshots only while Running.
pub trait ActivityProvider {
    type Snapshot;
    type Error;
    fn state(&self) -> ProviderState;
    fn start(&mut self) -> Result<(), Self::Error>;
    fn snapshot(&self) -> Option<Self::Snapshot>;
    fn stop(&mut self);
}

/// Commands wake a provider without a timer firing while it is suspended.
enum WorkerCommand {
    Enable(bool),
    Stop,
}

#[derive(Default)]
struct WorkerStatus {
    state: Option<ProviderState>,
    error: Option<String>,
    reported: bool,
}

/// Owned by the worker thread; waiting is interruptible even for long intervals.
pub struct WorkerContext {
    rx: std::sync::mpsc::Receiver<WorkerCommand>,
    status: std::sync::Arc<std::sync::Mutex<WorkerStatus>>,
    enabled: bool,
}

impl WorkerContext {
    pub fn suspend(&mut self) {
        self.enabled = false;
        self.status.lock().unwrap_or_else(|e| e.into_inner()).state =
            Some(ProviderState::Suspended);
    }

    /// True when a poll is due. Suspension blocks on commands, consuming no polling CPU.
    pub fn wait(&mut self, interval: std::time::Duration) -> bool {
        use std::sync::mpsc::RecvTimeoutError;
        loop {
            let command = if self.enabled {
                match self.rx.recv_timeout(interval) {
                    Ok(c) => c,
                    Err(RecvTimeoutError::Timeout) => return true,
                    Err(RecvTimeoutError::Disconnected) => return false,
                }
            } else {
                match self.rx.recv() {
                    Ok(c) => c,
                    Err(_) => return false,
                }
            };
            match command {
                WorkerCommand::Stop => return false,
                WorkerCommand::Enable(enabled) => {
                    self.enabled = enabled;
                    self.status.lock().unwrap_or_else(|e| e.into_inner()).state =
                        Some(if enabled {
                            ProviderState::Running
                        } else {
                            ProviderState::Suspended
                        });
                }
            }
        }
    }
}

/// A cooperative worker. Drop requests shutdown without blocking the UI thread.
/// Thread-owned resources are released when the current bounded OS call finishes.
pub struct ManagedProvider {
    name: &'static str,
    tx: std::sync::mpsc::Sender<WorkerCommand>,
    status: std::sync::Arc<std::sync::Mutex<WorkerStatus>>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl ManagedProvider {
    pub fn spawn(
        name: &'static str,
        enabled: bool,
        run: impl FnOnce(&mut WorkerContext) -> Result<(), String> + Send + 'static,
    ) -> std::io::Result<Self> {
        let (tx, rx) = std::sync::mpsc::channel();
        let status = std::sync::Arc::new(std::sync::Mutex::new(WorkerStatus {
            state: Some(ProviderState::Starting),
            error: None,
            reported: false,
        }));
        let shared = status.clone();
        let thread = std::thread::Builder::new()
            .name(name.into())
            .spawn(move || {
                let mut context = WorkerContext {
                    rx,
                    status: shared.clone(),
                    enabled,
                };
                shared.lock().unwrap_or_else(|e| e.into_inner()).state = Some(if enabled {
                    ProviderState::Running
                } else {
                    ProviderState::Suspended
                });
                let result =
                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run(&mut context)));
                let mut status = shared.lock().unwrap_or_else(|e| e.into_inner());
                match result {
                    Ok(Ok(())) => status.state = Some(ProviderState::Stopped),
                    Ok(Err(error)) => {
                        status.state = Some(ProviderState::Failed);
                        status.error = Some(error);
                    }
                    Err(_) => {
                        status.state = Some(ProviderState::Failed);
                        status.error = Some("provider panicked".into());
                    }
                }
            })?;
        Ok(Self {
            name,
            tx,
            status,
            thread: Some(thread),
        })
    }

    pub fn state(&self) -> ProviderState {
        self.status
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .state
            .unwrap_or(ProviderState::Stopped)
    }

    pub fn error(&self) -> Option<String> {
        self.status
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .error
            .clone()
    }

    pub fn set_enabled(&self, enabled: bool) {
        let _ = self.tx.send(WorkerCommand::Enable(enabled));
    }

    pub fn request_stop(&self) {
        let _ = self.tx.send(WorkerCommand::Stop);
    }

    /// Only use off the UI thread: waits for resource cleanup to finish.
    pub fn shutdown(mut self) {
        self.request_stop();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for ManagedProvider {
    fn drop(&mut self) {
        self.request_stop();
    }
}

/// Registration owns provider lifetimes. A failed provider remains inspectable; it is not
/// blindly restarted, avoiding crash loops and repeated OS permission requests.
#[derive(Default)]
pub struct ProviderRegistry {
    providers: std::collections::BTreeMap<&'static str, ManagedProvider>,
}

impl ProviderRegistry {
    pub fn register(&mut self, provider: ManagedProvider) -> Result<(), &'static str> {
        if self.providers.contains_key(provider.name) {
            return Err("duplicate provider");
        }
        self.providers.insert(provider.name, provider);
        Ok(())
    }
    pub fn set_enabled(&self, name: &str, enabled: bool) {
        if let Some(provider) = self.providers.get(name) {
            provider.set_enabled(enabled);
        }
    }
    pub fn get(&self, name: &str) -> Option<&ManagedProvider> {
        self.providers.get(name)
    }
    pub fn pending_errors(&self) -> Vec<(&'static str, String)> {
        self.providers
            .iter()
            .filter_map(|(name, provider)| {
                let mut status = provider.status.lock().unwrap_or_else(|e| e.into_inner());
                if status.reported {
                    return None;
                }
                let error = status.error.clone()?;
                status.reported = true;
                Some((*name, error))
            })
            .collect()
    }
    pub fn stop_all(&self) {
        for provider in self.providers.values() {
            provider.request_stop();
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct RankedActivity<K> {
    pub key: K,
    pub rank: u8,
    pub changed_ms: u64,
}

/// Stable priority ordering: ranks 0 and 1 override a user selection. The caller includes
/// expired presences in `latest_change_ms` so existing selection invalidation is preserved.
pub fn order<K: Copy + Eq>(
    mut activities: Vec<RankedActivity<K>>,
    latest_change_ms: u64,
    selection: Option<(K, u64)>,
) -> Vec<K> {
    activities.sort_by_key(|a| (a.rank, Reverse(a.changed_ms)));
    if let Some((key, picked_ms)) = selection {
        if latest_change_ms <= picked_ms {
            if let Some(i) = activities.iter().position(|a| a.key == key) {
                let picked = activities.remove(i);
                activities.insert(0, picked);
            }
        }
    }
    // Sorting the records directly avoids the former quadratic rank lookups.
    activities.sort_by_key(|a| a.rank.min(2));
    activities.into_iter().map(|a| a.key).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn a(key: u8, rank: u8, changed_ms: u64) -> RankedActivity<u8> {
        RankedActivity {
            key,
            rank,
            changed_ms,
        }
    }
    #[test]
    fn lifecycle_requires_readiness_and_cleanup_after_failure() {
        let mut s = ProviderState::Stopped;
        assert!(!s.apply(ProviderEvent::Ready));
        assert!(s.apply(ProviderEvent::Start));
        assert!(s.apply(ProviderEvent::Ready));
        assert!(s.apply(ProviderEvent::Suspend));
        assert!(s.apply(ProviderEvent::Resume));
        assert!(s.apply(ProviderEvent::Fail));
        assert!(!s.apply(ProviderEvent::Start));
        assert_eq!(s, ProviderState::Failed);
        assert!(s.apply(ProviderEvent::Stop));
        assert!(s.apply(ProviderEvent::Stop));
        assert!(s.apply(ProviderEvent::Start));
    }
    #[test]
    fn urgency_overrides_selection_then_selection_returns() {
        let live = vec![a(1, 4, 10), a(2, 8, 20), a(3, 1, 30), a(4, 0, 5)];
        assert_eq!(order(live, 30, Some((2, 40))), vec![4, 3, 2, 1]);
        assert_eq!(
            order(vec![a(1, 4, 10), a(2, 8, 20)], 30, Some((2, 40))),
            vec![2, 1]
        );
    }
    #[test]
    fn new_changes_invalidate_selection_even_when_expired() {
        assert_eq!(
            order(vec![a(1, 4, 10), a(2, 8, 20)], 50, Some((2, 40))),
            vec![1, 2]
        );
    }
    #[test]
    fn ties_are_stable_and_newer_changes_lead() {
        assert_eq!(
            order(vec![a(1, 4, 10), a(2, 4, 20), a(3, 4, 20)], 20, None),
            vec![2, 3, 1]
        );
        assert!(order::<u8>(vec![], 0, Some((1, 0))).is_empty());
    }
}

#[cfg(test)]
mod regression_tests {
    use super::*;
    /// Compare against the original application scheduler across ranks, timestamps,
    /// selection changes and priority interruptions. Provider keys are unique per snapshot.
    #[test]
    fn preserves_legacy_stack_order() {
        for r1 in 0..=10 {
            for r2 in 0..=10 {
                for selection in [
                    None,
                    Some((1, 0)),
                    Some((1, 50)),
                    Some((2, 50)),
                    Some((9, 50)),
                ] {
                    for latest in [20, 60] {
                        let mut ranked = vec![(r1, Reverse(10u64), 1u8), (r2, Reverse(20), 2)];
                        ranked.sort_by_key(|(r, changed, _)| (*r, *changed));
                        let ranks: Vec<_> = ranked.iter().map(|(r, _, k)| (*k, *r)).collect();
                        let mut expected: Vec<_> = ranked.into_iter().map(|(_, _, k)| k).collect();
                        if let Some((key, picked)) = selection {
                            if latest <= picked {
                                if let Some(i) = expected.iter().position(|k| *k == key) {
                                    let key = expected.remove(i);
                                    expected.insert(0, key);
                                }
                            }
                        }
                        expected.sort_by_key(|k| {
                            ranks.iter().find(|(key, _)| key == k).unwrap().1.min(2)
                        });
                        let input = vec![
                            RankedActivity {
                                key: 1,
                                rank: r1,
                                changed_ms: 10,
                            },
                            RankedActivity {
                                key: 2,
                                rank: r2,
                                changed_ms: 20,
                            },
                        ];
                        assert_eq!(order(input, latest, selection), expected);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod provider_tests {
    use super::*;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        mpsc, Arc,
    };
    use std::time::Duration;
    const LIMIT: Duration = Duration::from_secs(5);

    #[test]
    fn suspended_provider_does_not_poll_and_resume_restarts_sampling() {
        let (ready, ready_rx) = mpsc::channel();
        let (tick, tick_rx) = mpsc::channel();
        let provider = ManagedProvider::spawn("test", false, move |context| {
            ready.send(()).unwrap();
            while context.wait(Duration::from_millis(10)) {
                context.suspend();
                tick.send(()).unwrap();
            }
            Ok(())
        })
        .unwrap();
        ready_rx.recv_timeout(LIMIT).unwrap();
        assert_eq!(provider.state(), ProviderState::Suspended);
        assert!(tick_rx.try_recv().is_err());
        provider.set_enabled(true);
        tick_rx.recv_timeout(LIMIT).unwrap();
        // A second explicit resume produces one poll, not a polling loop.
        provider.set_enabled(true);
        tick_rx.recv_timeout(LIMIT).unwrap();
        provider.shutdown();
        assert!(tick_rx.try_recv().is_err());
    }

    struct Cleanup(Arc<AtomicUsize>);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }

    #[test]
    fn failure_releases_resources_and_is_observable() {
        let cleaned = Arc::new(AtomicUsize::new(0));
        let guard = Cleanup(cleaned.clone());
        let mut provider = ManagedProvider::spawn("failure", true, move |_| {
            let _guard = guard;
            Err("device unavailable".into())
        })
        .unwrap();
        provider.thread.take().unwrap().join().unwrap();
        assert_eq!(provider.state(), ProviderState::Failed);
        assert_eq!(provider.error().as_deref(), Some("device unavailable"));
        assert_eq!(cleaned.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn panic_is_contained_and_cleanup_runs() {
        let cleaned = Arc::new(AtomicUsize::new(0));
        let guard = Cleanup(cleaned.clone());
        let mut provider = ManagedProvider::spawn("panic", true, move |_| {
            let _guard = guard;
            panic!("test provider failure");
        })
        .unwrap();
        provider.thread.take().unwrap().join().unwrap();
        assert_eq!(provider.state(), ProviderState::Failed);
        assert_eq!(cleaned.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn registry_rejects_duplicate_ids_and_stop_interrupts_long_waits() {
        let (ready, rx) = mpsc::channel();
        let worker = ManagedProvider::spawn("registered", true, move |context| {
            ready.send(()).unwrap();
            assert!(!context.wait(Duration::from_secs(3600)));
            Ok(())
        })
        .unwrap();
        let mut registry = ProviderRegistry::default();
        registry.register(worker).unwrap();
        rx.recv_timeout(LIMIT).unwrap();
        let duplicate = ManagedProvider::spawn("registered", false, |_| Ok(())).unwrap();
        assert_eq!(registry.register(duplicate), Err("duplicate provider"));
        registry.stop_all();
        registry.providers.remove("registered").unwrap().shutdown();
    }
}
