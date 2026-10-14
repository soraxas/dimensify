# Dimensify

Dimensify is a Rust-based library for visualising and simulating robot motion. It provides tools for modeling, simulating, and visualising robotic systems, including kinematics, motion planning, and interactions in a 3D environment.

## Features

- 3D Visualisation: Render robots and their environments in real-time using modern graphics libraries.
- Physics Simulation: Simulate realistic robot movements with collision detection and dynamics.
- Efficient and Modular: Built in Rust for performance and flexibility, allowing easy extensions for specific robotic applications.
- ROS Integration: Potential integration with ROS for hardware-in-the-loop testing.

## Run RoboSim

From this directory, with the repository Rust toolchain and `just` installed:

```sh
rtk just robosim
rtk just robosim /absolute/path/to/robot.urdf
```

The default is an embedded demo arm, so no additional assets are needed.
`Tab` selects a joint, Left/Right moves it, and `R` resets joints within their
limits. Drag with the left mouse button to orbit, scroll to zoom, `F` to frame
the robot, and Escape to quit.

For headless validation or an offscreen render:

```sh
rtk just robosim --check
rtk just robosim /absolute/path/to/robot.urdf --check
rtk just robosim --screenshot /tmp/robosim.png
```

The underlying command is `cargo run -p dimensify --features robot --bin robosim`.
The optional `robot` feature adds the standalone native kinematic viewer.
It supports fixed, revolute, continuous, prismatic and mimic joints, URDF primitives, and local
STL/OBJ/Collada meshes. Mesh paths are relative to the URDF; `package://` paths
are searched under its ancestor directories. Materials use colors; texture
maps are not displayed. Unsupported joint types and missing meshes fail before
a window opens.

This is a kinematic viewer, without dynamics or collision simulation. The old
egui editor, IK and UDP/Python integrations are not restored; the old
`transport_udp,robot` recipe no longer applies.
