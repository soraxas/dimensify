const test = require("node:test");
const assert = require("node:assert/strict");
const { connectionOptions, connectBrowser, observeDevice } = require("./perf-device.cjs");
const target = { kind: "physical-mobile", id: "phone-01", model: "Pixel 9", os: "Android 16", provider: "local-usb" };
test("remote replay requires identity and one connection, never desktop emulation", () => {
  assert.throws(() => connectionOptions({ "cdp-endpoint": "http://host" }, {}), /physical-device/);
  assert.throws(() => connectionOptions({ target }, {}), /remote/);
  assert.throws(() => connectionOptions({ target, "cdp-endpoint": "http://host", "ws-endpoint": "wss://host" }, {}), /exactly one/);
  assert.deepEqual(connectionOptions({ target }, { DIMENSIFY_CDP_ENDPOINT: "http://host" }).target, target);
});
test("CDP retains device context and owns no emulated viewport", async () => {
  let connected, closed = false;
  const context = {}, browser = { contexts: () => [context], close: async () => { closed = true; } };
  const result = await connectBrowser({ connectOverCDP: async (...args) => { connected = args; return browser; } }, { target, cdpEndpoint: "http://device", contextOptions: { viewport: { width: 1000, height: 760 } } });
  assert.equal(result.context, context); assert.equal(result.connection, "cdp");
  assert.match(result.isolation, /isolation unavailable/); assert.equal(connected[0], "http://device");
  await result.close(); assert.equal(closed, true);
});
test("remote Playwright context preserves provider viewport", async () => {
  let options;
  const browser = { newContext: async value => { options = value; return { close: async () => {} }; }, close: async () => {} };
  await connectBrowser({ connect: async () => browser }, { target, wsEndpoint: "wss://device", contextOptions: { deviceScaleFactor: 1 } });
  assert.deepEqual(options, {});
});
test("connection errors redact full endpoints, credentials and query tokens", async () => {
  const endpoint = "wss://user:password@host/path?token=secret";
  await assert.rejects(connectBrowser({ connect: async () => { throw new Error(`failed ${endpoint}`); } }, { target, wsEndpoint: endpoint }), error => !/password|secret|user:/.test(error.message));
});
test("device observations reject desktop or missing touch and explicitly limit attestation", async () => {
  const observation = { userAgent: "Android 16", platform: "Linux arm", maxTouchPoints: 5, dpr: 3, screen: { width: 400, height: 800 } };
  const page = { evaluate: async () => observation };
  const result = await observeDevice(page, target); assert.match(result.physicalDeviceEvidence, /cannot independently attest/);
  observation.userAgent = "Desktop Chromium"; await assert.rejects(observeDevice(page, target), /does not match/);
  observation.userAgent = "Android 16"; observation.maxTouchPoints = 0; await assert.rejects(observeDevice(page, target), /touch/);
});
