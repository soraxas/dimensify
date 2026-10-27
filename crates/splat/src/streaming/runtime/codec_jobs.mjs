// Exact wire layout: positions, rotations, scales, harmonics; every field is f32.
export function expandedBytes(expanded) {
  return expanded
    ? (expanded.count + 32 - (expanded.count % 32)) *
        (12 + expanded.coefficients) *
        4
    : 0;
}
// Bounded transfer/lifetime contract; no ECS access. Each worker processes one
// job and is terminated even after success to release its WASM memory high water.
export const CODEC_BYTE_ALLOWANCE = 128 * 1024 * 1024;
export const MAX_ENCODED_FILE_BYTES = 32 * 1024 * 1024;

export function reservationFor(files, expanded) {
  let input = 0,
    output = 0,
    largest = 0;
  for (const file of files) {
    const bytes = file.width * file.height * 4;
    input += file.bytes.byteLength;
    output += bytes;
    largest = Math.max(largest, bytes);
  }
  // Input/output handoff copies plus a conservative working envelope for the
  // largest serially decoded image. This is not measured browser-process RSS.
  return (
    4 * 1024 * 1024 +
    4 * input +
    2 * output +
    8 * largest +
    3 * expandedBytes(expanded)
  );
}

function expandedAllowance(files, limits, expanded) {
  if (
    typeof expanded.meta !== "string" ||
    !Number.isSafeInteger(expanded.count) ||
    expanded.count < 0 ||
    ![1, 4, 9, 16, 25].includes(expanded.coefficientsPerChannel) ||
    expanded.coefficients !==
      Math.ceil((expanded.coefficientsPerChannel * 3) / 4) * 4
  )
    throw Error("invalid SOG expanded request");
  // The source-cache admission already owns this immutable splat reservation.
  // Derive the transient envelope from it, never from caller-supplied peak bytes.
  if (
    !Number.isSafeInteger(limits.maxSourceSplats) ||
    limits.maxSourceSplats < expanded.count ||
    limits.maxSourceSplats > 0xffffffff
  )
    throw Error("invalid SOG source admission count");
  const sourceBytes = expandedBytes({
    ...expanded,
    count: limits.maxSourceSplats,
  });
  const textureLimit = limits.flat
    ? 0xffffffff
    : 2 ** Math.ceil(Math.log2(Math.max(1, expanded.count))) * 24 +
      4 * 1024 * 1024;
  const inputBytes = files.reduce(
    (sum, file) => sum + file.bytes.byteLength,
    0,
  );
  const textureBytes = files.reduce(
    (sum, file) => sum + file.width * file.height * 4,
    0,
  );
  if (
    sourceBytes > 0x7fffffff ||
    inputBytes > 0xffffffff ||
    limits.maxOutputBytes > textureLimit ||
    (!limits.flat &&
      files.some((file) => file.bytes.byteLength > MAX_ENCODED_FILE_BYTES))
  )
    throw Error("SOG source or texture admission exceeded");
  // LOD retains its count-bounded textures and transport file limits; flat
  // retains the format/addressability bound without an arbitrary new size cap.
  // Checked input/header/source bounds make this finite. It is a conservative
  // accounting envelope, not a measured or guaranteed browser-process RSS cap.
  return 4 * 1024 * 1024 + 4 * inputBytes + 10 * textureBytes + 3 * sourceBytes;
}

