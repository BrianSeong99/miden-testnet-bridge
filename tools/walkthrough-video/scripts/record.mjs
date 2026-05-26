import { chromium } from "playwright";
import fs from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(__dirname, "..");
const recordingsDir = path.join(root, "recordings");
const assetsDir = path.join(root, "public", "assets");

const viewport = { width: 1920, height: 1080 };

const clips = [
  {
    name: "repo",
    url: "https://example.invalid/miden-testnet-bridge",
    description: "GitHub repository overview",
    steps: async (page) => {
      await page.waitForTimeout(1200);
      await page.mouse.wheel(0, 550);
      await page.waitForTimeout(900);
      await page.mouse.wheel(0, 720);
      await page.waitForTimeout(1100);
    },
  },
  {
    name: "guide",
    url: "https://example.invalid/miden-testnet-bridge/blob/main/docs/builder-testing-guide.md",
    description: "Sepolia-first builder guide",
    steps: async (page) => {
      await page.waitForTimeout(1200);
      await page.mouse.wheel(0, 700);
      await page.waitForTimeout(800);
      await page.mouse.wheel(0, 900);
      await page.waitForTimeout(800);
      await page.mouse.wheel(0, 1000);
      await page.waitForTimeout(900);
    },
  },
  {
    name: "evidence",
    url: "https://example.invalid/miden-testnet-bridge/smoke-test-report.html",
    description: "Published Sepolia evidence page",
    steps: async (page) => {
      await page.waitForTimeout(1200);
      await page.mouse.wheel(0, 760);
      await page.waitForTimeout(900);
      await page.mouse.wheel(0, 980);
      await page.waitForTimeout(900);
      await page.mouse.wheel(0, 1400);
      await page.waitForTimeout(1100);
    },
  },
];

await fs.rm(recordingsDir, { recursive: true, force: true });
await fs.mkdir(recordingsDir, { recursive: true });
await fs.mkdir(assetsDir, { recursive: true });

const browser = await chromium.launch({
  headless: true,
  chromiumSandbox: false,
});

const manifest = [];

for (const clip of clips) {
  const context = await browser.newContext({
    viewport,
    deviceScaleFactor: 1,
    recordVideo: {
      dir: recordingsDir,
      size: viewport,
    },
  });
  const page = await context.newPage();
  await page.goto(clip.url, { waitUntil: "networkidle", timeout: 60000 });
  await page.screenshot({
    path: path.join(assetsDir, `${clip.name}.png`),
    fullPage: false,
  });
  await clip.steps(page);
  const video = page.video();
  await page.close();
  await context.close();
  const videoPath = await video.path();
  const target = path.join(recordingsDir, `${clip.name}.webm`);
  const publicTarget = path.join(assetsDir, `${clip.name}.webm`);
  await fs.rename(videoPath, target);
  await fs.copyFile(target, publicTarget);
  manifest.push({
    name: clip.name,
    description: clip.description,
    url: clip.url,
    video: path.relative(root, target),
    publicVideo: `public/assets/${clip.name}.webm`,
    screenshot: `public/assets/${clip.name}.png`,
  });
}

await browser.close();

await fs.writeFile(
  path.join(recordingsDir, "manifest.json"),
  `${JSON.stringify(manifest, null, 2)}\n`,
);

console.log(`Recorded ${manifest.length} clips:`);
for (const clip of manifest) {
  console.log(`- ${clip.name}: ${clip.video}`);
}
