//! Frame-time detail control. Steps the screen-space LOD target through fixed
//! levels so the viewer holds its frame budget without manual tuning.
//!
//! Detail never goes finer than the starting level: that footprint is the
//! chosen quality, and the controller only trades it away to hold the budget.
//!
//! Frames are sampled only while the camera is still and every stream has
//! settled: motion may switch transparency to stochastic, and pending
//! refinement changes the workload being measured. Windows count frames that
//! miss a refresh rather than a percentile, because browser frame pacing
//! routinely drops isolated frames. Quality drops after two consecutive slow
//! windows and rises only after sustained headroom, one level at a time.
//! A level that was too slow becomes a ceiling whose back-off doubles each
//! time a probe fails, so a marginal device does not oscillate.
use crate::streaming::{SogLodPolicy, SogLodSettings, runtime::SogStream};
use bevy::prelude::*;
use bevy_gaussian_splatting::GaussianCamera;

/// Screen-space targets in physical pixels, finest first.
pub const LEVELS: [f32; 8] = [1.5, 2., 2.5, 3., 4., 5., 6., 8.];
/// Calm frames per decision.
const WINDOW: usize = 60;
/// Calm time discarded after motion or a level change.
const SETTLE_SECS: f64 = 0.5;
/// A frame longer than `budget * MISS` missed at least one refresh.
const MISS: f32 = 1.5;
/// More than this fraction of missed frames makes a window slow.
const SLOW_MISSES: f32 = 0.25;
/// At most this fraction of missed frames counts as holding the budget. Vsync
/// caps browser frames at the refresh interval, so spare headroom is invisible.
const HOLD_MISSES: f32 = 0.05;
/// Consecutive slow windows before stepping coarser.
const SLOW_WINDOWS: u32 = 2;
/// Sustained holding time before probing one level finer.
const PROBE_SECS: f64 = 4.0;
const CEILING_SECS: f64 = 30.;
const MAX_CEILING_SECS: f64 = 300.;
/// A coarser step must cut missed frames by this fraction to justify the next
/// one, so a frame limited by something other than splats does not drive
/// detail to the coarsest level.
const MIN_IMPROVEMENT: f32 = 0.2;

#[derive(Resource, Clone, Debug)]
pub struct AutoDetail {
    pub enabled: bool,
    /// Frame interval to hold, in seconds.
    pub frame_budget: f32,
    pub level: usize,
    /// Finest level the controller may return to: the starting footprint.
    finest: usize,
    samples: Vec<f32>,
    calm_since: Option<f64>,
    holding_since: Option<f64>,
    /// A finer level was probed and has not yet held the budget for
    /// `PROBE_SECS`. Not a deadline:
    /// streaming the finer detail can take arbitrarily long on a slow network.
    probing: bool,
    /// Finest level allowed until the given time.
    ceiling: Option<(usize, f64)>,
    backoff: f64,
    slow_windows: u32,
    /// Missed fraction that caused the last coarser step; cleared once the
    /// budget holds again.
    slow_misses: Option<f32>,
    camera: Option<GlobalTransform>,
}

impl Default for AutoDetail {
    fn default() -> Self {
        Self::starting_at(level_for(SogLodSettings::default().target_pixels))
    }
}

/// Nearest level at or coarser than `target_pixels`.
pub fn level_for(target_pixels: f32) -> usize {
    LEVELS
        .iter()
        .position(|&level| level >= target_pixels)
        .unwrap_or(LEVELS.len() - 1)
}

impl AutoDetail {
    pub fn starting_at(level: usize) -> Self {
        let level = level.min(LEVELS.len() - 1);
        Self {
            enabled: true,
            frame_budget: 1. / 60.,
            level,
            finest: level,
            samples: Vec::with_capacity(WINDOW),
            calm_since: None,
            holding_since: None,
            probing: false,
            ceiling: None,
            backoff: CEILING_SECS,
            slow_windows: 0,
            slow_misses: None,
            camera: None,
        }
    }

    pub fn target_pixels(&self) -> f32 {
        LEVELS[self.level]
    }