export class CodecJobs {
  constructor(createWorker, byteAllowance = CODEC_BYTE_ALLOWANCE) {
    this.createWorker = createWorker;
    this.byteAllowance = byteAllowance;
    this.active = null;
    this.admission = null;
    this.stats = {
      started: 0,
      completed: 0,
      failed: 0,
      decodeMs: 0,
      reservedBytes: 0,
      peakReservedBytes: 0,
    };
  }
  busy() {
    return this.active !== null || this.admission !== null;
  }
  start(identity, files, limits, expanded) {
    if (this.busy()) throw Error("SOG codec busy");
    if (!Array.isArray(files) || files.length === 0 || files.length > 7)
      throw Error("SOG codec texture count");
    const names = new Set();
    let output = 0;
    for (const file of files) {
      if (
        typeof file.name !== "string" ||
        names.has(file.name) ||
        !(file.bytes instanceof ArrayBuffer) ||
        file.bytes.byteLength === 0 ||
        !Number.isSafeInteger(file.width) ||
        !Number.isSafeInteger(file.height) ||
        file.width <= 0 ||
        file.height <= 0 ||
        file.width > limits.maxDimension ||
        file.height > limits.maxDimension ||
        !Number.isInteger(limits.maxDimension) ||
        limits.maxDimension > (limits.flat ? 16384 : 8192)
      )
        throw Error("invalid SOG codec input");
      names.add(file.name);
      output += file.width * file.height * 4;
    }
    const allowance = expanded
      ? expandedAllowance(files, limits, expanded)
      : this.byteAllowance;
    const reserved = reservationFor(files, expanded);
    if (
      !Number.isSafeInteger(limits.maxOutputBytes) ||
      output > limits.maxOutputBytes ||
      !Number.isSafeInteger(limits.reservedPeakBytes) ||
      reserved !== limits.reservedPeakBytes ||
      limits.reservedPeakBytes > allowance
    )
      throw Error("SOG codec byte allowance exceeded");
    const worker = this.createWorker();
    this.stats.started++;
    this.stats.reservedBytes = reserved;
    this.stats.peakReservedBytes = Math.max(
      this.stats.peakReservedBytes,
      reserved,
    );
    return new Promise((resolve, reject) => {
      const job = { identity, worker, reject, finish: null };
      this.active = job;
      const finish = (error, textures, retainAdmission = false) => {
        if (this.active !== job) return;
        this.active = null;
        clearTimeout(job.timer);
        if (retainAdmission) this.admission = { ...identity, cancelled: false };
        else this.stats.reservedBytes = 0;
        if (error) this.stats.failed++;
        else this.stats.completed++;
        worker.onmessage = null;
        worker.onerror = null;
        worker.onmessageerror = null;
        worker.terminate();
        if (error) reject(Error(error));
        else resolve(textures);
      };
      job.finish = finish;
      job.timer = setTimeout(() => finish("SOG codec timed out"), 60000);
      worker.onmessage = ({ data }) => {
        if (
          data?.identity?.job !== identity.job ||
          data?.identity?.scene !== identity.scene ||
          data?.identity?.source !== identity.source
        )
          return;
        if (data.error) {
          finish(
            data.kind === "boot"
              ? `SOG codec worker failed: ${data.error}`
              : `SOG codec: ${data.error}`,
          );
          return;
        }
        if (expanded) {
          if (
            !(data.expanded instanceof ArrayBuffer) ||
            data.expanded.byteLength !== expandedBytes(expanded)
          ) {
            finish("invalid SOG expanded result length");
            return;
          }
          if (Number.isFinite(data.decodeMs) && data.decodeMs >= 0)
            this.stats.decodeMs += data.decodeMs;
          finish(null, data.expanded, true);
          return;
        }
        const textures = data.textures;
        if (!Array.isArray(textures) || textures.length !== files.length) {
          finish("invalid SOG codec result count");
          return;
        }
        const unique = new Set();
        for (const result of textures) {
          if (!result || typeof result !== "object") {
            finish("invalid SOG codec result shape");
            return;
          }
          const expected = files.find((file) => file.name === result.name);
          if (
            !expected ||
            unique.has(result.name) ||
            result.width !== expected.width ||
            result.height !== expected.height ||
            !(result.pixels instanceof ArrayBuffer) ||
            result.pixels.byteLength !== result.width * result.height * 4
          ) {
            finish("invalid SOG codec result shape");
            return;
          }
          unique.add(result.name);
        }
        if (Number.isFinite(data.decodeMs) && data.decodeMs >= 0)
          this.stats.decodeMs += data.decodeMs;
        finish(null, textures, true);
      };
      worker.onerror = (event) => {
        event.preventDefault?.();
        finish(`SOG codec worker failed: ${event.message ?? "unknown error"}`);
      };
      worker.onmessageerror = () => finish("SOG codec message decode failed");
      try {
        worker.postMessage(
          {
            identity,
            files,
            expanded,
            maxDimension: limits.maxDimension,
            maxOutputBytes: limits.maxOutputBytes,
          },
          files.map((file) => file.bytes),
        );
      } catch (error) {
        finish(`SOG codec transfer failed: ${error}`);
      }
    });
  }
  admitted(id) {
    if (this.admission?.job === id) {
      this.admission = null;
      this.stats.reservedBytes = 0;
    }
  }
  cancelled(id) {
    return this.admission?.job === id && this.admission.cancelled;
  }
  cancel(id) {
    // Rust's yielded admission must retire its partial allocations before the
    // next job may reserve memory. Its weak owner notices cancellation and acks.
    if (this.admission?.job === id) this.admission.cancelled = true;
    if (this.active?.identity.job === id)
      this.active.finish("SOG codec cancelled");
  }
  close() {
    this.active?.finish("SOG codec closed");
    if (this.admission) this.cancel(this.admission.job);
  }
}

let pool;

export function codec_requested(mode) {
  return mode !== "serial";
}
export function codec_unavailable_reason() {
  return typeof Worker === "undefined" ? "module workers are unavailable" : "";
}
export function codec_busy() {
  return pool?.busy() ?? false;
}
export function codec_start(identity, files, limits, version, expanded) {
  if (!pool) {
    pool = new CodecJobs(() => {
      try {
        const url = new URL("sog-decode-worker.mjs", document.baseURI);
        url.searchParams.set("v", version);
        return new Worker(url, {
          type: "module",
          name: "SOG exact codec",
        });
      } catch (error) {
        throw error;
      }
    });
    window.__DIMENSIFY_SOG_CODEC__ = pool.stats;
    window.addEventListener("pagehide", () => pool.close(), { once: true });
  }
  return pool.start(identity, files, limits, expanded);
}
export function codec_cancel(id) {
  pool?.cancel(id);
}

export function codec_admitted(id) {
  pool?.admitted(id);
}
export function codec_yield() {
  // Resume as a browser task without nested timers' delay where supported.
  // A resolved Promise alone would not let input/rendering run between batches.
  if (typeof globalThis.scheduler?.yield === "function")
    return globalThis.scheduler.yield();
  return new Promise((resolve) => setTimeout(resolve, 0));
}

export function codec_cancelled(id) {
  return pool?.cancelled(id) ?? false;
}
