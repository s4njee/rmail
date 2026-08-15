import fs from "node:fs";
import path from "node:path";
import zlib from "node:zlib";
import { execSync } from "node:child_process";

function decodePng(buffer) {
  let pos = 8;
  let width = 0;
  let height = 0;
  let colorType = 6;
  const idatChunks = [];

  while (pos < buffer.length) {
    const len = buffer.readUInt32BE(pos);
    const type = buffer.toString("ascii", pos + 4, pos + 8);
    const data = buffer.subarray(pos + 8, pos + 8 + len);
    pos += 12 + len;
    if (type === "IHDR") {
      width = data.readUInt32BE(0);
      height = data.readUInt32BE(4);
      colorType = data[9];
    } else if (type === "IDAT") {
      idatChunks.push(data);
    }
  }

  const raw = zlib.inflateSync(Buffer.concat(idatChunks));
  const bpp =
    colorType === 6 ? 4 : colorType === 2 ? 3 : colorType === 0 ? 1 : 4;
  const rowSize = 1 + width * bpp;
  const pixels = Buffer.alloc(width * height * 4);
  let prevRow = Buffer.alloc(width * bpp);
  let currRow = Buffer.alloc(width * bpp);

  for (let y = 0; y < height; y++) {
    const filter = raw[y * rowSize];
    const rowData = raw.subarray(y * rowSize + 1, (y + 1) * rowSize);
    for (let x = 0; x < width * bpp; x++) {
      const byte = rowData[x];
      const left = x >= bpp ? currRow[x - bpp] : 0;
      const up = prevRow[x];
      const upLeft = x >= bpp ? prevRow[x - bpp] : 0;
      if (filter === 0) currRow[x] = byte;
      else if (filter === 1) currRow[x] = (byte + left) & 0xff;
      else if (filter === 2) currRow[x] = (byte + up) & 0xff;
      else if (filter === 3)
        currRow[x] = (byte + Math.floor((left + up) / 2)) & 0xff;
      else if (filter === 4) {
        const p = left + up - upLeft;
        const pa = Math.abs(p - left);
        const pb = Math.abs(p - up);
        const pc = Math.abs(p - upLeft);
        let pr;
        if (pa <= pb && pa <= pc) pr = left;
        else if (pb <= pc) pr = up;
        else pr = upLeft;
        currRow[x] = (byte + pr) & 0xff;
      }
    }
    for (let x = 0; x < width; x++) {
      const outIdx = (y * width + x) * 4;
      if (colorType === 6) {
        pixels[outIdx] = currRow[x * 4];
        pixels[outIdx + 1] = currRow[x * 4 + 1];
        pixels[outIdx + 2] = currRow[x * 4 + 2];
        pixels[outIdx + 3] = currRow[x * 4 + 3];
      } else if (colorType === 2) {
        pixels[outIdx] = currRow[x * 3];
        pixels[outIdx + 1] = currRow[x * 3 + 1];
        pixels[outIdx + 2] = currRow[x * 3 + 2];
        pixels[outIdx + 3] = 255;
      }
    }
    prevRow.set(currRow);
  }
  return { width, height, pixels };
}

function crc32(buf) {
  let crc = -1;
  for (let i = 0; i < buf.length; i++) {
    crc ^= buf[i];
    for (let j = 0; j < 8; j++) crc = (crc >>> 1) ^ (-(crc & 1) & 0xedb88320);
  }
  return (crc ^ -1) >>> 0;
}

function createChunk(type, data) {
  const len = data.length;
  const buf = Buffer.alloc(12 + len);
  buf.writeUInt32BE(len, 0);
  buf.write(type, 4, 4, "ascii");
  data.copy(buf, 8);
  const typeAndData = buf.subarray(4, 8 + len);
  buf.writeUInt32BE(crc32(typeAndData), 8 + len);
  return buf;
}

