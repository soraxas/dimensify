use bevy::{
    app::{App, Update},
    color::Color,
    prelude::{Gizmos, NonSendMut, Res},
};
use rapier3d::prelude::{ColliderSet, NarrowPhase};

use crate::{dimensify::DimensifyStateFlags, harness::Harness, DimensifyState};

pub fn plugin(app: &mut App) {
    app.add_systems(Update, draw_contact);
}

fn draw_contact(gizmos: Gizmos, mut state: Res<DimensifyState>, mut harness: NonSendMut<Harness>) {
    if state.flags.contains(DimensifyStateFlags::CONTACT_POINTS) {
        draw_contacts(
            &harness.physics.narrow_phase,
            &harness.physics.colliders,
            gizmos,
        );
    }
}

fn draw_contacts(nf: &NarrowPhase, colliders: &ColliderSet, mut gizmos: Gizmos) {
    use rapier3d::math::Isometry;

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
                    Color::srgb(0.0, 0.0, 1.0)
                } else {
                    Color::srgb(1.0, 0.0, 0.0)
                };
                let pos1 = colliders[pair.collider1].position();
                let pos2 = colliders[pair.collider2].position();
                let start =
                    pos1 * manifold.subshape_pos1.unwrap_or(Isometry::identity()) * pt.local_p1;
                let end =
                    pos2 * manifold.subshape_pos2.unwrap_or(Isometry::identity()) * pt.local_p2;
                let n = pos1
                    * manifold.subshape_pos1.unwrap_or(Isometry::identity())
                    * manifold.local_n1;

                // window.draw_graphics_line(&start, &(start + n * 0.4), &point![0.5, 1.0, 0.5]);
                // window.draw_graphics_line(&start, &end, &color);

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
