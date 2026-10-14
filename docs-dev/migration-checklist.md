# RoboSim restoration

- [x] Reproduce the missing `robot` feature and binary after Bevy migration
  `c3dc6884`; inspect the prior implementation.
- [x] Scope: optional native URDF kinematic viewer, bundled demo, joint and
  camera controls. Retired egui/editor, transport, IK, and physics integrations
  remain dormant. The historical Panda mesh assets are absent from this checkout.
- [x] Implement the feature-gated viewer and local mesh loading.
- [x] Verify loading, joint limits, hierarchy and a single Z-up conversion.
- [x] Build the native binary and check the default dependency boundary.
- [x] Document commands, supported inputs and verification limits.
- [x] Review and reproduce zero STL normals and untransformed Collada normals;
  recalculate imported normals from final geometry.
- Verification details and limits: [native reproduction](../docs/repros/robosim-2026-10-13.md).
- [x] Establish the open-source subtree boundary in `AGENTS.md` and remove
  downstream product references from maintained files.