function encodePng(width, height, rgbaPixels) {
  const rowSize = 1 + width * 4;
  const raw = Buffer.alloc(height * rowSize);
  for (let y = 0; y < height; y++) {
    raw[y * rowSize] = 0;
    for (let x = 0; x < width; x++) {
      const srcIdx = (y * width + x) * 4;
      const dstIdx = y * rowSize + 1 + x * 4;
      raw[dstIdx] = rgbaPixels[srcIdx];
      raw[dstIdx + 1] = rgbaPixels[srcIdx + 1];
      raw[dstIdx + 2] = rgbaPixels[srcIdx + 2];
      raw[dstIdx + 3] = rgbaPixels[srcIdx + 3];
    }
  }

  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(width, 0);
  ihdr.writeUInt32BE(height, 4);
  ihdr[8] = 8;
  ihdr[9] = 6;
  ihdr[10] = 0;
  ihdr[11] = 0;
  ihdr[12] = 0;

  const idat = zlib.deflateSync(raw, { level: 9 });
  const header = Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);
  return Buffer.concat([
    header,
    createChunk("IHDR", ihdr),
    createChunk("IDAT", idat),
    createChunk("IEND", Buffer.alloc(0)),
  ]);
}

function blurMask(src, w, h, radius) {
  const temp = new Float32Array(w * h);
  const dst = new Float32Array(w * h);
  const r = Math.round(radius);
  const weight = 1 / (2 * r + 1);

  for (let y = 0; y < h; y++) {
    let sum = 0;
    for (let x = -r; x <= r; x++) {
      const sx = Math.max(0, Math.min(w - 1, x));
      sum += src[y * w + sx];
    }
    for (let x = 0; x < w; x++) {
      temp[y * w + x] = sum * weight;
      const xRemove = Math.max(0, x - r);
      const xAdd = Math.min(w - 1, x + r + 1);
      sum += src[y * w + xAdd] - src[y * w + xRemove];
    }
  }

  for (let x = 0; x < w; x++) {
    let sum = 0;
    for (let y = -r; y <= r; y++) {
      const sy = Math.max(0, Math.min(h - 1, y));
      sum += temp[sy * w + x];
    }
    for (let y = 0; y < h; y++) {
      dst[y * w + x] = sum * weight;
      const yRemove = Math.max(0, y - r);
      const yAdd = Math.min(h - 1, y + r + 1);
      sum += temp[yAdd * w + x] - temp[yRemove * w + x];
    }
  }
  return dst;
}

