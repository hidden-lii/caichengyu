import type { BoardParseResult, OcrReviewRow } from '../entity/board';
import { rawMarkToState } from '../entity/board';
import type { DeduceGuess, IndexedIdiom, MarkState } from '../entity/idiom';
import { finalizeMarks } from './deduce';
import { syncPronunciationMarksForChar } from './pinyin';

export function boardResultToReviewRows(result: BoardParseResult): OcrReviewRow[] {
  return result.guesses.map((g) => ({
    word: g.word,
    marks: g.marks.map((m) => ({
      char: rawMarkToState(m.char),
      sm: rawMarkToState(m.sm),
      ym: rawMarkToState(m.ym),
      tone: rawMarkToState(m.tone),
    })),
    selected: true,
    missingLexicon: false,
  }));
}

export function reviewRowsToDeduceGuesses(
  rows: OcrReviewRow[],
  wordMap: Map<string, IndexedIdiom>
): { guesses: DeduceGuess[]; skipped: string[] } {
  const guesses: DeduceGuess[] = [];
  const skipped: string[] = [];

  for (const row of rows) {
    if (!row.selected) continue;
    const word = row.word.trim().replace(/\s/g, '');
    if (!word) {
      skipped.push('（空行）');
      continue;
    }

    const found = wordMap.get(word);
    if (!found) {
      skipped.push(word);
      continue;
    }

    const marks = row.marks.slice(0, found.chars.length);
    while (marks.length < found.chars.length) {
      marks.push({ char: 'absent', sm: 'absent', ym: 'absent', tone: 'absent' });
    }

    const normalizedMarks = marks.map((m) => {
      const marksAt = {
        char: m.char as MarkState,
        sm: m.sm as MarkState,
        ym: m.ym as MarkState,
        tone: m.tone as MarkState,
      };
      if (marksAt.char === 'hit' || marksAt.char === 'present') {
        syncPronunciationMarksForChar(marksAt);
      }
      return marksAt;
    });

    guesses.push({
      word: found.word,
      pinyin: found.pinyin,
      chars: found.chars.map((ch) => ({ ...ch })),
      marks: finalizeMarks(normalizedMarks),
    });
  }

  return { guesses, skipped };
}

export async function fileToBase64(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => {
      const result = String(reader.result || '');
      const comma = result.indexOf(',');
      resolve(comma >= 0 ? result.slice(comma + 1) : result);
    };
    reader.onerror = () => reject(reader.error || new Error('读取图片失败'));
    reader.readAsDataURL(file);
  });
}
