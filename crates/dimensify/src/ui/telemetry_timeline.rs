use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};

use crate::telemetry::{
    TelemetryEcsSync, TelemetryPlayback, TelemetryPlaybackMode, TelemetryRecordingState,
    TelemetryStore,
};

pub fn telemetry_timeline_ui(
    mut contexts: EguiContexts,
    mut playback: ResMut<TelemetryPlayback>,
    store: Res<TelemetryStore>,
    mut recording: ResMut<TelemetryRecordingState>,
    mut ecs_sync: ResMut<TelemetryEcsSync>,
) {
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };

    let timeline_bounds = store.timeline_bounds(&playback.timeline);

    let rrd_supported = cfg!(feature = "telemetry_rrd");
    let mut show_timeline = recording.enabled;

    egui::TopBottomPanel::bottom("telemetry_timeline_panel")
        .resizable(false)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                let mut enabled = recording.enabled;
                let record_checkbox = ui.add_enabled(
                    rrd_supported,
                    egui::Checkbox::new(&mut enabled, "Record .rrd"),
                );
                if record_checkbox.changed() {
                    recording.enabled = enabled;
                    recording.error = None;
                }
                if !rrd_supported {
                    ui.label("enable `telemetry_rrd` to record");
                }

                ui.label("Timeline");
                ui.text_edit_singleline(&mut playback.timeline);

                let mut live = playback.mode == TelemetryPlaybackMode::Live;
                if ui.checkbox(&mut live, "Live").changed() {
                    playback.mode = if live {
                        TelemetryPlaybackMode::Live
                    } else {
                        TelemetryPlaybackMode::Fixed
                    };
                }

                if let Some(err) = recording.error.as_ref() {
                    ui.colored_label(egui::Color32::RED, err);
                }

                ui.add_space(8.0);
                ui.checkbox(&mut ecs_sync.enabled, "Apply to ECS");
            });

            if recording.enabled {
                show_timeline = true;
            }

            if show_timeline {
                ui.add_space(4.0);
                match timeline_bounds {
                    Some((min, max)) if max >= min => {
                        let mut time = playback.time;
                        let slider = egui::Slider::new(&mut time, min..=max)
                            .text("time")
                            .clamping(egui::SliderClamping::Always);
                        let response = ui.add(slider);
                        if response.changed() {
                            playback.time = time;
                            playback.mode = TelemetryPlaybackMode::Fixed;
                        }
                        let rect = response.rect;
                        if rect.is_positive() {
                            let min_f = min as f32;
                            let max_f = max as f32;
                            let live_x =
                                egui::emath::remap_clamp(max_f, min_f..=max_f, rect.x_range());
                            let scrub_x = egui::emath::remap_clamp(
                                time as f32,
                                min_f..=max_f,
                                rect.x_range(),
                            );
                            let painter = ui.painter();
                            let live_color = egui::Color32::from_rgb(255, 200, 80);
                            let scrub_color = egui::Color32::from_rgb(120, 200, 255);
                            painter.line_segment(
                                [
                                    egui::pos2(live_x, rect.top()),
                                    egui::pos2(live_x, rect.bottom()),
                                ],
                                egui::Stroke::new(2.0, live_color),
                            );
                            painter.circle_filled(
                                egui::pos2(scrub_x, rect.center().y),
                                3.5,
                                scrub_color,
                            );
                        }
                        ui.horizontal(|ui| {
                            ui.label(format!("range: {:.3}s .. {:.3}s", min, max));
                            if ui.button("Reset to live").clicked() {
                                playback.time = max;
                                playback.mode = TelemetryPlaybackMode::Live;
                            }
                        });
                    }
                    _ => {
                        ui.label("No telemetry on this timeline yet.");
                    }
                }
            }
        });
}
