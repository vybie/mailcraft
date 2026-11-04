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
    await page.setContent(`${Array.from({length: 65}, (_, i) => `<img width="10" height="10" src="https://mailcraft.invalid/reading/asset/1/${i}">`).join('')}
      <img width="30" height="30" src="https://image.example/fragment#section">
      <img width="1" height="1" src="https://image.example/explicit-tracker">`, { waitUntil: 'load' });
    const later = await page.evaluate(script);
    assert.deepEqual(later.images.map(image => image.url), ['https://image.example/fragment']);
    await page.setContent(`
      <img src="https://image.example/collapsed" alt="">
      <img src="https://image.example/with-alt" alt="Picture">
      <img src="https://image.example/zero" alt="" style="width:0;height:0">
      <img src="https://image.example/tiny-attr" alt="" width="1" height="1">
      <div style="display:none"><img src="https://image.example/hidden" alt=""></div>
      <div style="width:20px;height:20px;overflow:hidden"><img src="https://image.example/clipped" alt="" style="position:relative;left:100px"></div>
      <img src="https://image.example/clip-path-hidden" style="width:40px;height:40px;clip-path:inset(50%)">
      <div style="clip-path:inset(50%)"><img src="https://image.example/ancestor-hidden" style="width:40px;height:40px"></div>
      <img src="https://image.example/cropped" style="width:40px;height:40px;clip-path:inset(5%)">
    `, { waitUntil: 'load' });
    const uncertain = await page.evaluate(script);
    const found = uncertain.images.map(image => image.url);
    assert(found.includes('https://image.example/collapsed'));
    assert(found.includes('https://image.example/with-alt'));
    assert(found.includes('https://image.example/cropped'));
    for (const suffix of ['zero', 'tiny-attr', 'hidden', 'clipped', 'clip-path-hidden', 'ancestor-hidden']) assert(!found.includes(`https://image.example/${suffix}`));
    await page.setContent(`${Array.from({length: 60}, (_, i) => `<img alt="" src="https://image.example/collapsed-${i}">`).join('')}
      <img width="40" height="40" src="https://image.example/already-supported">`, { waitUntil: 'load' });
    const crowded = await page.evaluate(script);
    assert.equal(crowded.images.length, 60);
    assert(crowded.images.some(image => image.url === 'https://image.example/already-supported'));
    console.log('PASS: tiny background layers rejected; ordinary layers, commas and images retained; requests blocked');
    await context.close();
  } finally {
    await browser.close();
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
