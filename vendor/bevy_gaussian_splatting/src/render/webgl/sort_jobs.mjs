// One bounded worker shared by cloud/view identities. No frame backlog.
export class SortJobs {
  constructor(createWorker, wake = () => {}, timeoutMs = 15000) {
    this.createWorker = createWorker;
    this.wake = wake;
    this.timeoutMs = timeoutMs;
    this.slots = new Map();
    this.queue = [];
    this.nextId = 1;
    this.active = null;
    this.worker = null;
    this.failed = false;
    this.timer = null;
    this.stats = { sorts: 0, positionBytes: 0, indexBytes: 0, sortMs: 0, failures: 0 };
  }
  create(positions) {
    const id = this.nextId++;
    this.slots.set(id, { count: positions.length / 3, positions: positions.slice(), axis: null, pending: false, order: null });
    return id;
  }
  request(id, axis) {
    const slot = this.slots.get(id);
    if (!slot || slot.pending || slot.order || this.failed) return false;
    slot.axis = axis.slice();
    slot.pending = true;
    this.queue.push(id);
    this.pump();
    return true;
  }
  pump() {
    if (this.active !== null || this.failed) return;
    const id = this.queue.shift();
    if (id === undefined) return;
    const slot = this.slots.get(id);
    try {
      if (!this.worker) {
        this.worker = this.createWorker();
        const worker = this.worker;
        worker.onmessage = ({ data }) => { if (this.worker === worker) this.receive(data); };
        worker.onerror = (e) => { if (this.worker === worker) this.fail(e.message || "Worker error"); };
        worker.onmessageerror = () => { if (this.worker === worker) this.fail("Worker message could not be decoded"); };
      }
      const positions = slot.positions;
      const axis = slot.axis;
      slot.positions = null;
      slot.axis = null;
      this.active = id;
      this.timer = setTimeout(() => this.fail("Sort worker timed out"), this.timeoutMs);
      this.worker.postMessage({ id, positions: positions ?? undefined, axis }, positions ? [positions.buffer] : []);
      this.stats.positionBytes += positions ? slot.count * 12 : 0;
    } catch (error) { this.fail(String(error)); }
  }
  receive(data) {
    if (data.id !== this.active) return;
    clearTimeout(this.timer);
    this.active = null;
    const slot = this.slots.get(data.id);
    if (data.error) { this.fail(data.error); return; }
    if (slot) {
      if (!(data.order instanceof Uint32Array) || data.order.length !== slot.count ||
          data.order.some(index => index >= slot.count)) {
        this.fail("Invalid sort worker indices");
        return;
      }
      slot.pending = false;
      slot.order = data.order;
      this.stats.sorts++;
      this.stats.indexBytes += data.order.byteLength;
      this.stats.sortMs = data.sortMs;
      this.wake();
    }
    this.pump();
  }
  status(id) {
    if (this.failed) return -1;
    return this.slots.get(id)?.order ? 1 : 0;
  }
  take(id) {
    const slot = this.slots.get(id);
    const order = slot?.order ?? new Uint32Array();
    if (slot) slot.order = null;
    return order;
  }
  remove(id) {
    this.slots.delete(id);
    this.queue = this.queue.filter(queued => queued !== id);
    if (this.slots.size === 0) {
      clearTimeout(this.timer);
      this.worker?.terminate();
      this.worker = null;
      this.active = null;
      this.failed = false;
    } else if (this.worker && !this.failed) {
      try { this.worker.postMessage({ remove: id }); }
      catch (error) { this.fail(String(error)); }
    }
  }
  fail(reason) {
    if (this.failed) return;
    clearTimeout(this.timer);
    this.worker?.terminate();
    this.worker = null;
    this.active = null;
    this.failed = true;
    this.queue.length = 0;
    this.stats.failures++;
    for (const slot of this.slots.values()) {
      slot.positions = null;
      slot.axis = null;
      slot.order = null;
      slot.pending = false;
    }
    console.warn(`Gaussian sort worker unavailable; using synchronous Rust indices: ${reason}`);
    this.wake();
  }
}

let jobs;
let wake = () => {};
export function sort_install(callback) { wake = callback; }
export function sort_create(positions, version) {
  if (!jobs) {
    jobs = new SortJobs(() => {
      const url = new URL("sort_worker.mjs", document.baseURI);
      url.searchParams.set("v", version);
      return new Worker(url, { type: "module", name: "Gaussian Rust depth sorter" });
    }, () => wake());
    globalThis.__DIMENSIFY_GAUSSIAN_SORT__ = jobs.stats;
  }
  return jobs.create(positions);
}
export function sort_request(id, axis) { return jobs.request(id, axis); }
export function sort_status(id) { return jobs.status(id); }
export function sort_take(id) { return jobs.take(id); }
export function sort_remove(id) { jobs?.remove(id); }
