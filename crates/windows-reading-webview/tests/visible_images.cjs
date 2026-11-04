// SPDX-FileCopyrightText: MailCraft contributors
// SPDX-License-Identifier: MIT OR Apache-2.0
// Run with MAILCRAFT_PLAYWRIGHT set to a Playwright package and, optionally,
// MAILCRAFT_BROWSER set to a Chromium executable. No external requests are allowed.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { chromium } = require(process.env.MAILCRAFT_PLAYWRIGHT || 'playwright');

(async () => {
  const browser = await chromium.launch({
    headless: true,
    executablePath: process.env.MAILCRAFT_BROWSER || undefined,
    args: ['--host-resolver-rules=MAP * ~NOTFOUND', '--proxy-server=127.0.0.1:9'],
  });
  try {
    const context = await browser.newContext({ javaScriptEnabled: false, serviceWorkers: 'block', viewport: { width: 900, height: 900 } });
    let blocked = 0;
    await context.route('**/*', route => { blocked++; return route.abort(); });
    const page = await context.newPage();
    await page.setContent(`
      <script>window.senderRan = true</script>
      <div style="width:100px;height:100px;background-image:url(https://image.example/tiny);background-size:1px 1px;background-repeat:no-repeat"></div>
      <div style="width:100px;height:100px;background-image:url(https://image.example/tiny-height);background-size:auto 2px"></div>
      <div style="width:100px;height:100px;background-image:url(https://image.example/tiny-percent);background-size:1% auto"></div>
      <div style="width:100px;height:100px;background-image:url('https://image.example/big,comma'),url(https://image.example/tiny-layer);background-size:cover,1px 1px"></div>
      <div style="width:100px;height:100px;background-image:url(https://image.example/big);background-size:50px 50px"></div>
      <img src="https://image.example/photo" style="width:30px;height:30px">
      <img src="https://image.example/tracker" width="1" style="width:30px;height:30px">
    `, { waitUntil: 'load' });
    const script = fs.readFileSync(path.join(__dirname, '..', 'src', 'visible_images.js'), 'utf8');
    const result = await page.evaluate(script);
    const urls = result.images.map(image => image.url);
    for (const suffix of ['big,comma', 'big', 'photo']) assert(urls.includes(`https://image.example/${suffix}`), `missing ${suffix}`);
    for (const suffix of ['tiny', 'tiny-height', 'tiny-percent', 'tiny-layer', 'tracker']) assert(!urls.includes(`https://image.example/${suffix}`), `included ${suffix}`);
    assert.equal(await page.evaluate('window.senderRan'), undefined);
    assert(blocked > 0);
    console.log('PASS: tiny background layers rejected; ordinary layers, commas and images retained; requests blocked');
    await context.close();
  } finally {
    await browser.close();
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
