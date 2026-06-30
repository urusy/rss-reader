// PWA アイコン生成（依存なし）。public/favicon.svg と同じ RSS マークを数式で
// ラスタライズし、icon-192 / icon-512 / icon-maskable(512) / apple-touch-icon(180) を出力する。
// 実行: node frontend/scripts/gen-icons.mjs
import { deflateSync } from "node:zlib";
import { writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const OUT = join(dirname(fileURLToPath(import.meta.url)), "..", "public");

// 色
const BG = [24, 24, 27]; // #18181b
const FG = [255, 255, 255];

// マーク幾何（512 基準。favicon.svg と一致）。原点(放射点)=(152,360)。
const ORIGIN = [152, 360];
const DOT_R = 34;
const STROKE_HALF = 23; // stroke-width 46 の半分
const ARCS = [
  { r: 120, p1: [152, 240], p2: [272, 360] },
  { r: 224, p1: [152, 136], p2: [376, 360] },
];

const hyp = (ax, ay, bx, by) => Math.hypot(ax - bx, ay - by);

// (px,py) を 512 基準・中心スケール k で評価し、マーク内なら true。
function isMark(px, py, k) {
  const bx = (px - 256) / k + 256;
  const by = (py - 256) / k + 256;
  const d = hyp(bx, by, ORIGIN[0], ORIGIN[1]);
  if (d <= DOT_R) return true;
  const inQuadrant = bx >= ORIGIN[0] && by <= ORIGIN[1]; // 北→東の四半円
  for (const a of ARCS) {
    if (inQuadrant && Math.abs(d - a.r) <= STROKE_HALF) return true;
    if (hyp(bx, by, a.p1[0], a.p1[1]) <= STROKE_HALF) return true; // 端の丸キャップ
    if (hyp(bx, by, a.p2[0], a.p2[1]) <= STROKE_HALF) return true;
  }
  return false;
}

// CRC32（PNG チャンク用）
const CRC_TABLE = (() => {
  const t = new Uint32Array(256);
  for (let n = 0; n < 256; n++) {
    let c = n;
    for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    t[n] = c >>> 0;
  }
  return t;
})();
function crc32(buf) {
  let c = 0xffffffff;
  for (let i = 0; i < buf.length; i++) c = CRC_TABLE[(c ^ buf[i]) & 0xff] ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
}
function chunk(type, data) {
  const len = Buffer.alloc(4);
  len.writeUInt32BE(data.length, 0);
  const typeBuf = Buffer.from(type, "ascii");
  const body = Buffer.concat([typeBuf, data]);
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(body), 0);
  return Buffer.concat([len, body, crc]);
}
function encodePng(size, rgba) {
  const sig = Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]);
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(size, 0);
  ihdr.writeUInt32BE(size, 4);
  ihdr[8] = 8; // bit depth
  ihdr[9] = 6; // color type RGBA
  // 残りは 0（圧縮/フィルタ/インタレース）
  const stride = size * 4;
  const raw = Buffer.alloc((stride + 1) * size);
  for (let y = 0; y < size; y++) {
    raw[y * (stride + 1)] = 0; // filter: none
    rgba.copy(raw, y * (stride + 1) + 1, y * stride, y * stride + stride);
  }
  const idat = deflateSync(raw, { level: 9 });
  return Buffer.concat([
    sig,
    chunk("IHDR", ihdr),
    chunk("IDAT", idat),
    chunk("IEND", Buffer.alloc(0)),
  ]);
}

// size×size を 4x スーパーサンプリングで描画。k=マークの中心スケール。
function render(size, k) {
  const SS = 4;
  const rgba = Buffer.alloc(size * size * 4);
  for (let y = 0; y < size; y++) {
    for (let x = 0; x < size; x++) {
      let hit = 0;
      for (let sy = 0; sy < SS; sy++) {
        for (let sx = 0; sx < SS; sx++) {
          const px = ((x + (sx + 0.5) / SS) * 512) / size;
          const py = ((y + (sy + 0.5) / SS) * 512) / size;
          if (isMark(px, py, k)) hit++;
        }
      }
      const f = hit / (SS * SS);
      const o = (y * size + x) * 4;
      rgba[o] = Math.round(BG[0] * (1 - f) + FG[0] * f);
      rgba[o + 1] = Math.round(BG[1] * (1 - f) + FG[1] * f);
      rgba[o + 2] = Math.round(BG[2] * (1 - f) + FG[2] * f);
      rgba[o + 3] = 255;
    }
  }
  return encodePng(size, rgba);
}

const jobs = [
  ["icon-192.png", 192, 1.0],
  ["icon-512.png", 512, 1.0],
  ["icon-maskable.png", 512, 0.62], // 中央 ~62% に収め maskable セーフゾーン内に
  ["apple-touch-icon.png", 180, 1.0],
];
for (const [name, size, k] of jobs) {
  writeFileSync(join(OUT, name), render(size, k));
  console.log("wrote", name, `${size}x${size}`, "k=", k);
}
