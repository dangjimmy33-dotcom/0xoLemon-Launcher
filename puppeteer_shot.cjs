const puppeteer = require('puppeteer');
(async () => {
  const browser = await puppeteer.launch();
  const page = await browser.newPage();
  await page.setViewport({width: 1280, height: 720});
  
  page.on('console', msg => console.log('PAGE LOG:', msg.text()));
  page.on('pageerror', err => console.log('PAGE ERROR:', err.message, err.stack));
  page.on('requestfailed', request => console.log('REQUEST FAILED:', request.url(), request.failure().errorText));

  try {
    await page.goto('http://localhost:1420', {waitUntil: 'domcontentloaded'});
    await new Promise(r => setTimeout(r, 8000));
    await page.screenshot({path: 'E:/007Launcher/screenshot.png'});
  } catch(e) {
    console.error('Puppeteer error:', e);
  }
  await browser.close();
})();
