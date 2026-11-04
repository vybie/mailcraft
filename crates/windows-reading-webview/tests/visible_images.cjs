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
      <div style="width:100px;height:100px;background-image:url(https://image.example/auto-bg);background-size:auto;background-repeat:no-repeat"></div>
      <div style="width:100px;height:100px;background-image:url(https://image.example/contain-bg);background-size:contain"></div>
      <div style="width:100px;height:100px;clip-path:inset(50%);background-image:url(https://image.example/clipped-bg)"></div>
      <div style="clip-path:inset(50%)"><div style="width:100px;height:100px;background-image:url(https://image.example/clipped-parent-bg)"></div></div>
      <div style="filter:opacity(0)"><div style="width:100px;height:100px;background-image:url(https://image.example/filter-bg)"></div></div>
      <div style="position:absolute;left:-9999px;width:100px;height:100px;background-image:url(https://image.example/offscreen-bg)"></div>
      <img src="https://image.example/photo" style="width:30px;height:30px">
      <img src="https://image.example/tracker" width="1" style="width:30px;height:30px">
      <img src="https://image.example/unknown">
      <img srcset="https://image.example/unknown-srcset 1x">
      <img src="https://image.example/zero" style="width:0;height:0">
      <style>.css-zero { width:calc(0px); height:calc(0px) }</style><img class="css-zero" src="https://image.example/css-zero">
      <div style="display:none"><img src="https://image.example/hidden-unknown"></div>
      <div style="position:relative;width:80px;height:80px;overflow:hidden"><img style="position:absolute;left:300px" src="https://image.example/clipped-unknown"></div>
      <img src="https://image.example/offscreen-unknown" style="position:absolute;left:-9999px">
      <img src="https://image.example/clip-unknown" style="position:absolute;clip:rect(0px,0px,0px,0px)">
      <div style="filter:opacity(0)"><img src="https://image.example/filter-unknown"></div>
    `, { waitUntil: 'load' });
    const script = fs.readFileSync(path.join(__dirname, '..', 'src', 'visible_images.js'), 'utf8');
    const result = await page.evaluate(script);
    const urls = result.images.map(image => image.url);
    for (const suffix of ['big,comma', 'big', 'photo']) assert(urls.includes(`https://image.example/${suffix}`), `missing ${suffix}`);
    for (const suffix of ['tiny', 'tiny-height', 'tiny-percent', 'tiny-layer', 'tracker']) assert(!urls.includes(`https://image.example/${suffix}`), `included ${suffix}`);
    assert(result.images.some(image => image.url === 'https://image.example/unknown' && image.kind === 'unknown'));
    assert(result.images.some(image => image.url === 'https://image.example/unknown-srcset' && image.kind === 'unknown'));
    for (const suffix of ['auto-bg','contain-bg']) assert(result.images.some(image => image.url === `https://image.example/${suffix}` && image.kind === 'unknown'));
    for (const suffix of ['zero', 'css-zero', 'hidden-unknown', 'clipped-unknown', 'offscreen-unknown', 'clip-unknown', 'filter-unknown', 'clipped-bg', 'clipped-parent-bg', 'filter-bg', 'offscreen-bg']) assert(!urls.includes(`https://image.example/${suffix}`), `included ${suffix}`);
    await page.setContent(Array.from({length: 65}, (_,i) => `<img width="40" height="40" src="https://mailcraft.invalid/reading/asset/1/${i}">`).join('') + '<img src="https://image.example/after-local">', {waitUntil:'load'});
    const afterLocal = await page.evaluate(script);
    assert(afterLocal.images.some(image => image.url === 'https://image.example/after-local' && image.kind === 'unknown'), 'embedded assets consumed remote discovery budget');
    assert.equal(await page.evaluate('window.senderRan'), undefined);
    assert(blocked > 0);
    console.log('PASS: tiny background layers rejected; ordinary layers, commas and images retained; requests blocked');
    await context.close();
  } finally {
    await browser.close();
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
