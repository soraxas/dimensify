import { describe, expect, test } from "bun:test";
import {
  CodecJobs,
  codec_requested,
  codec_yield,
  reservationFor,
} from "./codec_jobs.mjs";

class FakeWorker {
  onmessage: ((event: any) => void) | null = null;
  onerror: ((event: any) => void) | null = null;
  onmessageerror: ((event: any) => void) | null = null;
  terminated = false;
  messages: any[] = [];
  postMessage(value: any, transfer: any[]) {
    this.messages.push({ value, transfer });
  }
  terminate() {
    this.terminated = true;
  }
}
const identity = { job: 1, scene: "4294967296", source: "0_0/meta.json" };
const files = () => [
  { name: "lo.webp", bytes: new ArrayBuffer(4), width: 1, height: 1 },
];
const limits = {
  maxDimension: 8192,
  maxOutputBytes: 4,
  reservedPeakBytes: 4 * 1024 * 1024 + 56,
};

describe("bounded SOG codec jobs", () => {
  test("packed replies retain their reservation until admission acknowledges completion", async () => {
    const worker = new FakeWorker();
    const pool = new CodecJobs(() => worker);
    const result = pool.start(identity, files(), limits);
    expect(pool.busy()).toBe(true);
    expect(() => pool.start({ ...identity, job: 2 }, files(), limits)).toThrow(
      "busy",
    );
    expect(worker.messages[0].transfer.length).toBe(1);
    worker.onmessage?.({
      data: { identity: { ...identity, job: 999 }, textures: [] },
    });
    expect(pool.busy()).toBe(true); // stale completion cannot publish/release the active job
    worker.onmessage?.({
      data: {
        identity,
        textures: [
          { name: "lo.webp", width: 1, height: 1, pixels: new ArrayBuffer(4) },
        ],
      },
    });
    expect((await result).length).toBe(1);
    expect(worker.terminated).toBe(true);
    expect(pool.busy()).toBe(true);
    expect(pool.stats.reservedBytes).toBe(limits.reservedPeakBytes);
    pool.admitted(999);
    expect(pool.busy()).toBe(true);
    expect(() => pool.start({ ...identity, job: 2 }, files(), limits)).toThrow(
      "busy",
    );
    pool.admitted(identity.job);
    expect(pool.busy()).toBe(false);
    expect(pool.stats.reservedBytes).toBe(0);
  });
  test("packed admission cancellation retains memory until partial copies are retired", async () => {
    const worker = new FakeWorker();
    const pool = new CodecJobs(() => worker);
    const result = pool.start(identity, files(), limits);
    worker.onmessage?.({
      data: {
        identity,
        textures: [
          { name: "lo.webp", width: 1, height: 1, pixels: new ArrayBuffer(4) },
        ],
      },
    });
    await result;
    pool.cancel(identity.job);
    expect(pool.cancelled(identity.job)).toBe(true);
    expect(pool.busy()).toBe(true);
    expect(pool.stats.reservedBytes).toBe(limits.reservedPeakBytes);
    expect(() => pool.start({ ...identity, job: 2 }, files(), limits)).toThrow(
      "busy",
    );
    pool.admitted(identity.job);
    expect(pool.cancelled(identity.job)).toBe(false);
    expect(pool.busy()).toBe(false);
    expect(pool.stats.reservedBytes).toBe(0);
  });
  test("rejects count/byte overruns before creating a worker", () => {
    let created = 0;
    const pool = new CodecJobs(() => {
      created++;
      return new FakeWorker();
    }, 43);
    expect(() => pool.start(identity, files(), limits)).toThrow(
      "byte allowance",
    );
    expect(() =>
      pool.start(identity, Array(8).fill(files()[0]), limits),
    ).toThrow("texture count");
    expect(created).toBe(0);
  });
  test("cancellation rejects its promise, retires memory and ignores late completions", async () => {
    const worker = new FakeWorker();
    const pool = new CodecJobs(() => worker);
    const result = pool
      .start(identity, files(), limits)
      .catch((error) => error);
    const stale = worker.onmessage;
    pool.cancel(1);
    expect((await result).message).toContain("cancelled");
    expect(worker.terminated).toBe(true);
    expect(pool.busy()).toBe(false);
    stale?.({ data: { identity, textures: [] } });
    expect(pool.busy()).toBe(false);
  });
  test("worker failure, malformed output and teardown release the slot", async () => {
    for (const failure of ["error", "malformed", "teardown"]) {
      const worker = new FakeWorker();
      const pool = new CodecJobs(() => worker);
      const result = pool
        .start(identity, files(), limits)
        .catch((error) => error);
      if (failure === "error")
        worker.onerror?.({ message: "codec crashed", preventDefault() {} });
      if (failure === "malformed")
        worker.onmessage?.({ data: { identity, textures: [] } });
      if (failure === "teardown") pool.close();
      expect(await result).toBeInstanceOf(Error);
      expect(pool.busy()).toBe(false);
      expect(worker.terminated).toBe(true);
      const retry = pool
        .start({ ...identity, job: 2 }, files(), limits)
        .catch((error) => error);
      pool.close();
      expect(await retry).toBeInstanceOf(Error);
    }
  });
  test("null texture results fail gracefully and release the job", async () => {
    const worker = new FakeWorker();
    const pool = new CodecJobs(() => worker);
    const result = pool
      .start(identity, files(), limits)
      .catch((error) => error);
    try {
      expect(() =>
        worker.onmessage?.({ data: { identity, textures: [null] } }),
      ).not.toThrow();
      expect((await result).message).toContain(
        "invalid SOG codec result shape",
      );
      expect(pool.busy()).toBe(false);
      expect(worker.terminated).toBe(true);
    } finally {
      pool.close();
    }
  });
});

