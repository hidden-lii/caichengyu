import type { CharData, IndexedIdiom } from '../entity/idiom';
import { INITIALS, parseSyllable } from './pinyin';

const SM_SET = new Set(INITIALS);
const HAN = /^\p{Script=Han}$/u;
const LATIN = /^[a-zv]+$/;

export interface QueryConstraint {
  char?: string;
  py?: string;
  sm?: string;
  ym?: string;
  tone?: number;
}

function isInitialOnly(s: string): boolean {
  return SM_SET.has(s);
}

export function parseToken(token: string): QueryConstraint | null {
  const t = token.trim();
  if (!t) return null;
  if (/^\?+$/.test(t)) return {};
  if (HAN.test(t)) return { char: t };

  const lower = t.toLowerCase().replace(/ü/g, 'v');

  if (/^[1-5]$/.test(lower) || /^\?+[1-5]$/.test(lower)) {
    return { tone: parseInt(lower.replace(/\?/g, ''), 10) };
  }

  let tone: number | undefined;
  let body = lower;

  const toneTail = lower.match(/^(.+)([1-5])$/);
  if (toneTail && !/^\?+$/.test(toneTail[1])) {
    tone = parseInt(toneTail[2], 10);
    body = toneTail[1];
  }

  body = body.replace(/\?+$/, '');
  if (!body) {
    const cons: QueryConstraint = {};
    if (tone !== undefined) cons.tone = tone;
    return cons;
  }

  if (body.includes('?')) {
    const qIdx = body.indexOf('?');
    const left = body.slice(0, qIdx);
    const right = body.slice(qIdx + 1).replace(/\?/g, '');
    const cons: QueryConstraint = {};
    if (tone !== undefined) cons.tone = tone;
    if (left) cons.sm = left;
    if (right) cons.ym = right;
    return cons;
  }

  if (isInitialOnly(body)) {
    const cons: QueryConstraint = { sm: body };
    if (tone !== undefined) cons.tone = tone;
    return cons;
  }

  if (LATIN.test(body)) {
    const parsed = parseSyllable(tone !== undefined ? body + tone : body);
    const cons: QueryConstraint = { py: parsed.py, sm: parsed.sm, ym: parsed.ym };
    if (tone !== undefined) cons.tone = parsed.tone;
    return cons;
  }

  return null;
}

export function parseQuery(input: string): (QueryConstraint | null)[] {
  return input.trim().split(/\s+/).filter(Boolean).map(parseToken);
}

function matchConstraint(cons: QueryConstraint, ch: CharData): boolean {
  if (!cons) return false;
  if (cons.char !== undefined && cons.char !== ch.c) return false;
  if (cons.py !== undefined && cons.py !== ch.py) return false;
  if (cons.sm !== undefined && cons.sm !== ch.sm) return false;
  if (cons.ym !== undefined && cons.ym !== ch.ym) return false;
  if (cons.tone !== undefined && cons.tone !== ch.tone) return false;
  return true;
}

export function search(
  index: IndexedIdiom[],
  input: string
): { error: string | null; results: IndexedIdiom[] } {
  const tokens = parseQuery(input);
  if (!tokens.length) return { error: '请输入查询条件', results: [] };
  if (tokens.some((t) => t === null)) return { error: '查询串含无法识别的片段', results: [] };
  const results = index.filter((item) => {
    if (item.chars.length !== tokens.length) return false;
    return item.chars.every((ch, i) => matchConstraint(tokens[i]!, ch));
  });
  return { error: null, results };
}
