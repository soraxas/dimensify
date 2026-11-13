import { examples } from "./catalog.mjs";
import { robotLink, robotPresets, scenePresets } from "./launch.mjs";
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
  if (filter === "robots") {
    grid.replaceChildren(robotForm());
    for (const preset of robotPresets(location.href)) {
      const card = element("article");
      const link = element("a", "Open robot", "live");
      link.href = preset.href;
      card.append(
        element("span", "URDF / WebGPU", "badge browser"),
        element("h2", preset.title),
        element("p", preset.description),
        element("p", preset.source, "note"),
        link,
      );
      grid.append(card);
    }
    updateFilter(filter, `${grid.children.length - 1} robot presets`);
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
      const splat = example.href === "../splat-viewer/";
      const actions = element("div", "", "actions");
      const link = element(
        "a",
        splat ? "Open scene" : "Open browser example",
        "live",
      );
      link.href = example.href;
      actions.append(link);
      const pointcloud = example.href === "../examples/dimensify/pointcloud_viewer/";
      if (splat || pointcloud) {
        const builder = element("a", "Build a link", "secondary");
        builder.href = `./build-link.html?viewer=${pointcloud ? "pointcloud" : "splat"}`;
        actions.append(builder);
      }
      card.append(actions);
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
function robotForm() {
  const card = element("article");
  const form = element("form", "", "robot-url");
  const label = element("label", "URDF URL");
  const input = element("input");
  input.type = "url";
  input.required = true;
  input.name = "urdf";
  input.placeholder = "https://github.com/.../robot.urdf";
  label.append(input);
  const open = element("button", "Open in viewer", "live");
  open.type = "submit";
  form.append(label, open);
  form.addEventListener("submit", (event) => {
    event.preventDefault();
    location.href = robotLink(
      new URL("../examples/dimensify/robosim/", location.href),
      { urdf: input.value },
    );
  });
  card.append(
    element("span", "Your robot", "badge browser"),
    element("h2", "Load a URDF by URL"),
    element(
      "p",
      "Paste a raw or GitHub file URL. package:// meshes resolve from a directory named after the package, then each parent directory. Hosts must allow CORS.",
    ),
    form,
  );
  return card;
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
  render(filters.has(location.hash.slice(1)) ? location.hash.slice(1) : "all");
}
addEventListener("hashchange", route);
route();