test("worker decode is the default and serial is an explicit diagnostic", () => {
  for (const mode of [undefined, "", "worker", "serial"]) {
    expect(codec_requested(mode)).toBe(mode !== "serial");
  }
});

test("admission yields through the scheduler with its receiver when available", async () => {
  const previous = Object.getOwnPropertyDescriptor(globalThis, "scheduler");
  let calls = 0;
  const pending = Promise.resolve();
  const scheduler = {
    yield() {
      expect(this).toBe(scheduler);
      calls++;
      return pending;
    },
  };
  try {
    Object.defineProperty(globalThis, "scheduler", {
      configurable: true,
      value: scheduler,
    });
    expect(codec_yield()).toBe(pending);
    await pending;
    expect(calls).toBe(1);
  } finally {
    if (previous) Object.defineProperty(globalThis, "scheduler", previous);
    else Reflect.deleteProperty(globalThis, "scheduler");
  }
});

test("admission still yields to a task without scheduler support", async () => {
  const previous = Object.getOwnPropertyDescriptor(globalThis, "scheduler");
  try {
    for (const scheduler of [undefined, {}]) {
      Object.defineProperty(globalThis, "scheduler", {
        configurable: true,
        value: scheduler,
      });
      let resumed = false;
      const pending = codec_yield().then(() => {
        resumed = true;
      });
      await Promise.resolve();
      expect(resumed).toBe(false);
      await pending;
      expect(resumed).toBe(true);
    }
  } finally {
    if (previous) Object.defineProperty(globalThis, "scheduler", previous);
    else Reflect.deleteProperty(globalThis, "scheduler");
  }
});

test("expanded replies require the exact padded planar byte length", async () => {
  for (const delta of [0, -4, 4]) {
    const worker = new FakeWorker();
    const pool = new CodecJobs(() => worker);
    const expanded = {
      meta: "{}",
      count: 1,
      coefficients: 4,
      coefficientsPerChannel: 1,
    };
    const size = 32 * 16 * 4;
    const result = pool
      .start(
        identity,
        files(),
        {
          ...limits,
          maxSourceSplats: expanded.count,
          reservedPeakBytes: limits.reservedPeakBytes + 3 * size,
        },
        expanded,
      )
      .catch((e) => e);
    expect(worker.messages[0].value.expanded).toEqual(expanded);
    worker.onmessage?.({
      data: { identity, expanded: new ArrayBuffer(size + delta) },
    });
    const reply = await result;
    if (delta === 0) expect(reply).toBeInstanceOf(ArrayBuffer);
    else expect(reply).toBeInstanceOf(Error);
    expect(pool.busy()).toBe(delta === 0);
    pool.cancel(identity.job);
    expect(pool.cancelled(identity.job)).toBe(delta === 0);
    expect(pool.busy()).toBe(delta === 0);
    pool.admitted(identity.job);
    expect(pool.busy()).toBe(false);
  }
});

