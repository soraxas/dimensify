import { pointcloudLink, viewerLink } from "./launch.mjs";

const convention = document.querySelector('[name="orientation"]');
const viewer = document.querySelector('[name="viewer"]');
const preset = new URLSearchParams(location.search).get("viewer");
if (preset === "pointcloud" || preset === "splat") viewer.value = preset;
const isPointcloud = () => viewer.value === "pointcloud";
const viewerBase = () => new URL(
  isPointcloud() ? "../examples/dimensify/pointcloud_viewer/" : "../splat-viewer/",
  location.href,
);
const viewerLabel = () => isPointcloud() ? "point cloud" : "splat";
const syncControls = () => {
  const pointcloud = isPointcloud();
  for (const [selector, hidden] of [["[data-splat]", pointcloud], ["[data-pointcloud]", !pointcloud]]) {
    for (const node of document.querySelectorAll(selector)) {
      node.hidden = hidden;
      for (const input of node.querySelectorAll("input, select")) input.disabled = hidden;
    }
  }
  const custom = !pointcloud && convention.value === "custom";
  document.querySelector("#custom-axis").hidden = !custom;
  document.querySelector('[name="up"]').disabled = !custom;
  const open = document.querySelector("#open-viewer");
  open.href = viewerBase();
  open.textContent = `Open ${viewerLabel()} viewer`;
  document.querySelector('[name="url"]').placeholder = pointcloud
    ? "https://assets.example.com/cloud.ply"
    : "https://assets.example.com/scene.sog";
};
convention.addEventListener("change", syncControls);
viewer.addEventListener("change", () => {
  syncControls();
  document.querySelector("#launch-result").hidden = true;
  document.querySelector("#launch-error").textContent = "";
});
window.addEventListener("pageshow", syncControls);
syncControls();

document.querySelector("#launch-form").addEventListener("submit", (event) => {
  event.preventDefault();
  const form = new FormData(event.target);
  const error = document.querySelector("#launch-error");
  const output = document.querySelector("#launch-result");
  output.hidden = true;
  error.textContent = "";
  const options = Object.fromEntries(form);
  const vector = (value) =>
    value.split(",").length === 3 &&
    value
      .split(",")
      .every(
        (part) =>
          /^[-+]?(?:\d+(?:\.\d*)?|\.\d+)(?:[eE][-+]?\d+)?$/.test(part.trim()) &&
          Number.isFinite(Number(part)) &&
          Math.abs(Number(part)) <= 1e7,
      );
  if (
    (options.eye || options.target) &&
    !(vector(options.eye || "") && vector(options.target || ""))
  ) {
    error.textContent =
      "Enter both Eye and Look at as three finite coordinates (x,y,z).";
    return;
  }
  if (
    options.eye &&
    Math.hypot(
      ...options.eye
        .split(",")
        .map(
          (part, index) =>
            Math.fround(Number(part)) -
            Math.fround(Number(options.target.split(",")[index])),
        ),
    ) < 1e-4
  ) {
    error.textContent = "Eye and Look at must be at least 0.0001 units apart.";
    return;
  }
  let asset;
  try {
    asset = new URL(options.url);
  } catch {
    error.textContent = "Use an HTTP(S) scene URL.";
    return;
  }
  if (
    !["http:", "https:"].includes(asset.protocol) ||
    (!isPointcloud() && options.format === "auto" && !/\.(ply|sog|json)$/i.test(asset.pathname))
  ) {
    error.textContent = isPointcloud()
      ? "Use an HTTP(S) PLY or Potree source URL."
      : "Use an HTTP(S) scene URL. For an extensionless URL or an unsupported extension, select its data format.";
    return;
  }
  if (isPointcloud() && /(?:\.(?:sog|las|laz|copc)|cloud\.js)$/i.test(asset.pathname)) {
    error.textContent = "Use a PLY file or a Potree 2 dataset directory/metadata.json URL.";
    return;
  }
  if (
    location.protocol === "https:" &&
    asset.protocol === "http:" &&
    !["localhost", "127.0.0.1", "[::1]"].includes(asset.hostname)
  ) {
    error.textContent =
      "Use an HTTPS scene URL for this hosted viewer. HTTP is supported for localhost development.";
    return;
  }
  options.embed = form.has("embed");
  const link = isPointcloud()
    ? pointcloudLink(viewerBase(), { source: options.url, up: options["pointcloud-up"] })
    : viewerLink(viewerBase(), options);
  document.querySelector("#launch-link").textContent = `Open configured ${viewerLabel()} viewer`;
  document.querySelector("#launch-link").href = link;
  document.querySelector("#launch-url").value = link;
  const escaped = link
    .replaceAll("&", "&amp;")
    .replaceAll('"', "&quot;")
    .replaceAll("<", "&lt;");
  document.querySelector("#launch-embed").value =
    `<iframe src="${escaped}" title="Dimensify ${viewerLabel()} viewer" width="100%" height="600" allow="fullscreen" allowfullscreen loading="lazy" referrerpolicy="no-referrer"></iframe>`;
  output.hidden = false;
});
document
  .querySelector("#copy-link")
  .addEventListener("click", async (event) => {
    try {
      await navigator.clipboard.writeText(
        document.querySelector("#launch-url").value,
      );
      event.target.textContent = "Copied";
    } catch {
      event.target.textContent = "Select the URL above to copy";
    }
  });
