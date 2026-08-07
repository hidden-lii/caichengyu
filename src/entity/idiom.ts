export interface CharData {
  c: string;
  py: string;
  sm: string;
  ym: string;
  tone: number;
}

export interface IdiomEntry {
  id?: number;
  word: string;
  pinyin: string;
  explanation: string;
  char_count?: number;
  chars: CharData[];
  blind_eligible?: boolean;
}

export interface IdiomInput {
  word: string;
  pinyin: string;
  explanation?: string;
}

export interface IndexedIdiom extends IdiomEntry {
  chars: CharData[];
}

export type MarkState = 'hit' | 'present' | 'absent' | null;

export interface PositionMarks {
  char: MarkState;
  sm: MarkState;
  ym: MarkState;
  tone: MarkState;
  /** 声/韵/调是否由「字」的存在标记自动带出（人选可覆盖，冲突以人选为准） */
  autoSm?: boolean;
  autoYm?: boolean;
  autoTone?: boolean;
}

export interface DeduceGuess {
  word: string;
  pinyin: string;
  chars: CharData[];
  marks: PositionMarks[];
  editIndex?: number;
}

export interface LexiconMeta {
  total: number;
  blind_count: number;
}

export interface UpsertResult {
  added: number;
  updated: number;
  skipped: number;
  errors: string[];
}

export interface BuiltinLexiconInfo {
  id: string;
  name: string;
  description: string;
}
