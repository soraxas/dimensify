//! Bounded retries for slow device fences; no rendering is performed while waiting.
use std::time::Duration;

#[derive(Debug, PartialEq, Eq)]
pub enum WaitError<E> {
    Timeout,
    Other(E),
}

pub fn wait<E>(
    budget: Duration,
    mut elapsed: impl FnMut() -> Duration,
    mut poll: impl FnMut(Duration) -> Result<(), WaitError<E>>,
    mut progress: impl FnMut(Duration),
) -> Result<Duration, WaitError<E>> {
    let mut next_progress = Duration::from_secs(10);
    loop {
        let spent = elapsed();
        let Some(remaining) = budget.checked_sub(spent).filter(|left| !left.is_zero()) else {
            return Err(WaitError::Timeout);
        };
        match poll(remaining.min(Duration::from_secs(1))) {
            Ok(()) => return Ok(elapsed()),
            Err(WaitError::Timeout) => {
                let spent = elapsed();
                if spent >= next_progress {
                    progress(spent);
                    next_progress = spent.saturating_add(Duration::from_secs(10));
                }
            }
            Err(other) => return Err(other),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn timeout_retries_until_actual_completion_without_another_frame() {
        let clock = Cell::new(Duration::ZERO);
        let calls = Cell::new(0);
        let result = wait::<&str>(
            Duration::from_secs(3),
            || clock.get(),
            |slice| {
                assert_eq!(slice, Duration::from_secs(1));
                clock.set(clock.get() + slice);
                calls.set(calls.get() + 1);
                if calls.get() == 3 {
                    Ok(())
                } else {
                    Err(WaitError::Timeout)
                }
            },
            |_| {},
        );
        assert_eq!(result, Ok(Duration::from_secs(3)));
        assert_eq!(calls.get(), 3);
    }

    #[test]
    fn deadline_limits_retries_and_shortens_the_final_wait() {
        let clock = Cell::new(Duration::ZERO);
        let calls = Cell::new(0);
        let result = wait::<&str>(
            Duration::from_millis(1500),
            || clock.get(),
            |slice| {
                calls.set(calls.get() + 1);
                assert_eq!(
                    slice,
                    if calls.get() == 1 {
                        Duration::from_secs(1)
                    } else {
                        Duration::from_millis(500)
                    }
                );
                clock.set(clock.get() + slice);
                Err(WaitError::Timeout)
            },
            |_| {},
        );
        assert_eq!(result, Err(WaitError::Timeout));
        assert_eq!(calls.get(), 2);
        assert_eq!(clock.get(), Duration::from_millis(1500));
    }

    #[test]
    fn non_timeout_device_errors_are_never_retried() {
        let calls = Cell::new(0);
        let result = wait(
            Duration::from_secs(90),
            || Duration::ZERO,
            |_| {
                calls.set(calls.get() + 1);
                Err(WaitError::Other("device lost"))
            },
            |_| {},
        );
        assert_eq!(result, Err(WaitError::Other("device lost")));
        assert_eq!(calls.get(), 1);
    }

    #[test]
    fn expired_phase_does_not_submit_a_wait() {
        let result = wait::<&str>(
            Duration::ZERO,
            || Duration::ZERO,
            |_| panic!("expired deadline must not poll"),
            |_| {},
        );
        assert_eq!(result, Err(WaitError::Timeout));
    }
}
