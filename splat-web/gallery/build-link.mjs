import { viewerLink } from "./launch.mjs";

const convention = document.querySelector('[name="orientation"]');
const syncCustomAxis = () => {
  const custom = convention.value === "custom";
  document.querySelector("#custom-axis").hidden = !custom;
  document.querySelector('[name="up"]').disabled = !custom;
};
convention.addEventListener("change", syncCustomAxis);
window.addEventListener("pageshow", syncCustomAxis);
syncCustomAxis();

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
  const asset = new URL(options.url);
  if (
    !["http:", "https:"].includes(asset.protocol) ||
    (options.format === "auto" && !/\.(ply|sog|json)$/i.test(asset.pathname))
  ) {
    error.textContent =
      "Use an HTTP(S) scene URL. For an extensionless URL or an unsupported extension, select its data format.";
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
  const link = viewerLink(new URL("../splat-viewer/", location.href), options);
  document.querySelector("#launch-link").href = link;
  document.querySelector("#launch-url").value = link;
  const escaped = link
    .replaceAll("&", "&amp;")
    .replaceAll('"', "&quot;")
    .replaceAll("<", "&lt;");
  document.querySelector("#launch-embed").value =
    `<iframe src="${escaped}" title="Dimensify splat viewer" width="100%" height="600" allow="fullscreen" allowfullscreen loading="lazy" referrerpolicy="no-referrer"></iframe>`;
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
