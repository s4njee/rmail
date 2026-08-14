import { spawn } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const screenshotsDir = path.resolve(__dirname, "../docs/screenshots");
fs.mkdirSync(screenshotsDir, { recursive: true });

async function sleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

let cdpId = 1;
async function sendCdp(ws, method, params = {}) {
  const id = cdpId++;
  return new Promise((resolve, reject) => {
    const handler = (event) => {
      const data = JSON.parse(event.data);
      if (data.id === id) {
        ws.removeEventListener("message", handler);
        if (data.error) reject(data.error);
        else resolve(data.result);
      }
    };
    ws.addEventListener("message", handler);
    ws.send(JSON.stringify({ id, method, params }));
  });
}

async function evalInPage(ws, fnBody) {
  const expression = `(() => { ${fnBody} })()`;
  const res = await sendCdp(ws, "Runtime.evaluate", {
    expression,
    awaitPromise: true,
    returnByValue: true,
  });
  if (res.exceptionDetails) {
    console.error("Eval Exception:", res.exceptionDetails);
  }
  return res.result?.value;
}

async function takeScreenshot(ws, filename) {
  const res = await sendCdp(ws, "Page.captureScreenshot", {
    format: "png",
    fromSurface: true,
  });
  const filePath = path.join(screenshotsDir, filename);
  fs.writeFileSync(filePath, Buffer.from(res.data, "base64"));
  const stats = fs.statSync(filePath);
  console.log(`Saved screenshot: ${filename} (${stats.size} bytes)`);
}

