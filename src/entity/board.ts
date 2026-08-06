import type { MarkState } from './idiom';

export interface PositionMarksRaw {
  char: string;
  sm: string;
  ym: string;
  tone: string;
}

export interface ParsedCell {
  col: number;
  attr: string;
  text: string;
  rgb: number[];
  mark: string;
}

export interface ParsedGuess {
  word: string;
  marks: PositionMarksRaw[];
  cells?: ParsedCell[];
}

export interface BoardParseResult {
  guesses: ParsedGuess[];
  warnings: string[];
  /** 模型原始返回文本 */
  raw_response?: string;
}

export type OcrMarkState = Exclude<MarkState, null>;

export interface OcrReviewRow {
  word: string;
  marks: {
    char: OcrMarkState;
    sm: OcrMarkState;
    ym: OcrMarkState;
    tone: OcrMarkState;
  }[];
  selected: boolean;
  missingLexicon: boolean;
}

export function rawMarkToState(raw: string): OcrMarkState {
  if (raw === 'hit' || raw === 'present' || raw === 'absent') return raw;
  return 'absent';
}
