//! Bounded enumeration for Darwin's capacity-limited PROC_PIDLISTFDS.
//!
//! A successful, full buffer is ambiguous: the kernel may have truncated it.
//! The earlier pbi_nfiles sample is only a sizing hint, never a completeness
//! proof. This helper is deliberately independent of socket interpretation.

const MIN_CAPACITY: usize = 64;
const MAX_CAPACITY: usize = 65_536;
const MAX_ATTEMPTS: usize = 8;

pub(crate) fn read_complete_list<T, E>(
    declared_count: u32,
    mut read: impl FnMut(usize) -> Result<Vec<T>, E>,
) -> Option<Vec<T>> {
    let hint = usize::try_from(declared_count).ok()?;
    if hint >= MAX_CAPACITY {
        return None;
    }
    let mut capacity = hint
        .saturating_add(MIN_CAPACITY)
        .clamp(MIN_CAPACITY, MAX_CAPACITY);
    for _ in 0..MAX_ATTEMPTS {
        let entries = read(capacity).ok()?;
        if entries.len() < capacity {
            return Some(entries);
        }
        if entries.len() > capacity || capacity == MAX_CAPACITY {
            return None;
        }
        capacity = capacity.saturating_mul(2).min(MAX_CAPACITY);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stale_count_cannot_hide_a_late_socket_in_a_truncated_list() {
        let descriptors: Vec<_> = (0..130).collect();
        let mut capacities = Vec::new();
        let result = read_complete_list(1, |capacity| {
            capacities.push(capacity);
            Ok::<_, ()>(descriptors.iter().copied().take(capacity).collect())
        })
        .expect("eventually unsaturated");
        assert_eq!(capacities, [65, 130, 260]);
        assert_eq!(result.last(), Some(&129));
        assert_eq!(result.len(), 130);
    }

    #[test]
    fn zero_hint_still_allocates_a_real_buffer_and_empty_is_valid() {
        let result = read_complete_list(0, |capacity| {
            assert!(capacity > 0);
            Ok::<_, ()>(Vec::<u8>::new())
        });
        assert_eq!(result, Some(Vec::new()));
    }

    #[test]
    fn an_exact_fit_is_retried_even_without_actual_truncation() {
        let mut calls = 0;
        let result = read_complete_list(0, |_| {
            calls += 1;
            Ok::<_, ()>(vec![0; MIN_CAPACITY])
        });
        assert_eq!(calls, 2);
        assert_eq!(result.unwrap().len(), MIN_CAPACITY);
    }

    #[test]
    fn persistent_growth_exhausts_a_bounded_budget_without_claiming_completeness() {
        let mut calls = 0;
        assert!(
            read_complete_list(0, |capacity| {
                calls += 1;
                Ok::<_, ()>(vec![0; capacity])
            })
            .is_none()
        );
        assert_eq!(calls, MAX_ATTEMPTS);
    }

    #[test]
    fn errors_oversized_replies_and_capacity_exhaustion_are_incomplete() {
        assert!(read_complete_list::<u8, _>(0, |_| Err(())).is_none());
        assert!(read_complete_list(0, |capacity| Ok::<_, ()>(vec![0; capacity + 1])).is_none());
        assert!(
            read_complete_list(65_536, |_| -> Result<Vec<u8>, ()> {
                panic!("over-budget hints must not allocate")
            })
            .is_none()
        );
        let mut calls = 0;
        assert!(
            read_complete_list(65_535, |capacity| {
                calls += 1;
                assert_eq!(capacity, MAX_CAPACITY);
                Ok::<_, ()>(vec![0; capacity])
            })
            .is_none()
        );
        assert_eq!(calls, 1);
    }
}
