import test from "node:test";
import assert from "node:assert/strict";
import { viewerLink, scenePresets } from "../splat-web/gallery/launch.mjs";

test("launch links preserve signed scene queries and a project base path", () => {
  const scene = "https://assets.test/scene.sog?key=a+b&next=x%2Fy";
  const url = new URL(
    viewerLink("https://host.test/dimensify/splat-viewer/", {
      url: scene,
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
