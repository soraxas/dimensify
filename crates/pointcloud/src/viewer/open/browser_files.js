// File-system access only. All visible controls belong to the Bevy viewer.
let inputs;
export function chooseFiles(folder, up, ticket) {
  const input = inputs[folder ? 1 : 0];
  input.selection = { up, ticket };
  input.click();
}

async function entryFiles(entry, prefix = "") {
  const path = prefix + entry.name;
  if (entry.isFile) {
    const file = await new Promise((resolve, reject) => entry.file(resolve, reject));
    return [{ path, file }];
  }
  if (!entry.isDirectory) return [];
  const reader = entry.createReader();
  const files = [];
  // A directory reader may yield only 100 entries per call.
  for (;;) {
    const batch = await new Promise((resolve, reject) => reader.readEntries(resolve, reject));
    if (!batch.length) return files;
    for (const child of batch) files.push(...await entryFiles(child, path + "/"));
  }
}

export function installFiles(publish) {
  const removers = [];
  function on(target, type, callback) {
    target.addEventListener(type, callback);
    removers.push(() => target.removeEventListener(type, callback));
  }
  inputs = [false, true].map(folder => {
    const input = document.createElement("input");
    input.type = "file";
    input.hidden = true;
    input.id = folder ? "dimensify-potree-folder-input" : "dimensify-pointcloud-file-input";
    if (folder) input.setAttribute("webkitdirectory", "");
    else input.accept = ".ply";
    document.body.append(input);
    on(input, "change", () => {
      publish({ kind: "files", ...input.selection, files: Array.from(input.files, file => ({ path: file.webkitRelativePath || file.name, file })) });
      input.value = "";
    });
    on(input, "cancel", () => publish({ kind: "files", ...input.selection, files: [] }));
    return input;
  });
  let depth = 0, dropGeneration = 0;
  on(document, "dragenter", event => {
    if (!Array.from(event.dataTransfer?.types || []).includes("Files")) return;
    event.preventDefault(); depth++; publish({ kind: "hover", value: true });
  });
  on(document, "dragleave", event => {
    event.preventDefault(); depth = Math.max(0, depth - 1);
    if (!depth) publish({ kind: "hover", value: false });
  });
  on(document, "dragover", event => {
    event.preventDefault(); if (event.dataTransfer) event.dataTransfer.dropEffect = "copy";
  });
  on(document, "drop", event => {
    event.preventDefault(); depth = 0;
    publish({ kind: "hover", value: false });
    // Capture entries synchronously: the browser revokes DataTransfer access
    // once this handler returns. Enumerating directories remains asynchronous.
    const entries = Array.from(event.dataTransfer?.items || [], item => item.webkitGetAsEntry?.()).filter(Boolean);
    const loose = Array.from(event.dataTransfer?.files || [], file => ({ path: file.webkitRelativePath || file.name, file }));
    const generation = ++dropGeneration;
    const ticket = publish({ kind: "beginDrop" });
    (async () => {
      try {
        const files = entries.length ? (await Promise.all(entries.map(entry => entryFiles(entry)))).flat() : loose;
        if (generation === dropGeneration) publish({ kind: "dropFiles", ticket, files });
      } catch (error) {
        if (generation === dropGeneration) publish({ kind: "dropError", ticket, message: `Cannot read dropped folder: ${error.message || error}` });
      }
    })();
  });
  return () => { removers.forEach(remove => remove()); inputs.forEach(input => input.remove()); inputs = undefined; };
}
