import test from "node:test";
import assert from "node:assert/strict";
import { robotLink, robotPresets, viewerLink, scenePresets } from "../splat-web/gallery/launch.mjs";

test("launch links preserve signed scene queries and a project base path", () => {
  const scene = "https://assets.test/scene.sog?key=a+b&next=x%2Fy";
  const url = new URL(
    viewerLink("https://host.test/dimensify/splat-viewer/", {
      url: scene,
      format: "sog-unbundled",
      up: "-z",
      eye: "1,2,3",
      target: "0,0,0",
      fov: "60",
      mode: "orbit",
      embed: true,
    }),
  );
  assert.equal(url.pathname, "/dimensify/splat-viewer/");
  assert.equal(url.searchParams.get("url"), scene);
  assert.equal(url.searchParams.get("format"), "sog-unbundled");
  assert.equal(url.searchParams.get("embed"), "1");
  assert.equal(url.searchParams.get("eye"), "1,2,3");
  assert.equal(url.searchParams.get("target"), "0,0,0");
  assert.equal(url.searchParams.get("fov"), "60");
  assert.equal(url.searchParams.get("mode"), "orbit");
  assert.equal(url.searchParams.get("up"), "-z");
});

test("scene presets share one viewer and resolve fixtures within the site", () => {
  const presets = scenePresets("https://host.test/project/gallery/");
  assert(presets.length >= 3);
  for (const preset of presets) {
    const url = new URL(preset.href);
    assert.equal(url.pathname, "/project/splat-viewer/");
    assert.equal(
      new URL(url.searchParams.get("url")).pathname,
      "/project/scenes/rings.ply",
    );
    assert.equal(url.searchParams.get("eye"), "4,3,6");
  }
  assert(
    presets.some(
      (p) => new URL(p.href).searchParams.get("transparency") === "stochastic",
    ),
  );
});

test("empty optional fields are omitted and unknown form values never become options", () => {
  const url = new URL(
    viewerLink("https://host.test/splat-viewer/", {
      url: "",
      eye: " ",
      token: "secret",
      embed: false,
    }),
  );
  assert.equal(url.search, "");
});

test("coordinate presets produce unambiguous links without changing legacy axis links", () => {
  const preset = new URL(viewerLink("https://host.test/splat-viewer/", {
    orientation: "supersplat", up: "-y",
  }));
  assert.equal(preset.searchParams.get("orientation"), "supersplat");
  assert.equal(preset.searchParams.has("up"), false);
  const custom = new URL(viewerLink("https://host.test/splat-viewer/", {
    orientation: "custom", up: "-y",
  }));
  assert.equal(custom.searchParams.has("orientation"), false);
  assert.equal(custom.searchParams.get("up"), "-y");
});


test("coordinate selection normalizes whitespace before deciding which parameter to emit", () => {
  for (const orientation of [" custom ", "\tcustom\n"]) {
    const link = new URL(viewerLink("https://host.test/splat-viewer/", {
      orientation, up: " -y ",
    }));
    assert.equal(link.searchParams.has("orientation"), false);
    assert.equal(link.searchParams.get("up"), "-y");
  }
  const preset = new URL(viewerLink("https://host.test/splat-viewer/", {
    orientation: " supersplat ", up: "-y",
  }));
  assert.equal(preset.searchParams.get("orientation"), "supersplat");
  assert.equal(preset.searchParams.has("up"), false);
  const legacy = new URL(viewerLink("https://host.test/splat-viewer/", {
    orientation: " ", up: "-y",
  }));
  assert.equal(legacy.searchParams.has("orientation"), false);
  assert.equal(legacy.searchParams.get("up"), "-y");
});


test("omitted orientation preserves an independent signed up axis", () => {
  for (const orientation of [false, undefined, "", " "]) {
    const link = new URL(viewerLink("https://host.test/splat-viewer/", {
      orientation, up: "-y",
    }));
    assert.equal(link.searchParams.has("orientation"), false);
    assert.equal(link.searchParams.get("up"), "-y");
  }
});

test("robot links carry the URDF and repeated package overrides verbatim", () => {
  const urdf = "https://cdn.test/arm.urdf?sig=a+b&x=%2F";
  const link = new URL(
    robotLink("https://host.test/project/examples/dimensify/robosim/?stale=1", {
      urdf: ` ${urdf} `,
      packages: ["arm=https://cdn.test/pkg/", "hand=https://cdn.test/hand/"],
    }),
  );
  assert.equal(link.pathname, "/project/examples/dimensify/robosim/");
  assert.equal(link.searchParams.get("urdf"), urdf);
  assert.deepEqual(link.searchParams.getAll("package"), [
    "arm=https://cdn.test/pkg/",
    "hand=https://cdn.test/hand/",
  ]);
  assert.equal(link.searchParams.has("stale"), false);
  // An empty URDF opens the bundled demo instead of an invalid source.
  assert.equal(new URL(robotLink("https://host.test/r/", { urdf: " " })).search, "");
});

test("robot presets open the robot viewer within the site at a pinned revision", () => {
  const presets = robotPresets("https://host.test/project/gallery/");
  assert(presets.length >= 4);
  assert.equal(new Set(presets.map((preset) => preset.title)).size, presets.length);
  for (const preset of presets) {
    const url = new URL(preset.href);
    assert.equal(url.pathname, "/project/examples/dimensify/robosim/");
    const urdf = url.searchParams.get("urdf");
    if (!urdf) continue;
    const source = new URL(urdf);
    assert.equal(source.protocol, "https:");
    assert.match(source.pathname, /\/[0-9a-f]{40}\//, "pinned commit, not a branch");
    assert.match(source.pathname, /\.urdf$/);
  }
  assert(presets.some((preset) => !new URL(preset.href).searchParams.has("urdf")));
});
