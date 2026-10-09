// Run with a separate tool directory containing looks-same 10.0.1, pixelmatch 7.2.0 and pngjs 7.0.0.
// node docs/gptbridge/dot-compatibility/compare.mjs /absolute/path/to/tool-directory
import { createRequire } from "node:module";
import { pathToFileURL, fileURLToPath } from "node:url";
import { readFileSync, writeFileSync } from "node:fs";
import path from "node:path";
const toolRoot = process.argv[2];
if (!toolRoot || !path.isAbsolute(toolRoot)) throw new Error("Supply the absolute isolated tool directory.");
const require = createRequire(path.join(toolRoot, "package.json"));
const looksSame = require("looks-same");
const { PNG } = require("pngjs");
const { default: pixelmatch } = await import(pathToFileURL(require.resolve("pixelmatch")));
const root = path.dirname(fileURLToPath(import.meta.url));
const screenshots = path.join(root, "screenshots");
const pairs = [
  ["source-desktop", "prototype-desktop"],
  ["source-mobile", "prototype-mobile"],
  ["prototype-desktop", "component-desktop-native-unavailable"],
  ["prototype-mobile", "component-mobile-native-unavailable"],
];
const report = {
  generated_at: new Date().toISOString(),
  engines: { looks_same: "10.0.1", pixelmatch: "7.2.0", pngjs: "7.0.0" },
  interpretation: "NON_ISOMORPHIC: attachment includes a document/app shell and blue tokens; the accepted prototype uses current green tokens and simultaneous sections. The actual component has no native backend in this browser and shows unavailable states. Pixel equality is not an acceptance threshold. These images are not installed-application evidence.",
  pairs: [],
};
for (const [reference, current] of pairs) {
  const referencePath = path.join(screenshots, `${reference}.png`), currentPath = path.join(screenshots, `${current}.png`);
  const result = await looksSame(referencePath, currentPath, { createDiffImage: true, tolerance: 2.3, ignoreAntialiasing: true, ignoreCaret: true });
  const looksDiff = `${reference}-vs-${current}-looks-same.png`;
  if (result.diffImage) await result.diffImage.save(path.join(screenshots, looksDiff));
  const a = PNG.sync.read(readFileSync(referencePath)), b = PNG.sync.read(readFileSync(currentPath));
  if (a.width !== b.width || a.height !== b.height) throw new Error(`Dimensions differ for ${reference}/${current}; no resize performed.`);
  const diff = new PNG({ width: a.width, height: a.height });
  const mismatched = pixelmatch(a.data, b.data, diff.data, a.width, a.height, { threshold: 0.1 });
  const pixelDiff = `${reference}-vs-${current}-pixelmatch.png`;
  writeFileSync(path.join(screenshots, pixelDiff), PNG.sync.write(diff));
  report.pairs.push({ reference, current, width: a.width, height: a.height,
    looks_same: { equal: result.equal, different_pixels: result.differentPixels, total_pixels: result.totalPixels, diff_bounds: result.diffBounds, diff: `screenshots/${looksDiff}` },
    pixelmatch: { different_pixels: mismatched, total_pixels: a.width * a.height, different_ratio: mismatched / (a.width * a.height), diff: `screenshots/${pixelDiff}` },
  });
}
writeFileSync(path.join(root, "visual-metrics.json"), JSON.stringify(report, null, 2) + "\n");
console.log(JSON.stringify(report.pairs.map(item => ({ reference: item.reference, current: item.current, looks_same_equal: item.looks_same.equal, pixelmatch_ratio: item.pixelmatch.different_ratio })), null, 2));
