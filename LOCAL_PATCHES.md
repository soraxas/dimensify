# Local Dimensify patches

Upstream provenance is recorded in [`.gitrepo`](.gitrepo).

## Core viewer plugins and standalone RoboSim

The Bevy 0.19 migration reduced the default `dimensify` and `dimensify_ui`
interfaces to camera plugins and shared markers. The old robot/editor source
remains dormant and is not compiled into the default library.

`dimensify/robot` explicitly registers the `robosim` binary and its independent
`robosim` module. It provides local URDF parsing, visual geometry, hierarchy-based
forward kinematics, native controls and a bundled demo. It does not activate
Rapier, egui, Lightyear or the former editor. The single conversion from canonical
right-handed Z-up data to Bevy's Y-up frame is on the robot root.

[Run commands and supported inputs](README.md#run-robosim) are maintained in the
vendor README. [Restoration checklist](docs-dev/migration-checklist.md) records
verification and remaining limits.
