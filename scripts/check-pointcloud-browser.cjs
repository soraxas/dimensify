// Real WebGPU point-cloud controls. Screenshots stay in ignored artifacts.
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const { PNG } = require("pngjs");
const { chromium } = require(process.env.PLAYWRIGHT_MODULE || "playwright");
const { softwareFlags } = require("./visual-browser.cjs");

// A small real Potree 2 dataset keeps URL loading deterministic and exercises
// cross-origin metadata, hierarchy and ranged point requests without the internet.
async function potreeServer() {
  const points = [];
  for (let y = 0; y <= 10; y++) for (let x = 0; x <= 20; x++) points.push([x * 100, y * 100, (x % 3) * 200]);
  const octree = Buffer.alloc(points.length * 18);
  points.forEach((p, i) => {
    p.forEach((v, c) => octree.writeInt32LE(v, i * 18 + c * 4));
    [12000, 54000, 60000].forEach((v, c) => octree.writeUInt16LE(v, i * 18 + 12 + c * 2));
  });
  const hierarchy = Buffer.alloc(22);
  hierarchy[0] = 1; // leaf, no children
  hierarchy.writeUInt32LE(points.length, 2);
  hierarchy.writeBigUInt64LE(BigInt(octree.length), 14);
  const metadata = Buffer.from(JSON.stringify({
    version: "2.0", name: "viewer-url-check", description: "", points: points.length,
    projection: "", hierarchy: { firstChunkSize: 22, stepSize: 5, depth: 0 },
    offset: [1000, 2000, 3000], scale: [0.001, 0.001, 0.001], spacing: 0.15,
    boundingBox: { min: [1000, 2000, 3000], max: [1002, 2002, 3002] }, encoding: "DEFAULT",
    attributes: [
      { name: "position", description: "", size: 12, numElements: 3, elementSize: 4, type: "int32", min: [1000, 2000, 3000], max: [1002, 2001, 3000.4] },
      { name: "rgb", description: "", size: 6, numElements: 3, elementSize: 2, type: "uint16", min: [0, 0, 0], max: [65535, 65535, 65535] },
    ],
  }));
  const multiMetadata = Buffer.from(JSON.stringify({ ...JSON.parse(metadata), octreeLayout: "multi-file" }));
  const ply = Buffer.from("ply\nformat ascii 1.0\nelement vertex " + points.length + "\nproperty float x\nproperty float y\nproperty float z\nproperty uchar red\nproperty uchar green\nproperty uchar blue\nend_header\n" + points.map(p => `${p[0] / 1000} ${p[1] / 1000} ${p[2] / 1000} 45 210 240`).join("\n") + "\n");
  const requests = [];
  const server = require("node:http").createServer((req, res) => {
    res.setHeader("Access-Control-Allow-Origin", "*");
    res.setHeader("Access-Control-Allow-Headers", "Range");
    res.setHeader("Access-Control-Expose-Headers", "Content-Length, Content-Range");
    if (req.method === "OPTIONS") { res.writeHead(204); return res.end(); }
    requests.push({ url: req.url, method: req.method, range: req.headers.range });
    const file = new URL(req.url, "http://fixture").pathname;
    const data = { "/metadata.json": metadata, "/hierarchy.bin": hierarchy, "/octree.bin": octree, "/scan.ply": ply, "/multi/metadata.json": multiMetadata, "/multi/hierarchy.bin": hierarchy, "/multi/octree/r.bin": octree }[file];
    if (!data) { res.writeHead(404); return res.end(); }
    let body = data;
    const range = /^bytes=(\d+)-(\d+)?$/.exec(req.headers.range || "");
    if (range) {
      const start = Number(range[1]), end = Math.min(Number(range[2] || data.length - 1), data.length - 1);
      body = data.subarray(start, end + 1);
      res.statusCode = 206;
      res.setHeader("Content-Range", `bytes ${start}-${end}/${data.length}`);
    }
    res.setHeader("Content-Length", body.length);
    res.end(req.method === "HEAD" ? undefined : body);
  });
  await new Promise(resolve => server.listen(0, "127.0.0.1", resolve));
  server.unref();
  return { server, requests, files: {metadata, hierarchy, octree, multiMetadata, ply}, url: `http://127.0.0.1:${server.address().port}/metadata.json?rev=open-panel` };
}

