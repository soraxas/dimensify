import { expect, test } from "bun:test";
import { wasmLinearMemoryBytes } from "./sog-decode-worker.mjs";

test("worker memory capacity observes exported WASM growth independently of live allocation", () => {
  const memory = new WebAssembly.Memory({ initial: 1 });
  expect(wasmLinearMemoryBytes({ memory })).toBe(65536);
  memory.grow(2);
  expect(wasmLinearMemoryBytes({ memory })).toBe(3 * 65536);
});
test("worker memory capability fallback stays unavailable", () => {
  for (const exports of [undefined, {}, { memory: {} }, { memory: { buffer: { byteLength: 7 } } },
    { get memory() { throw Error("inaccessible"); } }]) {
    expect(wasmLinearMemoryBytes(exports)).toBeNull();
  }
});


test("worker handler publishes real wall stages and capacity on success and decode error", async () => {
  const { mkdtempSync, mkdirSync, writeFileSync, rmSync } = await import("node:fs");
  const { tmpdir } = await import("node:os");
  const { join } = await import("node:path");
  const { pathToFileURL } = await import("node:url");
  const directory = mkdtempSync(join(tmpdir(), "dimensify-worker-telemetry-test-"));
  mkdirSync(join(directory, "sog-codec"));
  writeFileSync(join(directory, "sog-codec", "sog_codec.js"), `
    const memory = new WebAssembly.Memory({ initial: 1 });
    export default async function init() { return { memory }; }
    export function decode_image(bytes) {
      memory.grow(1);
      if (bytes[0] === 255) throw Error("decode rejected");
      return { width: 1, height: 1, take_pixels: () => new Uint8Array(4), free() {} };
    }
    export class ExpandedJob {
      add() { memory.grow(1); }
      finish() { return new Uint8Array(2048); }
      free() {}
    }
  `);
  const previous = Object.getOwnPropertyDescriptor(globalThis, "self");
  try {
    for (const mode of ["packed", "expanded", "error"]) {
      const messages: any[] = [];
      const worker: any = {
        location: { href: pathToFileURL(join(directory, "sog-decode-worker.mjs")).href, search: "" },
        postMessage(data: any, transfer: any[]) { messages.push({ data, transfer }); },
      };
      Object.defineProperty(globalThis, "self", { configurable: true, value: worker });
      await import(`./sog-decode-worker.mjs?telemetry-test=${mode}`);
      const data = {
        identity: { job: 1, scene: "1", source: "source" },
        files: [{ name: "a", bytes: new Uint8Array([mode === "error" ? 255 : 0]).buffer }],
        maxDimension: 8192, maxOutputBytes: 4,
        expanded: mode === "expanded" ? { meta: "{}", coefficientsPerChannel: 1 } : undefined,
        dispatchedAtMs: performance.timeOrigin + performance.now(),
      };
      await worker.onmessage({ data });
      expect(messages.length).toBe(1);
      const reply = messages[0].data;
      expect(reply.telemetry.handlerElapsedMs).toBeGreaterThanOrEqual(0);
      expect(reply.telemetry.decodeMs).toBeGreaterThanOrEqual(0);
      expect(reply.telemetry.dispatchToHandlerMs).toBeGreaterThanOrEqual(0);
      expect(reply.telemetry.wasmLinearMemoryBytes).toBeGreaterThanOrEqual(131072);
      expect(reply.telemetry.sentAtMs).toBeGreaterThanOrEqual(data.dispatchedAtMs);
      if (mode === "error") {
        expect(reply.kind).toBe("decode");
        expect(reply.error).toContain("decode rejected");
      } else {
        const buffer = mode === "expanded" ? reply.expanded : reply.textures[0].pixels;
        expect(buffer).toBeInstanceOf(ArrayBuffer);
        expect(messages[0].transfer[0]).toBe(buffer);
      }
      await worker.onmessage({ data });
      expect(messages.length).toBe(1); // one bounded job per worker
    }
  } finally {
    if (previous) Object.defineProperty(globalThis, "self", previous);
    else Reflect.deleteProperty(globalThis, "self");
    rmSync(directory, { recursive: true });
  }
});
