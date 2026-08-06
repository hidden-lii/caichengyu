import type { DeduceGuess, IndexedIdiom, PositionMarks } from '../entity/idiom';
import {
  charsToDigitPinyin,
  formatPinyinAsDigit,
  isAutoPronunciationAttr,
  normalizePinyinInput,
  parseSyllable,
  syncPronunciationMarksForChar,
  toneDigit,
} from './pinyin';
import { deduceFilter, finalizeMarks, getActiveDeduceGuesses, mergeDeduceConstraints } from './deduce';

const MARK_LABEL: Record<string, string> = { hit: '对', present: '偏', absent: '无' };
const MARK_FROM_LABEL: Record<string, string> = {
  对: 'hit',
  偏: 'present',
  无: 'absent',
  hit: 'hit',
  present: 'present',
  absent: 'absent',
};

function markLabel(state: string | null): string {
  return MARK_LABEL[state || ''] || (state ? String(state) : '无');
}

function normalizeDebugMark(v: unknown): 'hit' | 'present' | 'absent' {
  if (v == null || v === '') return 'absent';
  const key = String(v).trim();
  return (MARK_FROM_LABEL[key] as 'hit' | 'present' | 'absent') || 'absent';
}

function serializeGuessForDebug(guess: DeduceGuess, idx: number) {
  const digitPy = charsToDigitPinyin(guess.chars) || formatPinyinAsDigit(guess.pinyin) || guess.pinyin;
  const positions = guess.chars.map((ch, pos) => {
    const marks = guess.marks[pos] || {};
    const pyFollowsChar =
      isAutoPronunciationAttr(marks, 'sm') ||
      isAutoPronunciationAttr(marks, 'ym') ||
      isAutoPronunciationAttr(marks, 'tone');
    return {
      pos: pos + 1,
      char: ch.c,
      pinyin: `${ch.py}${toneDigit(ch.tone)}`,
      sm: ch.sm || '',
      ym: ch.ym || '',
      tone: ch.tone,
      pyFollowsChar,
      auto: {
        sm: !!marks.autoSm,
        ym: !!marks.autoYm,
        tone: !!marks.autoTone,
      },
      marks: {
        char: marks.char || 'absent',
        sm: marks.sm || 'absent',
        ym: marks.ym || 'absent',
        tone: marks.tone || 'absent',
      },
    };
  });
  return { index: idx + 1, word: guess.word, pinyin: digitPy, positions };
}

export function buildDeduceDebugDump(
  index: IndexedIdiom[],
  deduceGuesses: DeduceGuess[],
  deduceDraft: DeduceGuess | null
): string {
  const active = getActiveDeduceGuesses(deduceGuesses, deduceDraft);
  const results = deduceFilter(index, active);
  const constraints = active.length ? mergeDeduceConstraints(active) : null;
  const payload = {
    type: 'caichengyu-debug',
    version: 1,
    guessCount: deduceGuesses.length,
    activeGuessCount: active.length,
    candidateCount: results.length,
    candidatesPreview: results.slice(0, 30).map((r) => r.word),
    guesses: deduceGuesses.map((g, i) => serializeGuessForDebug(g, i)),
    draft:
      deduceDraft && deduceDraft.marks.some((m) => m.char || m.sm || m.ym || m.tone)
        ? serializeGuessForDebug({ ...deduceDraft, marks: finalizeMarks(deduceDraft.marks) }, -1)
        : null,
    constraints: constraints
      ? {
          len: constraints.len,
          fixed: constraints.fixed,
          present: constraints.present,
          excludeAt: constraints.excludeAt,
          owned: {
            sm: [...constraints.owned.sm],
            ym: [...constraints.owned.ym],
            tone: [...constraints.owned.tone],
          },
          absent: {
            char: [...constraints.absent.char],
            sm: [...constraints.absent.sm],
            ym: [...constraints.absent.ym],
            tone: [...constraints.absent.tone],
          },
        }
      : null,
  };

  const textParts = [
    '# 猜成语调试信息',
    `已确认猜测: ${deduceGuesses.length} 条；参与筛选: ${active.length} 条；候选: ${results.length} 条`,
    '',
    '## JSON',
    JSON.stringify(payload, null, 2),
  ];
  return textParts.join('\n');
}

