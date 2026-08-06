import type { DeduceGuess, IndexedIdiom, MarkState, PositionMarks } from '../entity/idiom';
import {
  charOwnsPronunciation,
  DEDUCE_ATTRS,
  isAutoPronunciationAttr,
  setAutoPronunciationAttr,
  type PronunciationAttr,
} from './pinyin';

interface FixedPos {
  c?: string;
  sm?: string;
  ym?: string;
  tone?: number;
}

interface PresentConstraint {
  kind: 'char' | 'sm' | 'ym' | 'tone';
  value: string | number;
  notAt: number;
}

/** 某位明确标成 absent 时，该位禁止出现该值（即使同值在别处 present/hit 导致不能进全局 absent） */
interface ExcludeAtConstraint {
  kind: 'sm' | 'ym' | 'tone';
  value: string | number;
  at: number;
}

interface MergedConstraints {
  fixed: (FixedPos | undefined)[];
  present: PresentConstraint[];
  excludeAt: ExcludeAtConstraint[];
  absent: {
    char: Set<string>;
    sm: Set<string>;
    ym: Set<string>;
    tone: Set<string | number>;
  };
  len: number;
  owned: { sm: Set<string>; ym: Set<string>; tone: Set<number> };
}

function attrMatches(
  kind: 'sm' | 'ym' | 'tone',
  ch: { sm: string; ym: string; tone: number },
  value: string | number
): boolean {
  if (kind === 'sm') return ch.sm === value || String(ch.sm) === String(value);
  if (kind === 'ym') return ch.ym === value || String(ch.ym) === String(value);
  return ch.tone === value || String(ch.tone) === String(value);
}

export function mergeDeduceConstraints(guesses: DeduceGuess[]): MergedConstraints {
  const fixed: (FixedPos | undefined)[] = [];
  const present: PresentConstraint[] = [];
  const excludeAt: ExcludeAtConstraint[] = [];
  const absent = {
    char: new Set<string>(),
    sm: new Set<string>(),
    ym: new Set<string>(),
    tone: new Set<string | number>(),
  };
  let len = 0;
  const charFixed: (string | undefined)[] = [];
  const owned = { sm: new Set<string>(), ym: new Set<string>(), tone: new Set<number>() };

  for (const g of guesses) {
    len = g.chars.length;
    for (let i = 0; i < g.chars.length; i++) {
      const marks = g.marks[i];
      const charState = marks.char || 'absent';
      const ch = g.chars[i];
      if (charState === 'hit') {
        charFixed[i] = ch.c;
        if (!fixed[i]) fixed[i] = {};
        fixed[i]!.c = ch.c;
        if (ch.sm) owned.sm.add(ch.sm);
        if (ch.ym) owned.ym.add(ch.ym);
        if (ch.tone) owned.tone.add(ch.tone);
      } else if (charState === 'present') {
        present.push({ kind: 'char', value: ch.c, notAt: i });
        if (ch.sm) owned.sm.add(ch.sm);
        if (ch.ym) owned.ym.add(ch.ym);
        if (ch.tone) owned.tone.add(ch.tone);
      } else {
        absent.char.add(ch.c);
      }
    }
  }

  const isOwned = (kind: 'sm' | 'ym' | 'tone', val: string | number) => {
    if (kind === 'tone') return owned.tone.has(Number(val));
    return owned[kind].has(String(val));
  };

  for (const g of guesses) {
    for (let i = 0; i < g.chars.length; i++) {
      if (charFixed[i] !== undefined) continue;
      const marks = g.marks[i];
      for (const attr of DEDUCE_ATTRS) {
        if (attr.key === 'char') continue;
        const key = attr.key as PronunciationAttr;
        // 字带出的自动声韵调只参与 owned，不生成固定/偏移/排除；人选标记优先
        if (isAutoPronunciationAttr(marks, key)) continue;
        if (charOwnsPronunciation(marks) && marks[key] == null) continue;
        const state = marks[key] || 'absent';
        const rawVal =
          key === 'sm' ? g.chars[i].sm : key === 'ym' ? g.chars[i].ym : g.chars[i].tone;
        if ((state === 'present' || state === 'absent') && isOwned(key, rawVal)) {
          // 同值已被字命中/偏移“拥有”时不能进全局 absent；人选 present 仍生效
          if (state === 'absent') {
            excludeAt.push({ kind: key, value: rawVal, at: i });
          } else if (state === 'present') {
            present.push({ kind: key, value: rawVal, notAt: i });
          }
          continue;
        }
        if (state === 'hit') {
          if (!fixed[i]) fixed[i] = {};
          (fixed[i] as Record<string, unknown>)[key] = rawVal;
        } else if (state === 'absent') {
          excludeAt.push({ kind: key, value: rawVal, at: i });
          if (key === 'tone') absent.tone.add(rawVal);
          else absent[key].add(String(rawVal));
        } else if (state === 'present') {
          present.push({ kind: key, value: rawVal, notAt: i });
        }
      }
    }
  }

  for (const v of owned.sm) absent.sm.delete(v);
  for (const v of owned.ym) absent.ym.delete(v);
  for (const v of owned.tone) {
    absent.tone.delete(v);
    absent.tone.delete(String(v));
  }
  for (const p of present) {
    if (p.kind === 'char') absent.char.delete(String(p.value));
    else if (p.kind === 'tone') {
      absent.tone.delete(p.value);
      absent.tone.delete(Number(p.value));
      absent.tone.delete(String(p.value));
    } else {
      absent[p.kind].delete(String(p.value));
    }
  }

  // 存在(对/偏)优先于不存在：同位置同值若已 hit，去掉与之冲突的 excludeAt
  const resolvedExclude = excludeAt.filter((e) => {
    const f = fixed[e.at];
    if (!f) return true;
    const fv = (f as Record<string, unknown>)[e.kind];
    if (fv === undefined) return true;
    return !(fv === e.value || String(fv) === String(e.value));
  });

  return { fixed, present, excludeAt: resolvedExclude, absent, len, owned };
}

