// Photograph local previews in each theme with a headless browser, to look at the port before it
// is published. Pictures go to _agent_output, never into the repository.
//
//   node tools/shoot.cjs <out folder> <html file> [more html files…]
//
// Each file is taken in the four themes and written as <name>--<theme>.png. Console errors are printed.

const path = require('node:path');
const fs = require('node:fs');
const { chromium } = require(process.env.PLAYWRIGHT_PATH || 'C:/jason/dev/ai-service/ui/node_modules/playwright');

const [out, ...files] = process.argv.slice(2);
const themes = (process.env.THEMES || 'light,dark,unluminous-dark,unluminous-light').split(',');
const width = Number(process.env.WIDTH || 1100);

(async () => {
  fs.mkdirSync(out, { recursive: true });
  const browser = await chromium.launch();
  const page = await browser.newPage({ viewport: { width, height: 400 } });
  page.on('console', (m) => m.type() === 'error' && console.log(`  console: ${m.text()}`));
  page.on('pageerror', (e) => console.log(`  error: ${e.message}`));
  for (const file of files) {
    const name = path.basename(file).replace(/\.html$/, '');
    for (const theme of themes) {
      await page.goto('file:///' + path.resolve(file).replace(/\\/g, '/'));
      await page.evaluate((t) => { document.documentElement.dataset.theme = t; }, theme);
      await page.waitForTimeout(150);
      await page.screenshot({ path: path.join(out, `${name}--${theme}.png`), fullPage: true });
    }
    console.log(`shot ${name}`);
  }
  await browser.close();
})();
