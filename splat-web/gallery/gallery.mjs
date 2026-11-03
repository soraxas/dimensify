import { examples } from "./catalog.mjs";
import { viewerLink, scenePresets } from "./launch.mjs";
const grid = document.querySelector("#examples");
const results = document.querySelector("#results");
const labels = {
  browser: "Browser / WebGPU",
  native: "Native viewer",
  headless: "Headless",
  local: "Local / unclassified",
};
const element = (tag, text, className) => {
  const node = document.createElement(tag);
  if (text) node.textContent = text;
  if (className) node.className = className;
  return node;
};
function render(filter) {
  document.querySelector("#launcher").hidden = filter !== "scenes";
  if (filter === "scenes") {
    grid.replaceChildren();
    for (const preset of scenePresets(location.href)) {
      const card = element("article");
      const link = element("a", "Open scene", "live scene");
      link.href = preset.href;
      card.append(
        element("span", "Generated demo / WebGPU", "badge browser"),
        element("h2", preset.title),
        element("p", preset.description),
        element(
          "p",
          "384 splats; illustrative only. This is not a real-scene performance benchmark.",
          "note",
        ),
        link,
      );
      grid.append(card);
    }
    updateFilter(filter, `${grid.children.length} scene presets`);
    return;
  }
  const visible = examples.filter(
    (example) =>
      filter === "all" ||
      (filter === "developer"
        ? !example.targets.includes("browser")
        : example.targets.includes(filter)),
  );
  grid.replaceChildren();
  for (const example of visible) {
    const card = element("article");
    const badges = element("div", "", "badges");
    for (const target of example.targets)
      badges.append(element("span", labels[target], `badge ${target}`));
    card.append(
      badges,
      element("h2", example.title),
      element("code", example.package, "package"),
      element("p", example.description),
      element("p", example.note, "note"),
    );
    if (example.href) {
      const link = element("a", "Open browser example", "live");
      link.href = example.href;
      card.append(link);
    }
    const details = element("details");
    const pre = element("pre");
    pre.append(element("code", example.command));
    const copy = element("button", "Copy command", "copy");
    copy.type = "button";
    copy.setAttribute("aria-label", `Copy command for ${example.title}`);
    copy.addEventListener("click", async () => {
      try {
        await navigator.clipboard.writeText(example.command);
        copy.textContent = "Copied";
      } catch {
        copy.textContent = "Select the command above to copy";
      }
    });
    details.append(
      element("summary", "Run locally / source"),
      pre,
      copy,
      element("p", example.source, "source"),
    );
    card.append(details);
    grid.append(card);
  }
  updateFilter(
    filter,
    `${visible.length} example${visible.length === 1 ? "" : "s"}`,
  );
}
function updateFilter(filter, text) {
  results.textContent = text;
  for (const button of document.querySelectorAll("[data-filter]"))
    button.setAttribute(
      "aria-pressed",
      String(button.dataset.filter === filter),
    );
}
const filters = new Set(
  [...document.querySelectorAll("[data-filter]")].map(
    (button) => button.dataset.filter,
  ),
);
for (const button of document.querySelectorAll("[data-filter]"))
  button.addEventListener("click", () => {
    location.hash = button.dataset.filter;
  });
function route() {
  render(
    filters.has(location.hash.slice(1)) ? location.hash.slice(1) : "browser",
  );
}
addEventListener("hashchange", route);
route();

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
          part.trim() &&
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
    !/\.(ply|sog|json)$/i.test(asset.pathname)
  ) {
    error.textContent = "Use an HTTP(S) URL ending in .ply, .sog or .json.";
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