function extractDebugPayload(text: string): Record<string, unknown> | null {
  const trimmed = String(text || '').trim();
  if (!trimmed) return null;

  const tryParse = (raw: string) => {
    try {
      const parsed = JSON.parse(raw);
      if (!parsed || typeof parsed !== 'object') return null;
      if (parsed.type === 'caichengyu-debug') return parsed;
      if (Array.isArray(parsed.guesses)) return parsed;
      return null;
    } catch {
      return null;
    }
  };

  const direct = tryParse(trimmed);
  if (direct) return direct;

  const marker = trimmed.search(/##\s*JSON\b/i);
  if (marker >= 0) {
    const after = trimmed.slice(marker).replace(/^##\s*JSON\b/i, '').trim();
    const start = after.indexOf('{');
    if (start >= 0) {
      const fromMarker = tryParse(after.slice(start));
      if (fromMarker) return fromMarker;
    }
  }

  const brace = trimmed.indexOf('{');
  if (brace >= 0) return tryParse(trimmed.slice(brace));
  return null;
}

function deserializeGuessFromDebug(data: Record<string, unknown>): DeduceGuess | null {
  const word = String(data.word || '');
  const positions = data.positions as Array<Record<string, unknown>> | undefined;
  if (!word || !Array.isArray(positions) || !positions.length) return null;
  const wordChars = [...word.replace(/\s/g, '')];
  if (wordChars.length && wordChars.length !== positions.length) return null;

  const chars = positions.map((p, i) => {
    const c = String(p.char || wordChars[i] || '');
    if (!c) return null;
    if (p.pinyin) {
      const parsed = parseSyllable(String(p.pinyin));
      return {
        c,
        py: parsed.py,
        sm: p.sm !== undefined && p.sm !== null ? String(p.sm) : parsed.sm,
        ym: p.ym !== undefined && p.ym !== null ? String(p.ym) : parsed.ym,
        tone:
          p.tone !== undefined && p.tone !== null && p.tone !== ''
            ? Number(p.tone)
            : parsed.tone,
      };
    }
    return {
      c,
      py: String(p.sm || '') + String(p.ym || ''),
      sm: p.sm != null ? String(p.sm) : '',
      ym: p.ym != null ? String(p.ym) : '',
      tone: p.tone !== undefined && p.tone !== null && p.tone !== '' ? Number(p.tone) : 5,
    };
  });
  if (chars.some((ch) => !ch)) return null;

  const marks: PositionMarks[] = positions.map((p) => {
    const m = (p.marks || {}) as Record<string, unknown>;
    const auto = (p.auto || {}) as Record<string, unknown>;
    const out: PositionMarks = {
      char: normalizeDebugMark(m.char),
      sm: normalizeDebugMark(m.sm),
      ym: normalizeDebugMark(m.ym),
      tone: normalizeDebugMark(m.tone),
      autoSm: !!auto.sm,
      autoYm: !!auto.ym,
      autoTone: !!auto.tone,
    };
    if (p.pyFollowsChar || out.char === 'hit' || out.char === 'present') {
      syncPronunciationMarksForChar(out);
    }
    return out;
  });

  const digitPy =
    String(data.pinyin || '') ||
    charsToDigitPinyin(chars as NonNullable<(typeof chars)[0]>[]) ||
    (chars as NonNullable<(typeof chars)[0]>[]).map((ch) => ch.py + toneDigit(ch.tone)).join(' ');

  return {
    word: wordChars.length ? wordChars.join('') : (chars as NonNullable<(typeof chars)[0]>[]).map((ch) => ch.c).join(''),
    pinyin: normalizePinyinInput(digitPy) || digitPy,
    chars: chars as NonNullable<(typeof chars)[0]>[],
    marks: finalizeMarks(marks),
  };
}

export function applyDeduceDebugPaste(
  text: string
): { guesses: DeduceGuess[]; draft: DeduceGuess | null; failed: number[] } | null {
  const payload = extractDebugPayload(text);
  if (!payload) return null;

  const sourceGuesses = Array.isArray(payload.guesses) ? payload.guesses : [];
  const restored: DeduceGuess[] = [];
  const failed: number[] = [];
  for (let i = 0; i < sourceGuesses.length; i++) {
    const guess = deserializeGuessFromDebug(sourceGuesses[i] as Record<string, unknown>);
    if (guess) restored.push(guess);
    else failed.push(i + 1);
  }

  let draft: DeduceGuess | null = null;
  if (payload.draft) {
    draft = deserializeGuessFromDebug(payload.draft as Record<string, unknown>);
  }

  if (!restored.length && !draft) return null;
  return { guesses: restored, draft, failed };
}

export function formatGuessDebugText(guess: DeduceGuess, idx: number): string {
  const data = serializeGuessForDebug(guess, idx);
  const lines = [`## #${data.index} ${data.word} (${data.pinyin})`];
  for (const p of data.positions) {
    const pyNote = !p.pyFollowsChar ? '' : ' · 字已确认，声韵调不参与筛选';
    lines.push(
      `[${p.pos}] ${p.char} ${p.pinyin}` +
        ` | 字:${markLabel(p.marks.char)}` +
        ` 声母(${p.sm || '零'}):${markLabel(p.marks.sm)}` +
        ` 韵母(${p.ym || '—'}):${markLabel(p.marks.ym)}` +
        ` 调(${toneDigit(p.tone)}):${markLabel(p.marks.tone)}` +
        pyNote
    );
  }
  return lines.join('\n');
}