    /// Record one frame; returns the new level when it changes.
    pub fn observe(&mut self, now: f64, frame: f32, calm: bool) -> Option<usize> {
        if !calm || !frame.is_finite() {
            self.samples.clear();
            self.calm_since = None;
            self.holding_since = None;
            self.slow_windows = 0;
            return None;
        }
        let since = *self.calm_since.get_or_insert(now);
        if now - since < SETTLE_SECS {
            return None;
        }
        self.samples.push(frame);
        if self.samples.len() < WINDOW {
            return None;
        }
        let limit = self.frame_budget * MISS;
        let misses = self.samples.iter().filter(|&&f| f > limit).count() as f32 / WINDOW as f32;
        self.samples.clear();
        if misses > SLOW_MISSES {
            self.holding_since = None;
            self.slow_windows += 1;
            let improving = self
                .slow_misses
                .is_none_or(|last| misses < last * (1. - MIN_IMPROVEMENT));
            if self.slow_windows < SLOW_WINDOWS || self.level + 1 >= LEVELS.len() || !improving {
                return None;
            }
            let hold = if self.probing {
                let hold = self.backoff;
                self.backoff = (self.backoff * 2.).min(MAX_CEILING_SECS);
                hold
            } else {
                self.backoff = CEILING_SECS;
                CEILING_SECS
            };
            self.level += 1;
            self.ceiling = Some((self.level, now + hold));
            self.probing = false;
            self.slow_windows = 0;
            self.slow_misses = Some(misses);
            self.calm_since = Some(now);
            return Some(self.level);
        }
        self.slow_windows = 0;
        if misses > HOLD_MISSES {
            self.holding_since = None;
            return None;
        }
        self.slow_misses = None;
        let holding = *self.holding_since.get_or_insert(now);
        if now - holding >= PROBE_SECS {
            // The probed level has held as long as it took to earn the probe.
            self.probing = false;
        }
        let allowed = match self.ceiling {
            Some((finest, until)) if now < until => finest.max(self.finest),
            _ => self.finest,
        };
        if now - holding < PROBE_SECS || self.level <= allowed {
            return None;
        }
        self.level -= 1;
        self.probing = true;
        self.holding_since = None;
        self.calm_since = Some(now);
        Some(self.level)
    }
}

/// Feed frame intervals to the controller and apply its level.
pub(super) fn adjust(
    time: Res<Time<Real>>,
    mut auto: ResMut<AutoDetail>,
    mut lod: ResMut<SogLodSettings>,
    streams: Query<&SogStream>,
    cameras: Query<&GlobalTransform, With<GaussianCamera>>,
) {
    if !auto.enabled || lod.lod_policy != SogLodPolicy::ScreenSpace {
        return;
    }
    let camera = cameras.iter().next().copied();
    let still = camera.is_some() && camera == auto.camera;
    auto.camera = camera;
    let mut any = false;
    let settled = streams.iter().all(|stream| {
        any = true;
        let status = stream.status();
        status.evaluated
            && !status.pending_work
            && !status.pending_generation
            && status.pending_downloads == 0
    });
    let now = time.elapsed_secs_f64();
    auto.observe(now, time.delta_secs(), still && any && settled);
    let target = auto.target_pixels();
    if lod.target_pixels != target {
        lod.target_pixels = target;
    }
}

/// Starting detail for this device. Touch-first screens start at the same
/// CSS-pixel footprint as a desktop at DPR 1, with a smaller splat ceiling;
/// the controller then refines or coarsens from there.
pub fn device_defaults() -> (SogLodSettings, AutoDetail) {
    let mut lod = SogLodSettings::default();
    if let Some(ratio) = touch_device_pixel_ratio() {
        lod.target_pixels = (lod.target_pixels * ratio as f32).clamp(LEVELS[0], LEVELS[6]);
        lod.splat_budget = lod.splat_budget.min(2_000_000);
    }
    let auto = AutoDetail::starting_at(level_for(lod.target_pixels));
    lod.target_pixels = auto.target_pixels();
    (lod, auto)
}

/// Device pixel ratio when the primary pointer is coarse (a phone or tablet).
#[cfg(target_arch = "wasm32")]
fn touch_device_pixel_ratio() -> Option<f64> {
    let window = web_sys::window()?;
    let coarse = window
        .match_media("(pointer: coarse)")
        .ok()
        .flatten()
        .is_some_and(|query| query.matches());
    coarse.then(|| window.device_pixel_ratio().max(1.))
}