export function deduceFilter(index: IndexedIdiom[], guesses: DeduceGuess[]): IndexedIdiom[] {
  if (!guesses.length) return index;
  const { fixed, present, excludeAt, absent, len } = mergeDeduceConstraints(guesses);

  for (let i = 0; i < fixed.length; i++) {
    const f = fixed[i];
    if (!f) continue;
    if (f.c !== undefined) absent.char.delete(f.c);
    if (f.sm !== undefined) absent.sm.delete(f.sm);
    if (f.ym !== undefined) absent.ym.delete(f.ym);
    if (f.tone !== undefined) absent.tone.delete(f.tone);
  }

  return index.filter((item) => {
    if (len && item.chars.length !== len) return false;
    const locked = item.chars.map((_, i) => !!(fixed[i] && fixed[i]!.c !== undefined));

    for (let i = 0; i < item.chars.length; i++) {
      const f = fixed[i];
      if (f?.c !== undefined && item.chars[i].c !== f.c) return false;
    }

    for (const p of present) {
      if (p.kind !== 'char') continue;
      const elsewhere = item.chars.some((ch, j) => j !== p.notAt && ch.c === p.value);
      if (!elsewhere) return false;
      if (item.chars[p.notAt].c === p.value) return false;
    }

    for (const v of absent.char) {
      if (item.chars.some((ch) => ch.c === v)) return false;
    }

    for (let i = 0; i < item.chars.length; i++) {
      if (locked[i]) continue;
      const f = fixed[i];
      if (!f) continue;
      const ch = item.chars[i];
      if (f.sm !== undefined && ch.sm !== f.sm) return false;
      if (f.ym !== undefined && ch.ym !== f.ym) return false;
      if (f.tone !== undefined && ch.tone !== f.tone) return false;
    }

    for (const kind of ['sm', 'ym', 'tone'] as const) {
      for (const v of absent[kind]) {
        const cmp = kind === 'tone' ? Number(v) : v;
        const hit = item.chars.some((ch, j) => {
          if (locked[j]) return false;
          return ch[kind] === cmp || String(ch[kind]) === String(v);
        });
        if (hit) return false;
      }
    }

    for (const e of excludeAt) {
      if (locked[e.at]) continue;
      if (attrMatches(e.kind, item.chars[e.at], e.value)) return false;
    }

    for (const p of present) {
      if (p.kind === 'char') continue;
      const kind = p.kind;
      const elsewhere = item.chars.some((ch, j) => {
        if (j === p.notAt) return false;
        if (kind === 'sm') return ch.sm === p.value;
        if (kind === 'ym') return ch.ym === p.value;
        return ch.tone === p.value;
      });
      if (!elsewhere) return false;
      const atPos = item.chars[p.notAt];
      const atVal = kind === 'sm' ? atPos.sm : kind === 'ym' ? atPos.ym : atPos.tone;
      if (!locked[p.notAt] && atVal === p.value) return false;
    }

    return true;
  });
}

export function createEmptyMarks(len: number): PositionMarks[] {
  return Array.from({ length: len }, () => ({
    char: null,
    sm: null,
    ym: null,
    tone: null,
    autoSm: false,
    autoYm: false,
    autoTone: false,
  }));
}

export const MARK_CYCLE: MarkState[] = [null, 'hit', 'present', 'absent'];

export function nextMark(cur: MarkState): MarkState {
  const i = MARK_CYCLE.indexOf(cur || null);
  return MARK_CYCLE[(i + 1) % MARK_CYCLE.length];
}

export function guessHasMark(guess: DeduceGuess | null): boolean {
  return !!(guess && guess.marks.some((m) => m.char || m.sm || m.ym || m.tone));
}

export function finalizeMarks(marks: PositionMarks[]): PositionMarks[] {
  return marks.map((m) => {
    const out: PositionMarks = {
      char: m.char || 'absent',
      sm: m.sm || 'absent',
      ym: m.ym || 'absent',
      tone: m.tone || 'absent',
      autoSm: false,
      autoYm: false,
      autoTone: false,
    };
    if (out.char === 'hit' || out.char === 'present') {
      for (const attr of ['sm', 'ym', 'tone'] as const) {
        const wasAuto = isAutoPronunciationAttr(m, attr);
        const wasUnset = m[attr] == null;
        if (wasAuto || wasUnset) {
          out[attr] = out.char;
          setAutoPronunciationAttr(out, attr, true);
        }
      }
    }
    return out;
  });
}

export function getActiveDeduceGuesses(
  deduceGuesses: DeduceGuess[],
  deduceDraft: DeduceGuess | null
): DeduceGuess[] {
  const list = deduceGuesses.map((g) => ({ ...g, chars: g.chars.map((c) => ({ ...c })), marks: g.marks.map((m) => ({ ...m })) }));
  if (!deduceDraft || !guessHasMark(deduceDraft)) return list;
  const draftForFilter: DeduceGuess = {
    ...deduceDraft,
    marks: finalizeMarks(deduceDraft.marks),
  };
  if (
    Number.isInteger(deduceDraft.editIndex) &&
    deduceDraft.editIndex! >= 0 &&
    deduceDraft.editIndex! < list.length
  ) {
    list[deduceDraft.editIndex!] = draftForFilter;
    return list;
  }
  const existing = list.findIndex((g) => g.word === deduceDraft.word);
  if (existing >= 0) list[existing] = draftForFilter;
  else list.push(draftForFilter);
  return list;
}
