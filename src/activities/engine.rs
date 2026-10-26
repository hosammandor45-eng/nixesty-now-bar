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
