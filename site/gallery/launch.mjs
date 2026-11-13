// URL construction only. The viewer validates all startup options before loading.
export function viewerLink(base, options) {
  const orientation = typeof options.orientation === "string"
    ? options.orientation.trim() : options.orientation;
  const link = new URL(base);
  link.search = "";
  for (const key of [
    "url",
    "format",
    "up",
    "orientation",
    "eye",
    "target",
    "fov",
    "mode",
    "frame",
    "transparency",
    "embed",
  ]) {
    if (key === "up" && orientation && orientation !== "custom")
      continue;
    if (key === "orientation" && orientation === "custom") continue;
    const value = options[key];
    if (value === undefined || value === false || String(value).trim() === "")
      continue;
    link.searchParams.set(key, value === true ? "1" : String(value).trim());
  }
  return link.href;
}

export function scenePresets(gallery) {
  const viewer = new URL("../splat-viewer/", gallery);
  const url = new URL("../scenes/rings.ply", gallery).href;
  return [
    {
      title: "Gaussian rings",
      description:
        "A generated scene of three colored rings. Orbit to inspect anisotropic splats and depth blending.",
      transparency: "sorted",
      up: "y",
    },
    {
      title: "Stochastic coverage",
      description:
        "The same scene and camera with experimental stochastic coverage. Compare its noise and transparency with sorted rendering.",
      transparency: "stochastic",
      up: "y",
    },
    {
      title: "Source axis conversion",
      description:
        "The same source interpreted as Z-up. See how source orientation changes while the authored camera stays in viewer space.",
      transparency: "sorted",
      up: "z",
    },
  ].map((preset) => ({
    ...preset,
    href: viewerLink(viewer, {
      url,
      eye: "4,3,6",
      target: "0,0,0",
      mode: "orbit",
      ...preset,
    }),
  }));
}

/** Robot viewer link; omits `urdf` to open the bundled demo arm. */
export function robotLink(base, { urdf, packages = [] } = {}) {
  const link = new URL(base);
  link.search = "";
  if (urdf && String(urdf).trim()) link.searchParams.set("urdf", String(urdf).trim());
  for (const spec of packages) link.searchParams.append("package", spec);
  return link.href;
}

// Pinned dataset revision, so preset links keep loading the same robots.
const FRANKA =
  "https://raw.githubusercontent.com/Daniella1/urdf_files_dataset/81f4cdac42c3a51ba88833180db5bf3697988c87/urdf_files/oems/xacro_generated/franka_emika/franka_description/robots";

export function robotPresets(gallery) {
  const viewer = new URL("../examples/dimensify/robosim/", gallery);
  return [
    {
      title: "Demo arm",
      description:
        "The bundled two-joint arm. Loads instantly with no network access.",
      source: "Bundled",
    },
    {
      title: "Franka Emika Panda",
      description:
        "A 7-DOF arm with mimic gripper fingers and Collada meshes resolved from package:// URIs.",
      source: "Apache-2.0, urdf_files_dataset",
      urdf: `${FRANKA}/panda/panda.urdf`,
    },
    {
      title: "Franka Research 3",
      description: "The FR3 arm and hand, sharing the Franka mesh package.",
      source: "Apache-2.0, urdf_files_dataset",
      urdf: `${FRANKA}/fr3/fr3.urdf`,
    },
    {
      title: "Dual Panda workcell",
      description:
        "Two Panda arms on a table: 16 controllable joints and 119 visual meshes from 10 unique files.",
      source: "Apache-2.0, urdf_files_dataset",
      urdf: `${FRANKA}/dual_panda/dual_panda.urdf`,
    },
  ].map((preset) => ({ ...preset, href: robotLink(viewer, preset) }));
}


/** Point cloud startup supports only a remote source and Y/Z up axis. */
export function pointcloudLink(base, { source, up } = {}) {
  const link = new URL(base);
  link.search = "";
  if (source && String(source).trim()) {
    // Startup recognizes lowercase schemes; preserve the rest, including signatures.
    const normalized = String(source).trim().replace(/^https?:\/\//i, scheme => scheme.toLowerCase());
    link.searchParams.set("source", normalized);
  }
  if (up === "y" || up === "z") link.searchParams.set("up", up);
  return link.href;
}
