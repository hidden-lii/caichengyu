import type { CharData, IdiomInput, MarkState } from '../entity/idiom';

export const INITIALS = [
  'zh', 'ch', 'sh', 'b', 'p', 'm', 'f', 'd', 't', 'n', 'l', 'g', 'k', 'h', 'j', 'q', 'x', 'r',
  'z', 'c', 's', 'y', 'w',
];

const TONE_CHARS: Record<string, string> = {
  'ā': 'a1', 'á': 'a2', 'ǎ': 'a3', 'à': 'a4',
  'ē': 'e1', 'é': 'e2', 'ě': 'e3', 'è': 'e4',
  'ī': 'i1', 'í': 'i2', 'ǐ': 'i3', 'ì': 'i4',
  'ō': 'o1', 'ó': 'o2', 'ǒ': 'o3', 'ò': 'o4',
  'ū': 'u1', 'ú': 'u2', 'ǔ': 'u3', 'ù': 'u4',
  'ǖ': 'v1', 'ǘ': 'v2', 'ǚ': 'v3', 'ǜ': 'v4',
};

const TONE_MARKS: Record<string, string[]> = {
  a: ['', 'ā', 'á', 'ǎ', 'à'],
  e: ['', 'ē', 'é', 'ě', 'è'],
  i: ['', 'ī', 'í', 'ǐ', 'ì'],
  o: ['', 'ō', 'ó', 'ǒ', 'ò'],
  u: ['', 'ū', 'ú', 'ǔ', 'ù'],
  v: ['', 'ǖ', 'ǘ', 'ǚ', 'ǜ'],
};

const MARKED_VOWEL = /[āáǎàēéěèīíǐìōóǒòūúǔùǖǘǚǜ]/;

function toneVowelIndex(base: string): number {
  const s = base.toLowerCase().replace(/v/g, 'ü');
  if (s.includes('a')) return s.indexOf('a');
  if (s.includes('e')) return s.indexOf('e');
  if (s.includes('ou')) return s.indexOf('o');
  if (s.includes('iu')) return s.indexOf('u');
  if (s.includes('ui')) return s.indexOf('i');
  for (let i = s.length - 1; i >= 0; i--) {
    if ('aeiouü'.includes(s[i])) return i;
  }
  return -1;
}

export function markSyllable(syl: string): string {
  const raw = syl.trim();
  if (!raw || MARKED_VOWEL.test(raw)) return raw;
  const m = raw.toLowerCase().match(/^([a-z:]+)([1-5])$/i);
  if (!m) return raw;
  let base = m[1].replace(/v/g, 'ü');
  const tone = parseInt(m[2], 10);
  if (tone === 5) return base;
  const idx = toneVowelIndex(base);
  if (idx < 0) return base;
  const ch = base[idx];
  const key = ch === 'ü' ? 'v' : ch;
  const marks = TONE_MARKS[key];
  if (!marks) return base;
  return base.slice(0, idx) + marks[tone] + base.slice(idx + 1);
}

export function normalizePinyinInput(pinyin: string): string {
  return pinyin.trim().split(/\s+/).filter(Boolean).map(markSyllable).join(' ');
}

export function normalizeEntry(item: Partial<IdiomInput>): IdiomInput | null {
  if (!item || typeof item !== 'object') return null;
  const word = String(item.word || '').trim();
  const pinyin = normalizePinyinInput(String(item.pinyin || ''));
  if (!word || !pinyin) return null;
  return {
    word,
    pinyin,
    explanation: String(item.explanation || '').trim(),
  };
}

export function normalizeCorpus(data: unknown): IdiomInput[] {
  if (!Array.isArray(data)) throw new Error('词库必须是 JSON 数组');
  const out: IdiomInput[] = [];
  for (const item of data) {
    const e = normalizeEntry(item as Partial<IdiomInput>);
    if (e) out.push(e);
  }
  return out;
}

function stripTone(syl: string): { base: string; tone: number } {
  let s = syl.toLowerCase().replace(/ü/g, 'v');
  let tone = 0;
  const digit = s.match(/([1-5])$/);
  if (digit) {
    tone = parseInt(digit[1], 10);
    s = s.slice(0, -1);
  }
  for (const [ch, rep] of Object.entries(TONE_CHARS)) {
    if (s.includes(ch)) {
      const m = rep.match(/(.)([1-5])?/);
      s = s.replace(ch, m?.[1] || '');
      if (m?.[2]) tone = parseInt(m[2], 10);
      break;
    }
  }
  return { base: s, tone: tone || 5 };
}

function splitInitial(py: string): { sm: string; ym: string } {
  const lower = py.toLowerCase();
  const sorted = [...INITIALS].sort((a, b) => b.length - a.length);
  for (const init of sorted) {
    if (lower.startsWith(init)) {
      return { sm: init, ym: lower.slice(init.length) };
    }
  }
  return { sm: '', ym: lower };
}

export function parseSyllable(syl: string): Omit<CharData, 'c'> {
  const { base, tone } = stripTone(syl.trim());
  const { sm, ym } = splitInitial(base);
  return { py: base, sm, ym, tone };
}

export function charsFromWordPinyin(word: string, pinyin: string): CharData[] | null {
  const chars = [...word.replace(/\s/g, '')];
  const syls = normalizePinyinInput(pinyin).split(/\s+/).filter(Boolean);
  if (chars.length !== syls.length) return null;
  return chars.map((c, i) => {
    const p = parseSyllable(syls[i]);
    return { c, ...p };
  });
}

export function toneDigit(tone: number): string {
  if (tone >= 1 && tone <= 5) return String(tone);
  return '5';
}

export function charsToDigitPinyin(chars: CharData[]): string {
  return chars
    .map((ch) => {
      const tone = ch.tone >= 1 && ch.tone <= 5 ? ch.tone : '';
      return ch.py + (tone && tone !== 5 ? tone : tone === 5 ? '5' : '');
    })
    .join(' ');
}

export function formatPinyinAsDigit(pinyin: string): string {
  const syls = pinyin.split(/\s+/).filter(Boolean);
  return syls
    .map((syl) => {
      const p = parseSyllable(syl);
      if (p.tone >= 1 && p.tone <= 4) return p.py + p.tone;
      if (p.tone === 5) return p.py + '5';
      return p.py;
    })
    .join(' ');
}

export function toneLabel(tone: number): string {
  if (tone >= 1 && tone <= 4) return String(tone);
  return '轻';
}

export const DEDUCE_ATTRS = [
  { key: 'char' as const, label: '字', get: (ch: CharData) => ch.c },
  { key: 'sm' as const, label: '声母', get: (ch: CharData) => ch.sm || '零' },
  { key: 'ym' as const, label: '韵母', get: (ch: CharData) => ch.ym || '—' },
  { key: 'tone' as const, label: '声调', get: (ch: CharData) => toneLabel(ch.tone) },
];

export function charOwnsPronunciation(marks: { char?: MarkState | string | null }): boolean {
  return marks.char === 'hit' || marks.char === 'present';
}

export function syncPronunciationMarksForChar(marksAt: {
  char: MarkState | string | null;
  sm: MarkState | string | null;
  ym: MarkState | string | null;
  tone: MarkState | string | null;
}): void {
  if (marksAt.char === 'hit' || marksAt.char === 'present') {
    marksAt.sm = null;
    marksAt.ym = null;
    marksAt.tone = null;
  }
}
