import type { IdiomEntry, IdiomInput, IndexedIdiom } from '../entity/idiom';
import { normalizeCorpus, normalizeEntry, parseSyllable } from './pinyin';

export function buildIndex(entries: IdiomInput[]): { result: IndexedIdiom[]; skipped: number } {
  const result: IndexedIdiom[] = [];
  let skipped = 0;
  for (const entry of entries) {
    const chars = [...entry.word];
    const syls = entry.pinyin.split(/\s+/).filter(Boolean);
    if (chars.length !== syls.length) {
      skipped++;
      continue;
    }
    const charData = chars.map((c, i) => {
      const p = parseSyllable(syls[i]);
      return { c, ...p };
    });
    result.push({
      word: entry.word,
      pinyin: entry.pinyin,
      explanation: entry.explanation || '',
      chars: charData,
    });
  }
  return { result, skipped };
}

export function buildIndexFromDb(items: IdiomEntry[]): { result: IndexedIdiom[]; skipped: number } {
  const result: IndexedIdiom[] = [];
  let skipped = 0;
  for (const item of items) {
    if (item.chars?.length) {
      result.push({
        id: item.id,
        word: item.word,
        pinyin: item.pinyin,
        explanation: item.explanation || '',
        chars: item.chars,
        blind_eligible: item.blind_eligible,
      });
      continue;
    }
    const normalized = normalizeEntry(item);
    if (!normalized) {
      skipped++;
      continue;
    }
    const built = buildIndex([normalized]);
    if (built.result.length) result.push(...built.result);
    else skipped++;
  }
  return { result, skipped };
}

/** 分块建索引，中间让出主线程，避免转圈动画卡死 */
export async function buildIndexFromDbChunked(
  items: IdiomEntry[],
  onProgress?: (done: number, total: number) => void | Promise<void>,
  chunkSize = 800
): Promise<{ result: IndexedIdiom[]; skipped: number }> {
  const result: IndexedIdiom[] = [];
  let skipped = 0;
  const total = items.length;
  for (let i = 0; i < total; i++) {
    const item = items[i];
    if (item.chars?.length) {
      result.push({
        id: item.id,
        word: item.word,
        pinyin: item.pinyin,
        explanation: item.explanation || '',
        chars: item.chars,
        blind_eligible: item.blind_eligible,
      });
    } else {
      const normalized = normalizeEntry(item);
      if (!normalized) {
        skipped++;
      } else {
        const built = buildIndex([normalized]);
        if (built.result.length) result.push(...built.result);
        else skipped++;
      }
    }
    if ((i + 1) % chunkSize === 0 || i + 1 === total) {
      await onProgress?.(i + 1, total);
    }
  }
  return { result, skipped };
}

function allDistinct<T>(values: T[]): boolean {
  return new Set(values).size === values.length;
}

export function hasDistinctPhonetics(item: IndexedIdiom): boolean {
  const chars = item.chars;
  if (!chars.length) return false;
  return (
    allDistinct(chars.map((ch) => ch.c)) &&
    allDistinct(chars.map((ch) => ch.sm || '')) &&
    allDistinct(chars.map((ch) => ch.ym)) &&
    allDistinct(chars.map((ch) => ch.tone))
  );
}

export function getBlindPool(index: IndexedIdiom[]): IndexedIdiom[] {
  return index.filter(hasDistinctPhonetics);
}

export function buildWordMap(index: IndexedIdiom[]): Map<string, IndexedIdiom> {
  return new Map(index.map((item) => [item.word, item]));
}

export function charToQueryToken(ch: { py: string; sm: string; ym: string; tone: number }): string {
  const tone = ch.tone >= 1 && ch.tone <= 4 ? ch.tone : '';
  if (ch.sm) return tone ? `${ch.sm}?${tone}` : `${ch.sm}?`;
  if (ch.ym) return tone ? `?${ch.ym}${tone}` : `?${ch.ym}`;
  return tone ? `${ch.py}${tone}` : ch.py;
}

export function buildQueryFromIdiom(item: IndexedIdiom): string {
  return item.chars.map(charToQueryToken).join(' ');
}

export function setCorpusFromInputs(data: unknown): {
  index: IndexedIdiom[];
  skipped: number;
  wordMap: Map<string, IndexedIdiom>;
} {
  const normalized = normalizeCorpus(data);
  const { result, skipped } = buildIndex(normalized);
  return { index: result, skipped, wordMap: buildWordMap(result) };
}
