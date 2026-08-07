import { invoke } from '@tauri-apps/api/tauri';
import type { BuiltinLexiconInfo, IdiomEntry, IdiomInput, LexiconMeta, UpsertResult } from '../entity/idiom';
import type { BoardParseResult } from '../entity/board';

export function loadAllIdioms(): Promise<IdiomEntry[]> {
  return invoke<IdiomEntry[]>('load_all_idioms');
}

export function getLexiconMeta(): Promise<LexiconMeta> {
  return invoke<LexiconMeta>('get_lexicon_meta');
}

export function addIdiom(item: IdiomInput): Promise<UpsertResult> {
  return invoke<UpsertResult>('add_idiom', { item });
}

export function upsertIdioms(items: IdiomInput[]): Promise<UpsertResult> {
  return invoke<UpsertResult>('upsert_idioms', { items });
}

export function replaceLexicon(items: IdiomInput[]): Promise<UpsertResult> {
  return invoke<UpsertResult>('replace_lexicon', { items });
}

export function importLexiconFromUrl(url: string): Promise<UpsertResult> {
  return invoke<UpsertResult>('import_lexicon_from_url', { url });
}

export function listBuiltinLexicons(): Promise<BuiltinLexiconInfo[]> {
  return invoke<BuiltinLexiconInfo[]>('list_builtin_lexicons');
}

export function applyBuiltinLexicon(sourceId: string): Promise<UpsertResult> {
  return invoke<UpsertResult>('apply_builtin_lexicon', { sourceId });
}

export function deleteIdiom(word: string): Promise<boolean> {
  return invoke<boolean>('delete_idiom', { word });
}

export function updateIdiomPinyin(word: string, pinyin: string): Promise<UpsertResult> {
  return invoke<UpsertResult>('update_idiom_pinyin', { word, pinyin });
}

export function getSetting(key: string): Promise<string | null> {
  return invoke<string | null>('get_setting', { key });
}

export function setSetting(key: string, value: string): Promise<boolean> {
  return invoke<boolean>('set_setting', { key, value });
}

export function listQwenModels(apiKey: string, keyPlan?: string): Promise<string[]> {
  return invoke<string[]>('list_qwen_models', {
    apiKey,
    keyPlan: keyPlan ?? null,
  });
}

export function getQwenPromptSchema(): Promise<string> {
  return invoke<string>('get_qwen_prompt_schema');
}

export function getQwenDefaultPrompt(): Promise<string> {
  return invoke<string>('get_qwen_default_prompt');
}

export function parseGuessBoard(params: {
  imageB64: string;
  mime?: string;
  apiKey: string;
  /** dashscope | token_plan | coding_plan */
  keyPlan?: string;
  model: string;
  prompt?: string;
  /** 为 true 时后端走 SSE，并通过 qwen-ocr-chunk 事件推送增量 */
  stream?: boolean;
}): Promise<BoardParseResult> {
  return invoke<BoardParseResult>('parse_guess_board', {
    imageB64: params.imageB64,
    mime: params.mime ?? null,
    apiKey: params.apiKey,
    keyPlan: params.keyPlan ?? null,
    model: params.model,
    prompt: params.prompt ?? null,
    stream: params.stream ?? false,
  });
}

/** 本地 PP-OCRv5：放大 + 三色通道二值化识别 */
export function parseGuessBoardLocal(params: {
  imageB64: string;
  /** 放大倍数，默认 5（500%） */
  scale?: number;
}): Promise<BoardParseResult> {
  return invoke<BoardParseResult>('parse_guess_board_local', {
    imageB64: params.imageB64,
    scale: params.scale ?? 5,
  });
}
