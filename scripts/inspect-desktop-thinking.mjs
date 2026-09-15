// Read-only, version-pinned characterization of the signed Desktop picker.
// Evaluate only its pure catalog filter in a VM, never its application entrypoint.
import assert from "node:assert/strict";
import fs from "node:fs";
import vm from "node:vm";

const fd = fs.openSync(process.argv[2], "r");
function readAt(offset, length) {
  const buffer = Buffer.alloc(length);
  assert.equal(fs.readSync(fd, buffer, 0, length, offset), length);
  return buffer;
}
try {
  const prefix = readAt(0, 16);
  const header = JSON.parse(readAt(16, prefix.readUInt32LE(12)));
  const base = 8 + prefix.readUInt32LE(4);
  function asset(name) {
    const entry = header.files.webview.files.assets.files[name];
    assert.ok(entry && !entry.unpacked, `Unsupported Desktop version: ${name}`);
    return readAt(base + Number(entry.offset), entry.size).toString();
  }
  const profile = [
    ["app-initial-9b95fa538c62.js", "app-primary-44ec287874b7.js"],
    ["app-initial-4d7ea7f81c2d.js", "app-primary-4af6ed7f68d1.js"],
  ].find(([initial, primary]) =>
    header.files.webview.files.assets.files[initial] &&
    header.files.webview.files.assets.files[primary],
  );
  assert.ok(profile, "Unsupported Desktop version: unknown picker assets");
  const initial = asset(profile[0]);
  const settings = asset("src-996ff3571e1f.js");
  const primary = asset(profile[1]);
  const defaults = JSON.parse(
    settings.match(/Qy=(\[[^\]]+\])/)[1].replaceAll("`", '"'),
  );
  const filter = initial.match(/function m7n\([\s\S]*?(?=function h7n\()/)[0];
  const visible = initial.match(/function h7n\([\s\S]*?(?=var g7n=)/)[0];
  const valid = initial.match(/function tk\([^]*?(?=var xZn,)/)[0];
  const catalog = JSON.parse(fs.readFileSync(process.argv[3], "utf8"));
  const models = catalog.models
    .filter((x) => x.slug.includes("cloud"))
    .map((x) => ({
      model: x.slug,
      supportedReasoningEfforts: x.supported_reasoning_levels.map((y) => ({
        reasoningEffort: y.effort,
      })),
    }));
  function evaluate(enabled) {
    const context = vm.createContext({ models, enabled });
    return JSON.parse(
      vm.runInContext(
        `${valid}\n${visible}\n${filter}\nJSON.stringify(m7n({authMethod:'chatgpt',availableModels:new Set(),hasConfiguredModelCatalog:true,isCustomModelProvider:true,enabledReasoningEfforts:new Set(enabled),models}).models)`,
        context,
        { timeout: 1000 },
      ),
    );
  }
  const before = evaluate(defaults);
  const after = evaluate([...new Set([...defaults, "max", "none"])]);
  assert.deepEqual(after, models);
  assert.deepEqual(
    before,
    models.map(({ model }) => ({
      model,
      supportedReasoningEfforts: (model === "gemma4:31b-cloud"
        ? ["high"]
        : ["low", "high"]
      ).map((reasoningEffort) => ({ reasoningEffort })),
    })),
  );
  assert.ok(
    primary.includes(
      "defaultMessage:`Light`,description:`Reasoning effort label for a given model: low.",
    ),
  );
  console.log(
    JSON.stringify(
      {
        defaultVisibility: defaults,
        lowLabel: "Light",
        before,
        after,
        result:
          "PASS: actual bundled filter reproduces missing Max/None and preserves both when enabled",
      },
      null,
      2,
    ),
  );
} finally {
  fs.closeSync(fd);
}
