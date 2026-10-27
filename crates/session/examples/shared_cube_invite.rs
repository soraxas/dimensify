//! Shared cube with a short, role-specific invite code instead of room/token arguments.
#[path = "shared_cube/mod.rs"]
mod demo;

fn main() {
    demo::run(true);
}
