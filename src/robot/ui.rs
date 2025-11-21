use bevy::prelude::*;
use bevy_egui::egui::{self, Slider};
use egui::DragValue;
use rand::{RngCore, rngs::SmallRng};
use std::ops::RangeInclusive;

pub trait SmallRngSampleRange {
    fn sample(&mut self, range: &RangeInclusive<f32>) -> f32;
}

impl SmallRngSampleRange for SmallRng {
    fn sample(&mut self, range: &RangeInclusive<f32>) -> f32 {
        let next_u32 = self.next_u32() as f32 / u32::MAX as f32;

        range.start() + next_u32 * (*range.end() - *range.start())
    }
}

pub fn ui_for_joint(
    ui: &mut egui::Ui,
    node: &k::node::Node<f32>,
    rng: Option<&mut SmallRng>,
) -> Option<f32> {
    let joint_info = node
        .mimic_parent()
        .map(|parent| format!("(mimic: {})", parent.joint().name));
    let joint = node.joint();

    if let Some(cur_joint_position) = joint.joint_position() {
        let mut joint_position = cur_joint_position;

        ui.horizontal(|ui| {
            ui.label(joint.name.clone());

            if let Some(limit) = joint.limits {
                let range = RangeInclusive::new(limit.min, limit.max);

                if let Some(rng) = rng {
                    joint_position = rng.sample(&range);
                }

                ui.add(Slider::new(&mut joint_position, range));
            } else {
                // no joint limits
                if let Some(rng) = rng {
                    const DEFAULT_RANGE: RangeInclusive<f32> = RangeInclusive::new(-1000., 1000.);
                    warn!(
                        "No joint limits for {}. Implicitly setting a limit of {} to {}",
                        joint.name,
                        DEFAULT_RANGE.start(),
                        DEFAULT_RANGE.end()
                    );

                    joint_position = rng.sample(&DEFAULT_RANGE);
                }

                ui.add(DragValue::new(&mut joint_position).speed(0.1));
            }
            if let Some(joint_info) = joint_info {
                ui.label(joint_info);
            }
        });

        if joint_position != cur_joint_position {
            return Some(joint_position);
        }
    } else {
        ui.weak(format!("{} (fixed)", joint.name,));
    }
    None
}
