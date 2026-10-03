// Render assets/icon.svg to assets/AppIcon.png (1024 px), the source for the
// bundle's AppIcon.icns and the runtime Dock icon.
// usage: node scripts/render-icon.cjs   (needs Playwright with Chromium;
// a global install works with NODE_PATH="$(npm root -g)")
const { readFileSync } = require("node:fs");
const { join } = require("node:path");
const { chromium } = require("playwright");

const root = join(__dirname, "..");
const svg = readFileSync(join(root, "assets/icon.svg"), "utf8");

(async () => {
const browser = await chromium.launch();
try {
  for (const size of [1024]) {
    const page = await browser.newPage({ viewport: { width: size, height: size } });
    await page.setContent(
      `<html><body style="margin:0;background:transparent">` +
        svg.replace('width="1024" height="1024"', `width="${size}" height="${size}"`) +
        `</body></html>`,
    );
    const out = join(root, "assets/AppIcon.png");
    await page.screenshot({ path: out, omitBackground: true, clip: { x: 0, y: 0, width: size, height: size } });
    await page.close();
    console.log(`wrote ${out}`);
  }
} finally {
  await browser.close();
}
})();
