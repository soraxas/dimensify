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