#[cfg(not(target_arch = "wasm32"))]
fn touch_device_pixel_ratio() -> Option<f64> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    const FRAME: f64 = 1. / 60.;

    /// A controller allowed to probe up to the finest level.
    fn uncapped(level: usize) -> AutoDetail {
        AutoDetail {
            finest: 0,
            ..AutoDetail::starting_at(level)
        }
    }

    /// Run `seconds` of calm frames of `frame` length from `start`.
    fn run(auto: &mut AutoDetail, start: f64, seconds: f64, frame: f32) -> (f64, Vec<usize>) {
        let mut changes = Vec::new();
        let mut now = start;
        while now < start + seconds {
            now += FRAME;
            if let Some(level) = auto.observe(now, frame, true) {
                changes.push(level);
            }
        }
        (now, changes)
    }

    #[test]
    fn sustained_slow_frames_coarsen_one_level() {
        let mut auto = AutoDetail::starting_at(1);
        let (_, changes) = run(&mut auto, 0., 3., 0.05);
        assert_eq!(changes, vec![2]);
        assert_eq!(auto.target_pixels(), 2.5);
    }

    #[test]
    fn coarsening_stops_when_it_does_not_help() {
        let mut auto = AutoDetail::starting_at(1);
        // Constant slow frames: the first step is taken, the next shows no
        // improvement, so detail holds instead of running to the coarsest.
        let (_, changes) = run(&mut auto, 0., 10., 0.05);
        assert_eq!(changes, vec![2]);
    }

    #[test]
    fn sustained_headroom_probes_finer_and_a_failed_probe_backs_off() {
        let mut auto = uncapped(3);
        let (now, changes) = run(&mut auto, 0., 7., FRAME as f32);
        assert_eq!(changes, vec![2], "one probe after the hold time");
        // The probe is too slow: step back and set a ceiling.
        let (now, changes) = run(&mut auto, now, 3., 0.03);
        assert_eq!(changes, vec![3]);
        // Holding again does not re-probe while the ceiling lasts.
        let (now, changes) = run(&mut auto, now, 20., FRAME as f32);
        assert!(changes.is_empty(), "{changes:?}");
        // After it expires, it probes again; a second failure holds twice as long.
        let (now, changes) = run(&mut auto, now, 12., FRAME as f32);
        assert_eq!(changes, vec![2]);
        let (now, changes) = run(&mut auto, now, 3., 0.03);
        assert_eq!(changes, vec![3]);
        let (_, until) = auto.ceiling.unwrap();
        assert!(
            (until - now - 2. * CEILING_SECS).abs() < 2.,
            "{until} {now}"
        );
    }

    #[test]
    fn a_probe_fails_even_when_its_detail_streams_slowly() {
        let mut auto = uncapped(3);
        let (now, changes) = run(&mut auto, 0., 7., FRAME as f32);
        assert_eq!(changes, vec![2]);
        // The finer level takes 15 s to stream; nothing is sampled meanwhile.
        let mut now = now;
        for _ in 0..(15. / FRAME) as usize {
            now += FRAME;
            assert_eq!(auto.observe(now, FRAME as f32, false), None);
        }
        let (now, changes) = run(&mut auto, now, 3., 0.03);
        assert_eq!(changes, vec![3], "the probe failed");
        assert_eq!(
            auto.backoff,
            2. * CEILING_SECS,
            "a failed probe doubles the next hold"
        );
        let (_, until) = auto.ceiling.unwrap();
        assert!((until - now - CEILING_SECS).abs() < 2., "{until} {now}");
    }

    #[test]
    fn headroom_never_refines_past_the_starting_level() {
        let mut auto = AutoDetail::starting_at(3);
        let (now, changes) = run(&mut auto, 0., 3., 0.05);
        assert_eq!(changes, vec![4]);
        // After the ceiling expires, sustained headroom returns to the start only.
        let (_, changes) = run(&mut auto, now, 60., FRAME as f32);
        assert_eq!(changes, vec![3]);
        assert_eq!(auto.level, 3);
    }

    #[test]
    fn isolated_dropped_frames_do_not_coarsen() {
        // Browser pacing: one frame in eight takes two refresh intervals.
        let mut auto = AutoDetail::starting_at(1);
        let mut now = 0.;
        for frame in 0..1200 {
            now += FRAME;
            let interval = if frame % 8 == 0 { 2. * FRAME } else { FRAME };
            assert_eq!(auto.observe(now, interval as f32, true), None);
        }
    }

    #[test]
    fn motion_and_refinement_discard_samples() {
        let mut auto = AutoDetail::starting_at(1);
        let mut now = 0.;
        for frame in 0..600 {
            now += FRAME;
            // Interrupt the window before it can fill.
            let calm = frame % 20 != 0;
            assert_eq!(auto.observe(now, 0.05, calm), None);
        }
    }

    #[test]
    fn levels_round_toward_coarser() {
        assert_eq!(LEVELS[level_for(2.)], 2.);
        assert_eq!(LEVELS[level_for(2.2)], 2.5);
        assert_eq!(LEVELS[level_for(100.)], 8.);
        assert_eq!(LEVELS[level_for(0.1)], 1.5);
    }
}
