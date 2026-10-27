import { expect, test } from "bun:test";
import { SortJobs } from "./sort_jobs.mjs";

// Controllable transport exercises late messages/errors without timing races.
class Transport {
  onmessage: any;
  onerror: any;
  onmessageerror: any;
  sent: any[] = [];
  terminated = false;
  postMessage(message: any, transfers: any[] = []) {
    this.sent.push(structuredClone(message, { transfer: transfers }));
  }
  terminate() { this.terminated = true; }
  reply(id: number, indices: number[]) {
    this.onmessage({ data: { id, order: new Uint32Array(indices), sortMs: 2 } });
  }
}
const positions = () => new Float32Array([0, 0, -1, 0, 0, -4]);
const axis = new Float32Array([0, 0, 1, 0]);

test("one request per source/view and only positions on first dispatch", () => {
  const worker = new Transport();
  let wakes = 0;
  const jobs = new SortJobs(() => worker, () => wakes++);
  const source = positions();
  const id = jobs.create(source);
  expect(jobs.request(id, axis)).toBe(true);
  expect(jobs.request(id, axis)).toBe(false);
  expect(source.length).toBe(6); // Rust's input must not be detached.
  expect(worker.sent[0].positions).toEqual(source);
  worker.reply(id, [1, 0]);
  expect(jobs.status(id)).toBe(1);
  expect(Array.from(jobs.take(id))).toEqual([1, 0]);
  expect(wakes).toBe(1);
  expect(jobs.request(id, new Float32Array([0, 0, -1, 0]))).toBe(true);
  expect(worker.sent[1].positions).toBeUndefined();
  worker.reply(id, [0, 1]);
  expect(Array.from(jobs.take(id))).toEqual([0, 1]);
  jobs.remove(id);
  expect(worker.terminated).toBe(true);
});

test("retired generations never publish into a replacement and views serialize", () => {
  const worker = new Transport();
  const jobs = new SortJobs(() => worker);
  const old = jobs.create(positions());
  const otherView = jobs.create(positions());
  jobs.request(old, axis);
  jobs.request(otherView, axis);
  expect(worker.sent.filter(m => m.axis).length).toBe(1);
  jobs.remove(old);
  const replacement = jobs.create(positions());
  expect(replacement).not.toBe(old);
  jobs.request(replacement, axis);
  worker.reply(old, [1, 0]);
  expect(jobs.status(replacement)).toBe(0);
  expect(worker.sent.filter(m => m.axis).at(-1).id).toBe(otherView);
  worker.reply(otherView, [0, 1]);
  expect(Array.from(jobs.take(otherView))).toEqual([0, 1]);
  worker.reply(replacement, [1, 0]);
  expect(Array.from(jobs.take(replacement))).toEqual([1, 0]);
  jobs.remove(otherView);
  jobs.remove(replacement);
});

test("worker failure and malformed replies expose fallback instead of stale readiness", () => {
  for (const fail of [
    (w: Transport, id: number) => w.onerror({ message: "shader unrelated; worker boot failed" }),
    (w: Transport, id: number) => w.reply(id, [999, 0]),
    (w: Transport, id: number) => w.reply(id, [1]),
  ]) {
    const worker = new Transport();
    const jobs = new SortJobs(() => worker);
    const id = jobs.create(positions());
    jobs.request(id, axis);
    fail(worker, id);
    expect(jobs.status(id)).toBe(-1);
    expect(jobs.take(id).length).toBe(0);
    expect(worker.terminated).toBe(true);
    jobs.remove(id);
  }
});

test("blocked workers time out and wake the viewer for fallback", async () => {
  const worker = new Transport();
  let wakes = 0;
  const jobs = new SortJobs(() => worker, () => wakes++, 5);
  const id = jobs.create(positions());
  jobs.request(id, axis);
  await new Promise(resolve => setTimeout(resolve, 20));
  expect(jobs.status(id)).toBe(-1);
  expect(wakes).toBe(1);
  jobs.remove(id);
});

test("busy earlier views cannot starve a queued later view", () => {
  const worker = new Transport();
  const jobs = new SortJobs(() => worker);
  const ids = [0, 1, 2].map(() => jobs.create(positions()));
  ids.forEach(id => jobs.request(id, axis));
  worker.reply(ids[0], [1, 0]);
  jobs.take(ids[0]);
  jobs.request(ids[0], axis);
  worker.reply(ids[1], [1, 0]);
  expect(worker.sent.filter(m => m.axis).at(-1).id).toBe(ids[2]);
  ids.forEach(id => jobs.remove(id));
});

test("events from a terminated worker cannot fail a new source lifetime", () => {
  const workers = [new Transport(), new Transport()];
  let next = 0;
  const jobs = new SortJobs(() => workers[next++]);
  const old = jobs.create(positions());
  jobs.request(old, axis);
  jobs.remove(old);
  const current = jobs.create(positions());
  jobs.request(current, axis);
  workers[0].onerror({ message: "late event after terminate" });
  workers[1].reply(current, [1, 0]);
  expect(jobs.status(current)).toBe(1);
  expect(Array.from(jobs.take(current))).toEqual([1, 0]);
  jobs.remove(current);
});
