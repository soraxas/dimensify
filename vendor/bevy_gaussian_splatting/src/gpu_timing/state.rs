use serde::Serialize;
use std::collections::VecDeque;

pub(super) const SLOT_COUNT: usize = 4;
const HISTORY_LIMIT: usize = 120;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    Initializing,
    UnsupportedTimestampQuery,
    InvalidTimestampPeriod,
    TimestampQuery,
    DeviceFailed,
}

/// IDs are encoded as strings so browser JSON never rounds 64-bit identities.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Identity {
    #[serde(serialize_with = "id_string")]
    pub frame: u64,
    #[serde(serialize_with = "id_string")]
    pub view: u64,
    #[serde(serialize_with = "id_string")]
    pub scene: u64,
    #[serde(serialize_with = "id_string")]
    pub selection: u64,
    /// Rows dispatched to sorting, including storage padding. Not visible splats.
    pub input_rows: usize,
    pub compaction: bool,
}

fn id_string<S: serde::Serializer>(value: &u64, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(&value.to_string())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Failure {
    MapFailed,
    InvalidTimestamp,
    UnwrittenTimestamp,
    DeviceLost,
}

#[derive(Clone, Debug, Serialize)]
pub struct Measurement {
    pub duration_ms: Option<f64>,
    pub failure: Option<Failure>,
    pub begin_tick: Option<String>,
    pub end_tick: Option<String>,
    pub period_ns: Option<f32>,
}

impl Measurement {
    pub(super) fn from_ticks(ticks: [u64; 2], period_ns: f32, metal: bool) -> Self {
        let duration = ticks[1]
            .checked_sub(ticks[0])
            .map(|ticks| ticks as f64 * f64::from(period_ns) / 1_000_000.0);
        let failure = if !period_ns.is_finite()
            || period_ns <= 0.0
            || duration.is_none_or(|value| !value.is_finite())
        {
            Some(Failure::InvalidTimestamp)
        } else if metal && ticks == [0, 0] {
            Some(Failure::UnwrittenTimestamp)
        } else {
            None
        };
        Self {
            duration_ms: if failure.is_none() { duration } else { None },
            failure,
            begin_tick: Some(ticks[0].to_string()),
            end_tick: Some(ticks[1].to_string()),
            period_ns: period_ns.is_finite().then_some(period_ns),
        }
    }
    pub(super) fn failed(failure: Failure) -> Self {
        Self {
            duration_ms: None,
            failure: Some(failure),
            begin_tick: None,
            end_tick: None,
            period_ns: None,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct Sample {
    pub identity: Identity,
    pub measurement: Measurement,
}

#[derive(Clone, Debug, Serialize)]
pub struct Report {
    pub capability: Capability,
    pub scope: &'static str,
    pub slot_capacity: usize,
    pub history_capacity: usize,
    pub pending: usize,
    pub completed: u64,
    pub invalid: u64,
    pub dropped_busy: u64,
    pub evicted_history: u64,
    pub samples: VecDeque<Sample>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Ticket {
    pub slot: usize,
    serial: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Phase {
    Recording,
    Submitted,
    Ready,
    Mapping,
}

#[derive(Clone, Copy)]
struct Lease {
    ticket: Ticket,
    identity: Identity,
    phase: Phase,
}

pub(super) struct State {
    pub report: Report,
    serial: u64,
    slots: [Option<Lease>; SLOT_COUNT],
}

impl Default for State {
    fn default() -> Self {
        Self {
            serial: 0,
            slots: [None; SLOT_COUNT],
            report: Report {
                capability: Capability::Initializing,
                scope: "gaussian_radix_compute_interval_including_compaction_excluding_clears",
                slot_capacity: SLOT_COUNT,
                history_capacity: HISTORY_LIMIT,
                pending: 0,
                completed: 0,
                invalid: 0,
                dropped_busy: 0,
                evicted_history: 0,
                samples: VecDeque::with_capacity(HISTORY_LIMIT),
            },
        }
    }
}

impl State {
    pub(super) fn reserve(&mut self, identity: Identity) -> Option<Ticket> {
        if self.report.capability != Capability::TimestampQuery {
            return None;
        }
        let Some(slot) = self.slots.iter().position(Option::is_none) else {
            self.report.dropped_busy = self.report.dropped_busy.saturating_add(1);
            return None;
        };
        self.serial = self.serial.checked_add(1)?;
        let ticket = Ticket {
            slot,
            serial: self.serial,
        };
        self.slots[slot] = Some(Lease {
            ticket,
            identity,
            phase: Phase::Recording,
        });
        self.report.pending += 1;
        Some(ticket)
    }
    fn transition(&mut self, ticket: Ticket, from: Phase, to: Phase) -> bool {
        let Some(lease) = self.slots.get_mut(ticket.slot).and_then(Option::as_mut) else {
            return false;
        };
        if lease.ticket != ticket || lease.phase != from {
            return false;
        }
        lease.phase = to;
        true
    }
    pub(super) fn submitted(&mut self, ticket: Ticket) -> bool {
        self.transition(ticket, Phase::Recording, Phase::Submitted)
    }
    pub(super) fn ready(&mut self, ticket: Ticket) -> bool {
        self.transition(ticket, Phase::Submitted, Phase::Ready)
    }
    pub(super) fn mapping(&mut self, ticket: Ticket) -> bool {
        self.transition(ticket, Phase::Ready, Phase::Mapping)
    }
    pub(super) fn tickets_in(&self, phase: Phase) -> Vec<Ticket> {
        self.slots
            .iter()
            .flatten()
            .filter(|lease| lease.phase == phase)
            .map(|lease| lease.ticket)
            .collect()
    }
    pub(super) fn complete(&mut self, ticket: Ticket, measurement: Measurement) -> bool {
        let Some(lease) = self.slots.get(ticket.slot).copied().flatten() else {
            return false;
        };
        if lease.ticket != ticket || lease.phase != Phase::Mapping {
            return false;
        }
        self.slots[ticket.slot] = None;
        self.report.pending -= 1;
        self.record(lease.identity, measurement);
        true
    }
    fn record(&mut self, identity: Identity, measurement: Measurement) {
        if self.report.samples.len() == HISTORY_LIMIT {
            self.report.samples.pop_front();
            self.report.evicted_history = self.report.evicted_history.saturating_add(1);
        }
        self.report.completed = self.report.completed.saturating_add(1);
        self.report.invalid = self
            .report
            .invalid
            .saturating_add(u64::from(measurement.failure.is_some()));
        self.report.samples.push_back(Sample {
            identity,
            measurement,
        });
    }
    pub(super) fn stop(&mut self) {
        self.report.capability = Capability::DeviceFailed;
        for slot in 0..SLOT_COUNT {
            if let Some(lease) = self.slots[slot].take() {
                self.record(lease.identity, Measurement::failed(Failure::DeviceLost));
            }
        }
        self.report.pending = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity(selection: u64) -> Identity {
        Identity {
            frame: selection,
            view: 10,
            scene: 20,
            selection,
            input_rows: 32,
            compaction: false,
        }
    }
    fn enabled() -> State {
        let mut state = State::default();
        state.report.capability = Capability::TimestampQuery;
        state
    }
    fn map(state: &mut State, ticket: Ticket) {
        assert!(state.submitted(ticket));
        assert!(state.ready(ticket));
        assert!(state.mapping(ticket));
    }
    fn measured() -> Measurement {
        Measurement {
            duration_ms: Some(1.0),
            failure: None,
            begin_tick: Some("1".into()),
            end_tick: Some("2".into()),
            period_ns: Some(1_000_000.0),
        }
    }

    #[test]
    fn delayed_readbacks_hold_four_slots_until_matching_map_completion() {
        let mut state = enabled();
        let tickets: Vec<_> = (1..=4)
            .map(|n| state.reserve(identity(n)).expect("free slot"))
            .collect();
        assert!(state.reserve(identity(5)).is_none());
        assert!(!state.mapping(tickets[0]));
        assert!(!state.complete(tickets[0], measured()));
        assert_eq!(state.report.pending, 4);
        map(&mut state, tickets[0]);
        assert!(state.reserve(identity(6)).is_none());
        assert!(state.complete(tickets[0], measured()));
        let replacement = state.reserve(identity(7)).expect("released slot");
        assert_eq!(replacement.slot, tickets[0].slot);
        assert!(!state.complete(tickets[0], measured()));
        assert_eq!(state.report.pending, 4);
        assert_eq!(state.report.completed, 1);
        assert_eq!(state.report.dropped_busy, 2);
    }

    #[test]
    fn out_of_order_results_keep_their_original_scene_view_and_selection() {
        let mut state = enabled();
        let old = identity(1);
        let new = Identity {
            view: 11,
            scene: 21,
            selection: 2,
            ..identity(2)
        };
        let a = state.reserve(old).expect("old");
        let b = state.reserve(new).expect("new");
        map(&mut state, a);
        map(&mut state, b);
        assert!(state.complete(b, measured()));
        assert!(state.complete(a, measured()));
        assert_eq!(state.report.samples[0].identity, new);
        assert_eq!(state.report.samples[1].identity, old);
        let mut wrong = a;
        wrong.serial = b.serial;
        assert!(!state.complete(wrong, measured()));
    }

    #[test]
    fn long_lived_history_is_bounded_and_failed_maps_are_not_zero_timings() {
        let mut state = enabled();
        for n in 0..1_000 {
            let ticket = state.reserve(identity(n)).expect("slot reused");
            map(&mut state, ticket);
            assert!(state.complete(ticket, Measurement::failed(Failure::MapFailed)));
        }
        assert_eq!(state.report.samples.len(), 120);
        assert_eq!(state.report.evicted_history, 880);
        assert_eq!(state.report.invalid, 1_000);
        assert_eq!(state.report.pending, 0);
        assert!(
            state
                .report
                .samples
                .iter()
                .all(|s| s.measurement.duration_ms.is_none())
        );
    }

    #[test]
    fn unsupported_and_lost_devices_do_not_reuse_in_flight_queries() {
        let mut state = State::default();
        assert!(state.reserve(identity(1)).is_none());
        state.report.capability = Capability::UnsupportedTimestampQuery;
        assert!(state.reserve(identity(1)).is_none());
        assert_eq!(state.report.dropped_busy, 0);
        state.report.capability = Capability::TimestampQuery;
        let ticket = state.reserve(identity(1)).expect("supported");
        map(&mut state, ticket);
        state.stop();
        assert!(state.reserve(identity(2)).is_none());
        assert!(!state.complete(ticket, measured()));
        assert_eq!(state.report.pending, 0);
        assert_eq!(
            state.report.samples[0].measurement.failure,
            Some(Failure::DeviceLost)
        );
    }

    #[test]
    fn javascript_preserves_full_width_identity() {
        let sample = Identity {
            scene: u64::MAX,
            selection: u64::MAX - 1,
            ..identity(1)
        };
        let json = serde_json::to_value(sample).expect("identity");
        assert_eq!(json["scene"], "18446744073709551615");
        assert_eq!(json["selection"], "18446744073709551614");
    }

    #[test]
    fn durations_use_device_period_and_distinguish_unwritten_from_quantized_zero() {
        assert_eq!(
            Measurement::from_ticks([100, 2100], 500.0, false).duration_ms,
            Some(1.0)
        );
        assert_eq!(
            Measurement::from_ticks([10, 10], 1.0, true).duration_ms,
            Some(0.0)
        );
        assert_eq!(
            Measurement::from_ticks([0, 0], 1.0, false).duration_ms,
            Some(0.0)
        );
        assert_eq!(
            Measurement::from_ticks([0, 0], 1.0, true).failure,
            Some(Failure::UnwrittenTimestamp)
        );
        for (ticks, period) in [([11, 10], 1.0), ([1, 2], 0.0), ([1, 2], f32::NAN)] {
            let result = Measurement::from_ticks(ticks, period, false);
            assert_eq!(result.failure, Some(Failure::InvalidTimestamp));
            assert!(result.duration_ms.is_none());
            serde_json::to_value(result).expect("invalid measurements remain serializable");
        }
    }
}
