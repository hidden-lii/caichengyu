import { getSetting, setSetting } from '../api/idiom';
import type { BoardParseResult, ParsedGuess } from '../entity/board';

export const OCR_HISTORY_KEY = 'qwen_ocr_history';
export const OCR_HISTORY_MAX = 80;

export interface OcrHistoryEntry {
  id: string;
  createdAt: number;
  model: string;
  ok: boolean;
  error?: string;
  rawResponse: string;
  guesses: ParsedGuess[];
  warnings: string[];
  guessWords: string[];
}

function newId(): string {
  return `${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`;
}

export function createHistoryEntry(params: {
  model: string;
  ok: boolean;
  error?: string;
  rawResponse?: string;
  result?: BoardParseResult | null;
}): OcrHistoryEntry {
  const guesses = params.result?.guesses ?? [];
  return {
    id: newId(),
    createdAt: Date.now(),
    model: params.model,
    ok: params.ok,
    error: params.error,
    rawResponse: params.rawResponse || params.result?.raw_response || '',
    guesses,
    warnings: params.result?.warnings ?? [],
    guessWords: guesses.map((g) => g.word),
  };
}

export async function loadOcrHistory(): Promise<OcrHistoryEntry[]> {
  const raw = await getSetting(OCR_HISTORY_KEY);
  if (!raw?.trim()) return [];
  try {
    const parsed = JSON.parse(raw) as unknown;
    if (!Array.isArray(parsed)) return [];
    return parsed.filter(isHistoryEntry);
  } catch {
    return [];
  }
}

function isHistoryEntry(v: unknown): v is OcrHistoryEntry {
  if (!v || typeof v !== 'object') return false;
  const o = v as Record<string, unknown>;
  return typeof o.id === 'string' && typeof o.createdAt === 'number' && typeof o.ok === 'boolean';
}

export async function saveOcrHistory(list: OcrHistoryEntry[]): Promise<void> {
  await setSetting(OCR_HISTORY_KEY, JSON.stringify(list.slice(0, OCR_HISTORY_MAX)));
}

export async function appendOcrHistory(entry: OcrHistoryEntry): Promise<OcrHistoryEntry[]> {
  const list = await loadOcrHistory();
  list.unshift(entry);
  const next = list.slice(0, OCR_HISTORY_MAX);
  await saveOcrHistory(next);
  return next;
}

export async function removeOcrHistory(id: string): Promise<OcrHistoryEntry[]> {
  const list = await loadOcrHistory();
  const next = list.filter((e) => e.id !== id);
  await saveOcrHistory(next);
  return next;
}

export async function clearOcrHistory(): Promise<OcrHistoryEntry[]> {
  await saveOcrHistory([]);
  return [];
}

export function formatHistoryTime(ts: number): string {
  try {
    return new Date(ts).toLocaleString('zh-CN', {
      year: 'numeric',
      month: '2-digit',
      day: '2-digit',
      hour: '2-digit',
      minute: '2-digit',
      second: '2-digit',
      hour12: false,
    });
  } catch {
    return String(ts);
  }
}