async function main() {
  console.log("Starting vite dev server on 5173...");
  const vite = spawn("npx", ["vite", "--port", "5173", "--strictPort"], {
    cwd: path.resolve(__dirname, ".."),
    stdio: "pipe",
  });

  await sleep(2000);

  const tmpProfile = fs.mkdtempSync(path.join(path.resolve(__dirname, "../scratch"), "chrome-"));

  console.log("Starting headless Chrome...");
  const chromePath = "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";
  const chrome = spawn(chromePath, [
    "--headless=new",
    "--remote-debugging-port=9222",
    `--user-data-dir=${tmpProfile}`,
    "--window-size=1280,820",
    "--hide-scrollbars",
    "--disable-gpu",
    "--disable-extensions",
    "--no-first-run",
    "--no-default-browser-check",
    "about:blank",
  ]);

  await sleep(2000);

  try {
    const targetsRes = await fetch("http://localhost:9222/json");
    const targets = await targetsRes.json();
    const pageTarget = targets.find((t) => t.type === "page");
    if (!pageTarget || !pageTarget.webSocketDebuggerUrl) {
      throw new Error("No Chrome page target found");
    }

    console.log("Connecting to Chrome CDP...");
    const ws = new WebSocket(pageTarget.webSocketDebuggerUrl);
    await new Promise((resolve) => ws.addEventListener("open", resolve));

    await sendCdp(ws, "Page.enable");
    await sendCdp(ws, "Runtime.enable");
    await sendCdp(ws, "Emulation.setDeviceMetricsOverride", {
      width: 1280,
      height: 820,
      deviceScaleFactor: 2,
      mobile: false,
    });

    console.log("Navigating to http://localhost:5173...");
    await sendCdp(ws, "Page.navigate", { url: "http://localhost:5173" });
    await sleep(2500);

    // 1. Mail Inbox View (Hairline Theme)
    console.log("Capturing 01-mail-inbox.png...");
    await evalInPage(ws, `
      const mailTab = Array.from(document.querySelectorAll('.sidebar__section-tab')).find(el => el.textContent.includes('Mail'));
      if (mailTab) mailTab.click();
      const firstRow = document.querySelector('.message-row');
      if (firstRow) firstRow.click();
    `);
    await sleep(800);
    await takeScreenshot(ws, "01-mail-inbox.png");

    // 2. Calendar Month View
    console.log("Capturing 02-calendar-month.png...");
    await evalInPage(ws, `
      const calTab = Array.from(document.querySelectorAll('.sidebar__section-tab')).find(el => el.textContent.includes('Calendar'));
      if (calTab) calTab.click();
      const viewBtns = document.querySelectorAll('.view-segment-btn');
      if (viewBtns[0]) viewBtns[0].click();
    `);
    await sleep(800);
    await takeScreenshot(ws, "02-calendar-month.png");

    // 3. Calendar Week View
    console.log("Capturing 03-calendar-week.png...");
    await evalInPage(ws, `
      const viewBtns = document.querySelectorAll('.view-segment-btn');
      if (viewBtns[1]) viewBtns[1].click();
    `);
    await sleep(800);
    await takeScreenshot(ws, "03-calendar-week.png");

    // 4. Calendar 3-Day View
    console.log("Capturing 04-calendar-3day.png...");
    await evalInPage(ws, `
      const viewBtns = document.querySelectorAll('.view-segment-btn');
      if (viewBtns[2]) viewBtns[2].click();
    `);
    await sleep(800);
    await takeScreenshot(ws, "04-calendar-3day.png");

    // 5. Calendar Day View
    console.log("Capturing 05-calendar-day.png...");
    await evalInPage(ws, `
      const viewBtns = document.querySelectorAll('.view-segment-btn');
      if (viewBtns[3]) viewBtns[3].click();
    `);
    await sleep(800);
    await takeScreenshot(ws, "05-calendar-day.png");

    // 6. Calendar Agenda View
    console.log("Capturing 06-calendar-agenda.png...");
    await evalInPage(ws, `
      const viewBtns = document.querySelectorAll('.view-segment-btn');
      if (viewBtns[4]) viewBtns[4].click();
    `);
    await sleep(800);
    await takeScreenshot(ws, "06-calendar-agenda.png");

    // 7. Calendar Event Editor Modal
    console.log("Capturing 07-calendar-event-modal.png...");
    await evalInPage(ws, `
      const viewBtns = document.querySelectorAll('.view-segment-btn');
      if (viewBtns[0]) viewBtns[0].click();
      const newEvtBtn = document.querySelector('.quill-cal-btn--primary');
      if (newEvtBtn) newEvtBtn.click();
    `);
    await sleep(800);
    await takeScreenshot(ws, "07-calendar-event-modal.png");

    // Close modal
    await evalInPage(ws, `
      const closeBtn = document.querySelector('.modal-close, button[aria-label="Close"], .modal-overlay button');
      if (closeBtn) closeBtn.click();
      else window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));
    `);
    await sleep(500);

    // 8. Settings - Calendar Tab
    console.log("Capturing 08-settings-calendar.png...");
    await evalInPage(ws, `
      window.dispatchEvent(new KeyboardEvent('keydown', { key: ',', metaKey: true }));
    `);
    await sleep(600);
    await evalInPage(ws, `
      const navItems = Array.from(document.querySelectorAll('.settings-nav button, .settings-tab, button'));
      const calNav = navItems.find(el => el.textContent && el.textContent.trim() === 'Calendar');
      if (calNav) calNav.click();
    `);
    await sleep(600);
    await takeScreenshot(ws, "08-settings-calendar.png");

    // 9. Settings - Accounts Tab
    console.log("Capturing 09-settings-accounts.png...");
    await evalInPage(ws, `
      const navItems = Array.from(document.querySelectorAll('.settings-nav button, .settings-tab, button'));
      const accNav = navItems.find(el => el.textContent && el.textContent.trim() === 'Accounts');
      if (accNav) accNav.click();
    `);
    await sleep(600);
    await takeScreenshot(ws, "09-settings-accounts.png");

    // 10. Banded Theme (Mail)
    console.log("Capturing 10-mail-banded-theme.png...");
    await evalInPage(ws, `
      window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));
      const mailTab = Array.from(document.querySelectorAll('.sidebar__section-tab')).find(el => el.textContent.includes('Mail'));
      if (mailTab) mailTab.click();
      document.querySelector('.app')?.setAttribute('data-theme', 'banded');
      document.body.setAttribute('data-theme', 'banded');
    `);
    await sleep(600);
    await takeScreenshot(ws, "10-mail-banded-theme.png");

    ws.close();
    console.log("All screenshots captured successfully!");
  } finally {
    chrome.kill();
    vite.kill();
  }
}

main().catch((err) => {
  console.error("Screenshot capture failed:", err);
  process.exit(1);
});