test("largest audited SH0 tile fits expanded LOD reservation while packed remains bounded", async () => {
  const worker = new FakeWorker();
  const pool = new CodecJobs(() => worker);
  const encoded = [1960342, 273274, 1988852, 2092220, 2413022].map(
    (length, i) => ({
      name: `${i}.webp`,
      width: 872,
      height: 872,
      bytes: new ArrayBuffer(length),
    }),
  );
  const expanded = {
    meta: "{}",
    count: 758088,
    coefficients: 4,
    coefficientsPerChannel: 1,
  };
  const reservedPeakBytes = reservationFor(encoded, expanded);
  expect(reservedPeakBytes).toBe(239410296);
  const result = pool
    .start(
      identity,
      encoded,
      {
        maxDimension: 8192,
        maxOutputBytes: 15207680,
        maxSourceSplats: expanded.count,
        reservedPeakBytes,
      },
      expanded,
    )
    .catch((e) => e);
  pool.cancel(identity.job);
  expect(await result).toBeInstanceOf(Error);
  expect(pool.stats.started).toBe(1);
});

test("expanded jobs preserve admitted LOD and large flat scenes", async () => {
  const cases = [
    {
      flat: true,
      count: 2097152,
      coefficients: 4,
      coefficientsPerChannel: 1,
      widths: [2048, 2048, 2048, 2048, 2048],
      heights: [1024, 1024, 1024, 1024, 1024],
      lengths: [5000000, 5000000, 5000000, 5000000, 5000000],
    },
    {
      flat: false,
      count: 1048576,
      coefficients: 4,
      coefficientsPerChannel: 1,
      widths: [1024, 1024, 1024, 1024, 1024],
      heights: [1024, 1024, 1024, 1024, 1024],
      lengths: [2500000, 400000, 2500000, 2500000, 3000000],
    },
    {
      flat: false,
      count: 758088,
      coefficients: 48,
      coefficientsPerChannel: 16,
      widths: [872, 872, 872, 872, 872, 960, 872],
      heights: [872, 872, 872, 872, 872, 1024, 872],
      lengths: [1960342, 273274, 1988852, 2092220, 2413022, 1242482, 1480798],
    },
  ];
  for (const data of cases) {
    const pool = new CodecJobs(() => new FakeWorker());
    const expanded = {
      meta: "{}",
      count: data.count,
      coefficients: data.coefficients,
      coefficientsPerChannel: data.coefficientsPerChannel,
    };
    const encoded = data.lengths.map((length, i) => ({
      name: `${i}.webp`,
      width: data.widths[i],
      height: data.heights[i],
      bytes: new ArrayBuffer(length),
    }));
    const limits = {
      flat: data.flat,
      maxDimension: 8192,
      maxOutputBytes:
        2 ** Math.ceil(Math.log2(data.count)) * 24 + 4 * 1024 * 1024,
      maxSourceSplats: data.count,
      reservedPeakBytes: reservationFor(encoded, expanded),
    };
    const result = pool
      .start(identity, encoded, limits, expanded)
      .catch((e) => e);
    expect(pool.busy()).toBe(true);
    expect(pool.stats.reservedBytes).toBe(limits.reservedPeakBytes);
    pool.cancel(identity.job);
    expect(await result).toBeInstanceOf(Error);
  }
});

test("expanded admission rejects forged peaks, source counts and texture bounds", () => {
  const expanded = {
    meta: "{}",
    count: 1,
    coefficients: 4,
    coefficientsPerChannel: 1,
  };
  const encoded = files();
  const valid = {
    ...limits,
    maxSourceSplats: 1,
    reservedPeakBytes: reservationFor(encoded, expanded),
  };
  let created = 0;
  const pool = new CodecJobs(() => {
    created++;
    return new FakeWorker();
  });
  for (const patch of [
    { reservedPeakBytes: valid.reservedPeakBytes + 1 },
    { maxSourceSplats: 0 },
    { maxSourceSplats: 40000000 },
    { maxOutputBytes: 512 * 1024 * 1024 },
  ]) {
    expect(() =>
      pool.start(identity, encoded, { ...valid, ...patch }, expanded),
    ).toThrow();
  }
  expect(created).toBe(0);
});
