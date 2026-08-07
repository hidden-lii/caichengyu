/**
 * 从汉文学网爬虫 JSONL 生成仅四字成语的精简词库 JSON。
 *
 * 用法:
 *   node scripts/build-hwxnet-lexicon.mjs [input.jsonl] [output.json]
 */
import fs from 'fs';
import path from 'path';
import readline from 'readline';
import { fileURLToPath } from 'url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(__dirname, '..');

const inputPath =
  process.argv[2] ||
  path.resolve(root, '..', 'pachong', 'data', 'idioms.jsonl');
const outputPath =
  process.argv[3] ||
  path.join(root, 'src-tauri', 'resources', 'idioms_hwxnet.json');

function charLen(s) {
  return [...s].length;
}

async function main() {
  if (!fs.existsSync(inputPath)) {
    console.error(`输入文件不存在: ${inputPath}`);
    process.exit(1);
  }

  const out = [];
  let total = 0;
  let kept = 0;
  let skippedLen = 0;
  let skippedInvalid = 0;

  const rl = readline.createInterface({
    input: fs.createReadStream(inputPath, { encoding: 'utf8' }),
    crlfDelay: Infinity,
  });

  for await (const line of rl) {
    const trimmed = line.trim();
    if (!trimmed) continue;
    total += 1;
    let obj;
    try {
      obj = JSON.parse(trimmed);
    } catch {
      skippedInvalid += 1;
      continue;
    }
    const word = String(obj.word || '').trim();
    const pinyin = String(obj.pinyin || '').trim();
    if (!word || !pinyin) {
      skippedInvalid += 1;
      continue;
    }
    if (charLen(word) !== 4) {
      skippedLen += 1;
      continue;
    }
    const explanation = obj.explanation == null ? '' : String(obj.explanation);
    out.push({ word, pinyin, explanation });
    kept += 1;
  }

  fs.mkdirSync(path.dirname(outputPath), { recursive: true });
  fs.writeFileSync(outputPath, JSON.stringify(out), 'utf8');

  const sizeMb = (fs.statSync(outputPath).size / (1024 * 1024)).toFixed(2);
  console.log(
    `done: total=${total} kept=${kept} skip_len=${skippedLen} skip_invalid=${skippedInvalid}`,
  );
  console.log(`wrote ${outputPath} (${sizeMb} MB)`);
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
