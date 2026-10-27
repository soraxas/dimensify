import { examples } from "./catalog.mjs";
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
  const visible = examples.filter(
    (example) => filter === "all" || example.targets.includes(filter),
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
  results.textContent = `${visible.length} example${visible.length === 1 ? "" : "s"}`;
  for (const button of document.querySelectorAll("[data-filter]"))
    button.setAttribute(
      "aria-pressed",
      String(button.dataset.filter === filter),
    );
}
for (const button of document.querySelectorAll("[data-filter]"))
  button.addEventListener("click", () => render(button.dataset.filter));
render("all");