function difference(a, b) {
  a = PNG.sync.read(a); b = PNG.sync.read(b);
  assert.equal(a.data.length, b.data.length);
  let changed = 0;
  for (let i = 0; i < a.data.length; i += 4) {
    if (Math.max(...[0, 1, 2].map(c => Math.abs(a.data[i + c] - b.data[i + c]))) > 8) changed++;
  }
  return changed / (a.width * a.height);
}

(async () => {
  const [site, out = "dist/pointcloud-browser-check"] = process.argv.slice(2);
  assert(site, "Usage: node scripts/check-pointcloud-browser.cjs SITE_URL [EVIDENCE]");
  fs.mkdirSync(out, { recursive: true });
  const fixture = await potreeServer();
  const browser = await chromium.launch({
    headless: process.env.POINTCLOUD_BROWSER_HEADED !== "1",
    executablePath: process.env.CHROMIUM_EXECUTABLE,
    args: softwareFlags(),
  });
  const errors = [], logs = [], checks = [];
  try {
    const page = await browser.newPage({ viewport: { width: 1000, height: 760 }, deviceScaleFactor: 1 });
    page.on("pageerror", e => errors.push(String(e)));
    page.on("console", m => { logs.push(m.text()); if (m.type() === "error") errors.push(m.text()); });
    const capture = (name, clip) => page.screenshot({ path: path.join(out, `${name}.png`), ...(clip ? { clip } : {}) });
    const click = async (x, y) => {
      await page.mouse.move(x, y);
      await page.waitForTimeout(200);
      await page.mouse.down();
      await page.waitForTimeout(200);
      await page.mouse.up();
      await page.waitForTimeout(400);
      assert.deepEqual(errors, []);
    };
    // Hold the modifier across presented frames. Sending down/up in the same
    // frame leaves Bevy's current modifier state released during text dispatch.
    const command = async key => {
      const modifier = process.platform === "darwin" ? "Meta" : "Control";
      await page.keyboard.down(modifier);
      await page.waitForTimeout(200);
      await page.keyboard.press(key);
      await page.waitForTimeout(200);
      await page.keyboard.up(modifier);
      await page.waitForTimeout(200);
    };
    await page.goto(new URL("examples/dimensify/pointcloud_viewer/", site.endsWith("/") ? site : site + "/").href);
    const deadline = Date.now() + 90000;
    while (!logs.some(line => line.includes("PointCloud added"))) {
      assert.deepEqual(errors, []);
      assert(Date.now() < deadline, "Point-cloud renderer did not start");
      await page.waitForTimeout(100);
    }
    await page.waitForTimeout(2000);
    const closed = await capture("desktop-closed");
    await click(55, 726);
    const opened = await capture("desktop-settings");
    assert(difference(closed, opened) > 0.02, "Settings must open a visible panel");
    checks.push("open-settings");
    const cloudClip = { x: 180, y: 180, width: 480, height: 400 };
    const circle = await capture("circular-billboard", cloudClip);
    await click(820, 122);
    await capture("circle-mesh-settings");
    await capture("circle-mesh", cloudClip);
    await click(820, 122);
    const square = await capture("square-billboard", cloudClip);
    assert(difference(circle, square) > 0.0001, "Point shape must change rendered cloud pixels");
    checks.push("circle-mesh-and-square-render");
    await click(820, 174);
    const aligned = await capture("normal-aligned", cloudClip);
    assert(difference(square, aligned) > 0.0001, "Normal alignment must change rendered cloud pixels");
    checks.push("normal-aligned-render");
    await click(820, 232);
    const manual = await capture("manual-size", cloudClip);
    // Each browser-native key event goes through the actual number editor.
    await click(820, 307);
    await command("a");
    await page.waitForTimeout(200);
    // Shrink the 0.3-world-unit fixture points. Oversized overlapping splats
    // turn this input check into an overdraw stress test on software WebGPU.
    await page.keyboard.type("0.1");
    await page.waitForTimeout(200);
    await page.waitForTimeout(500);
    await capture("number-settings");
    const small = await capture("small-points", cloudClip);
    assert(difference(manual, small) > 0.001, "Point-size editor must update the renderer");
    await click(820, 388);
    const blurred = await capture("size-after-blur", cloudClip);
    assert(difference(small, blurred) < 0.001, "Changing numeric focus must preserve the value without Enter");
    checks.push("number-editor-render");
    await page.mouse.move(820, 550);
    await page.mouse.wheel(0, 1200);
    await page.waitForTimeout(700);
    await capture("desktop-settings-bottom");
    await click(820, 596);
    const reset = await capture("reset-defaults", cloudClip);
    assert(difference(circle, reset) < 0.001, "Reset settings must restore the initial rendered cloud");
    checks.push("reset-settings-render");
    await click(180, 726);
    const openPanel = await capture("open-url-panel");
    await click(180, 172);
    await page.keyboard.type("https://example.org/cloud.laz", { delay: 15 });
    await click(66, 284);
    const invalid = await capture("unsupported-url");
    assert(difference(openPanel, invalid) > 0.001, "Unsupported format must show a visible error");
    assert.equal(fixture.requests.length, 0, "Validation must not fetch or replace the current cloud");
    await click(180, 172);
    await command("a");
    await page.waitForTimeout(250);
    await page.context().grantPermissions(["clipboard-read", "clipboard-write"]);
    await page.evaluate(value => navigator.clipboard.writeText(value), fixture.url);
    await command("v");
    await page.waitForTimeout(500);
    await capture("pasted-url");
    const added = logs.filter(line => line.includes("PointCloud added")).length;
    await page.keyboard.press("Enter");
    const loadDeadline = Date.now() + 30000;
    while (!fixture.requests.some(r => r.url.startsWith("/octree.bin")) || logs.filter(line => line.includes("PointCloud added")).length <= added) {
      assert.deepEqual(errors, []);
      if (Date.now() >= loadDeadline) await capture("url-load-timeout");
      assert(Date.now() < loadDeadline, "URL panel did not load the Potree dataset");
      await page.waitForTimeout(100);
    }
    await page.waitForTimeout(1500);
    const loaded = await capture("url-loaded-cloud", cloudClip);
    assert(difference(circle, loaded) > 0.01, "Loading a URL must replace the fixture's rendered cloud");
    const loadedImage = PNG.sync.read(loaded);
    let cloudPixels = 0;
    for (let i = 0; i < loadedImage.data.length; i += 4) if (loadedImage.data[i + 1] > 40 && loadedImage.data[i + 2] > 70) cloudPixels++;
    assert(cloudPixels > 500, "URL dataset must render visible points after auto framing");
    assert(fixture.requests.some(r => r.url === "/metadata.json?rev=open-panel"));
    assert(fixture.requests.some(r => r.url === "/hierarchy.bin?rev=open-panel"));
    assert(fixture.requests.some(r => r.url === "/octree.bin?rev=open-panel" && r.range), "Potree point bytes must use Range");
    checks.push("open-url-validation-and-potree-render");
    await click(180, 726);
    await capture("current-source-panel");
    await click(150, 340);
    await page.waitForTimeout(1500);
    const demo = await capture("demo-restored", cloudClip);
    assert(difference(circle, demo) < 0.001, "Demo action must restore the initial fixture and camera");
    checks.push("restore-built-in-demo");
    // Real file-picker change events, byte decoding and GPU rendering.
    const waitCloud = async (before, name) => {
      const deadline = Date.now() + 30000;
      while (logs.filter(line => line.includes("PointCloud added")).length <= before) {
        assert.deepEqual(errors, []);
        if (Date.now() >= deadline) await capture(`${name}-timeout`);
        assert(Date.now() < deadline, `${name} did not create a cloud`);
        await page.waitForTimeout(100);
      }
      await page.waitForTimeout(1500);
      const pixels = PNG.sync.read(await capture(name, cloudClip));
      let visible = 0;
      for (let i=0; i<pixels.data.length; i+=4) if (pixels.data[i+1]>40 && pixels.data[i+2]>70) visible++;
      assert(visible > 500, `${name} must render points after auto framing`);
      assert.deepEqual(errors, []);
    };
    const count = () => logs.filter(line => line.includes("PointCloud added")).length;
    await click(180,726);
    const chooseFile = page.waitForEvent("filechooser");
    await click(90,397);
    let before = count();
    await (await chooseFile).setFiles({name:"scan.ply",mimeType:"application/octet-stream",buffer:fixture.files.ply});
    await waitCloud(before,"local-ply-picker");
    checks.push("local-ply-picker-render");
    // A browser DataTransfer drop follows the document's actual drop handler.
    before = count();
    await page.evaluate(bytes => {
      const transfer = new DataTransfer();
      transfer.items.add(new File([new Uint8Array(bytes)],"dropped.ply"));
      document.dispatchEvent(new DragEvent("dragenter", {dataTransfer:transfer,bubbles:true,cancelable:true}));
    },Array.from(fixture.files.ply));
    await page.waitForTimeout(400);
    await capture("drop-overlay");
    await page.evaluate(bytes => {
      const transfer = new DataTransfer();
      transfer.items.add(new File([new Uint8Array(bytes)],"dropped.ply"));
      document.dispatchEvent(new DragEvent("drop", {dataTransfer:transfer,bubbles:true,cancelable:true}));
    },Array.from(fixture.files.ply));
    await waitCloud(before,"local-ply-drop");
    checks.push("ply-drag-drop-render");
    // URL PLY uses a whole GET, never a Range request.
    await click(180,726);
    await click(180,172); await command("a");
    const plyUrl = new URL("scan.ply?rev=ply", fixture.url).href;
    await page.keyboard.type(plyUrl,{delay:10});
    before = count(); await page.keyboard.press("Enter");
    await waitCloud(before,"remote-ply");
    assert(fixture.requests.some(r=>r.url==="/scan.ply?rev=ply" && r.method==="GET" && !r.range));
    checks.push("whole-file-ply-url-render");
    // Both selected directory layouts must stream without HTTP requests.
    const local = path.resolve(out,"local-fixtures"); fs.mkdirSync(local,{recursive:true});
    for (const multi of [false,true]) {
      const dir = path.join(local,multi?"multi":"single"); fs.mkdirSync(dir,{recursive:true});
      fs.writeFileSync(path.join(dir,"metadata.json"),multi?fixture.files.multiMetadata:fixture.files.metadata);
      fs.writeFileSync(path.join(dir,"hierarchy.bin"),fixture.files.hierarchy);
      if (multi) fs.mkdirSync(path.join(dir,"octree"));
      fs.writeFileSync(path.join(dir,multi?"octree/r.bin":"octree.bin"),fixture.files.octree);
      await click(180,726);
      const chooseFolder = page.waitForEvent("filechooser"); await click(255,397);
      const requestsBefore = fixture.requests.length;
      before=count(); await (await chooseFolder).setFiles(dir);
      await waitCloud(before,multi?"local-potree-multi":"local-potree-single");
      assert.equal(fixture.requests.length,requestsBefore,"Local Potree must not make HTTP requests");
      checks.push(multi?"local-multi-file-potree-render":"local-single-file-potree-render");
    }
    // Directory drops expose FileSystemEntry objects. Put required files after
    // the first 100 entries so stopping after one readEntries batch fails.
    before=count();
    await page.evaluate(data => {
      const leaf = (name,bytes) => ({name,isFile:true,isDirectory:false,file:resolve=>resolve(new File([new Uint8Array(bytes)],name))});
      const directory = (name,children) => ({name,isFile:false,isDirectory:true,createReader:()=>{
        let offset=0; return {readEntries:resolve=>{const batch=children.slice(offset,offset+100); offset+=100; resolve(batch);}};
      }});
      const root = directory("dropped-potree",[
        ...Array.from({length:101},(_,i)=>leaf(`note-${i}.txt`,[])),
        leaf("metadata.json",data.metadata),leaf("hierarchy.bin",data.hierarchy),
        directory("octree",[leaf("r.bin",data.octree)]),
      ]);
      const transfer = new DataTransfer(); transfer.items.add(new File(["folder"],"dropped-potree"));
      Object.defineProperty(transfer,"items",{value:[{webkitGetAsEntry:()=>root}]});
      document.dispatchEvent(new DragEvent("drop",{dataTransfer:transfer,bubbles:true,cancelable:true}));
    },{metadata:Array.from(fixture.files.multiMetadata),hierarchy:Array.from(fixture.files.hierarchy),octree:Array.from(fixture.files.octree)});
    await waitCloud(before,"potree-directory-drop");
    checks.push("potree-directory-drop-multiple-entry-batches");
    const stableClip = {x:450,y:180,width:350,height:400};
    const stable = await capture("before-incomplete-folder",stableClip);
    before=count();
    await page.evaluate(bytes=>{
      const transfer=new DataTransfer(); transfer.items.add(new File([new Uint8Array(bytes)],"metadata.json"));
      document.dispatchEvent(new DragEvent("drop",{dataTransfer:transfer,bubbles:true,cancelable:true}));
    },Array.from(fixture.files.multiMetadata));
    await page.waitForTimeout(800);
    await capture("incomplete-folder-error");
    assert.equal(count(),before,"Incomplete folder must preserve the current source");
    assert(difference(stable,await capture("after-incomplete-folder",stableClip))<0.001,"Incomplete selection must preserve rendered points");
    checks.push("incomplete-folder-preserves-cloud");
    // Close the validation panel; the next URL test opens it afresh.
    await page.keyboard.press("Escape"); await page.waitForTimeout(400);
    // Hosted per-node point bytes are plain GETs. Shared hierarchy is ranged.
    await click(180,726); await click(180,172); await command("a");
    await page.keyboard.type(new URL("multi/metadata.json?rev=multi",fixture.url).href,{delay:10});
    before=count(); await page.keyboard.press("Enter"); await waitCloud(before,"remote-potree-multi");
    assert(fixture.requests.some(r=>r.url==="/multi/octree/r.bin?rev=multi" && !r.range));
    assert(fixture.requests.some(r=>r.url==="/multi/hierarchy.bin?rev=multi" && r.range));
    checks.push("hosted-multi-file-potree-render");
    await click(180, 726);
    await page.setViewportSize({ width: 360, height: 640 });
    await capture("narrow-open-url");
    await page.keyboard.press("Escape");
    await page.waitForTimeout(400);
    await click(55, 606);
    await page.mouse.move(200, 200);
    await page.mouse.wheel(0, -1500);
    await page.waitForTimeout(700);
    await capture("narrow-settings");
    await click(65, 606);
    await capture("narrow-closed");
    checks.push("narrow-panel-and-close");
    assert.deepEqual(errors, []);
    fs.writeFileSync(path.join(out, "result.json"), JSON.stringify({ checks, errors }, null, 2));
    console.log(JSON.stringify({ result: "PASS", checks, evidence: out }));
  } finally {
    fs.writeFileSync(path.join(out, "potree-requests.json"), JSON.stringify(fixture.requests, null, 2));
    fixture.server.closeAllConnections();
    fixture.server.close();
    fs.writeFileSync(path.join(out, "browser.log"), logs.join("\n") + "\n" + errors.join("\n"));
    await browser.close();
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
