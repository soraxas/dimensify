use dimensify::Dimensify;

pub fn init_world(viewer: &mut Dimensify) {
    crate::dynamic_trimesh3::do_init_world(viewer, true);
}