export function generateAppIcon() {
  const rootDir = process.cwd();
  const inputJpg = path.join(rootDir, "appicon.jpg");
  const tempPng = path.join(rootDir, "target", "temp_appicon_crop.png");
  fs.mkdirSync(path.join(rootDir, "target"), { recursive: true });

  // Convert and center-crop to square using sips
  execSync(`sips -s format png -c 1152 1152 "${inputJpg}" --out "${tempPng}"`, {
    stdio: "inherit",
  });

  const { width, height, pixels } = decodePng(fs.readFileSync(tempPng));

  const targetCx = 512,
    targetCy = 500;
  const scale = 824 / 724;
  const targetHw = (724 * scale) / 2;
  const targetHh = (721 * scale) / 2;
  const targetR = 158 * scale;

  function targetSdf(x, y) {
    const qx = Math.abs(x - targetCx) - (targetHw - targetR);
    const qy = Math.abs(y - targetCy) - (targetHh - targetR);
    const dx = Math.max(0, qx);
    const dy = Math.max(0, qy);
    return (
      Math.sqrt(dx * dx + dy * dy) + Math.min(0, Math.max(qx, qy)) - targetR
    );
  }

  const mask = new Float32Array(1024 * 1024);
  for (let y = 0; y < 1024; y++) {
    for (let x = 0; x < 1024; x++) {
      const d = targetSdf(x + 0.5, y + 0.5);
      mask[y * 1024 + x] = Math.max(0, Math.min(1, 0.5 - d));
    }
  }

  let shadow1 = blurMask(mask, 1024, 1024, 16);
  shadow1 = blurMask(shadow1, 1024, 1024, 16);
  let shadow2 = blurMask(mask, 1024, 1024, 32);
  shadow2 = blurMask(shadow2, 1024, 1024, 32);

  const shadowAlpha = new Float32Array(1024 * 1024);
  for (let y = 0; y < 1024; y++) {
    for (let x = 0; x < 1024; x++) {
      const sy1 = y - 22;
      const sy2 = y - 10;
      const val1 = sy1 >= 0 && sy1 < 1024 ? shadow1[sy1 * 1024 + x] * 0.28 : 0;
      const val2 = sy2 >= 0 && sy2 < 1024 ? shadow2[sy2 * 1024 + x] * 0.18 : 0;
      shadowAlpha[y * 1024 + x] = Math.min(1, val1 + val2);
    }
  }

  const origCx = 576.5,
    origCy = 568.5;
  const out1024 = Buffer.alloc(1024 * 1024 * 4);

  for (let y = 0; y < 1024; y++) {
    for (let x = 0; x < 1024; x++) {
      const idx = (y * 1024 + x) * 4;
      const m = mask[y * 1024 + x];
      const s = shadowAlpha[y * 1024 + x];

      const ox = origCx + (x - targetCx) / scale;
      const oy = origCy + (y - targetCy) / scale;

      let r = 0,
        g = 0,
        b = 0;
      if (ox >= 0 && ox < width - 1 && oy >= 0 && oy < height - 1) {
        const x0 = Math.floor(ox),
          x1 = x0 + 1;
        const y0 = Math.floor(oy),
          y1 = y0 + 1;
        const wx1 = ox - x0,
          wx0 = 1 - wx1;
        const wy1 = oy - y0,
          wy0 = 1 - wy1;

        const idx00 = (y0 * width + x0) * 4;
        const idx10 = (y0 * width + x1) * 4;
        const idx01 = (y1 * width + x0) * 4;
        const idx11 = (y1 * width + x1) * 4;

        r =
          pixels[idx00] * wx0 * wy0 +
          pixels[idx10] * wx1 * wy0 +
          pixels[idx01] * wx0 * wy1 +
          pixels[idx11] * wx1 * wy1;
        g =
          pixels[idx00 + 1] * wx0 * wy0 +
          pixels[idx10 + 1] * wx1 * wy0 +
          pixels[idx01 + 1] * wx0 * wy1 +
          pixels[idx11 + 1] * wx1 * wy1;
        b =
          pixels[idx00 + 2] * wx0 * wy0 +
          pixels[idx10 + 2] * wx1 * wy0 +
          pixels[idx01 + 2] * wx0 * wy1 +
          pixels[idx11 + 2] * wx1 * wy1;
      }

      if (m > 0.001) {
        const finalA = m + (1 - m) * s;
        out1024[idx] = Math.round(r * m);
        out1024[idx + 1] = Math.round(g * m);
        out1024[idx + 2] = Math.round(b * m);
        out1024[idx + 3] = Math.round(finalA * 255);
      } else if (s > 0.001) {
        out1024[idx] = 0;
        out1024[idx + 1] = 0;
        out1024[idx + 2] = 0;
        out1024[idx + 3] = Math.round(s * 255);
      }
    }
  }

  const masterPngPath = path.join(rootDir, "target", "master_app_icon.png");
  fs.writeFileSync(masterPngPath, encodePng(1024, 1024, out1024));

  // Run tauri icon generator to produce all bundle icons
  execSync(`pnpm tauri icon "${masterPngPath}" -o src-tauri/icons`, {
    stdio: "inherit",
  });

  // Copy favicon to public/ for browser and webview
  fs.copyFileSync(
    path.join(rootDir, "src-tauri", "icons", "32x32.png"),
    path.join(rootDir, "public", "favicon.png"),
  );

  console.log("Successfully generated all Tauri app icons from appicon.jpg!");
}

if (process.argv[1] && process.argv[1].endsWith("generate-app-icons.mjs")) {
  generateAppIcon();
}
