use bevy::{
    app::{App, Startup, Update},
    color::Color,
    core::Name,
    prelude::{warn, Commands, Component, Gizmos, IntoSystemConfigs, Query, Res},
};
use bevy_egui::egui::Ui;
use bevy_trait_query::RegisterExt;

use crate::{harness::Harness, ui::main_ui::MainUiPainter};

#[derive(Component)]
struct DrawContactData {
    pub enabled: bool,
}

impl MainUiPainter for DrawContactData {
    fn draw(&mut self, ui: &mut Ui) {
        ui.checkbox(&mut self.enabled, "draw contacts");
    }
}

pub fn plugin(app: &mut App) {
    app.add_systems(
        Update,
        draw_contacts.run_if(|data: Query<&DrawContactData>| data.single().enabled),
    )
    .register_component_as::<dyn MainUiPainter, DrawContactData>()
    .add_systems(Startup, |mut commands: Commands| {
        // insert the settings component
        commands.spawn((
            Name::new("MainUI:DrawContact"),
            DrawContactData { enabled: false },
        ));
    });
}

fn draw_contacts(mut gizmos: Gizmos, harness: Res<Harness>) {
    let nf = &harness.physics.narrow_phase;
    let colliders = &harness.physics.colliders;

    use rapier3d::math::Isometry;
    macro_rules! skip_empty {
        ($colliders:expr, $accessor:expr) => {
            match $colliders.get($accessor) {
                Some(val) => val,
                None => {
                    warn!("Failed to obtain collider: {:?}; skipped.", $accessor);
                    continue;
                }
            }
        };
    }

    for pair in nf.contact_pairs() {
        for manifold in &pair.manifolds {
            /*
            for contact in &manifold.data.solver_contacts {
                let p = contact.point;
                let n = manifold.data.normal;

                use crate::engine::GraphicsWindow;
                window.draw_graphics_line(&p, &(p + n * 0.4), &point![0.5, 1.0, 0.5]);
            }
            */
            for pt in manifold.contacts() {
                let color = if pt.dist > 0.0 {
                    Color::BLUE
                } else {
                    Color::RED
                };
                let pos1 = skip_empty!(colliders, pair.collider1).position();
                let pos2 = skip_empty!(colliders, pair.collider2).position();
                let start =
                    pos1 * manifold.subshape_pos1.unwrap_or(Isometry::identity()) * pt.local_p1;
                let end =
                    pos2 * manifold.subshape_pos2.unwrap_or(Isometry::identity()) * pt.local_p2;
                let n = pos1
                    * manifold.subshape_pos1.unwrap_or(Isometry::identity())
                    * manifold.local_n1;

                gizmos.line(
                    start.into(),
                    (start + n * 0.4).into(),
                    Color::srgb(0.5, 1.0, 0.5),
                );
                gizmos.line(start.into(), end.into(), color);
            }
        }
    }
}
