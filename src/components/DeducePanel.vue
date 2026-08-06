<script setup lang="ts">
import { computed, nextTick, onUnmounted, ref } from 'vue';
import { writeText } from '@tauri-apps/api/clipboard';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { getSetting, parseGuessBoard, setSetting } from '../api/idiom';
import type { BoardParseResult, OcrReviewRow } from '../entity/board';
import type { DeduceGuess, IndexedIdiom, MarkState } from '../entity/idiom';
import { applyDeduceDebugPaste, buildDeduceDebugDump } from '../engine/debug';
import {
  createEmptyMarks,
  deduceFilter,
  finalizeMarks,
  getActiveDeduceGuesses,
  guessHasMark,
} from '../engine/deduce';
import { boardResultToReviewRows, reviewRowsToDeduceGuesses } from '../engine/ocr';
import { appendOcrHistory, createHistoryEntry } from '../engine/ocrHistory';
import {
  charsFromWordPinyin,
  charsToDigitPinyin,
  formatPinyinAsDigit,
  isAutoPronunciationAttr,
  normalizePinyinInput,
  parseSyllable,
  setAutoPronunciationAttr,
  syncPronunciationMarksForChar,
  toneDigit,
  DEDUCE_ATTRS,
  type PronunciationAttr,
} from '../engine/pinyin';
import { addIdiom, updateIdiomPinyin } from '../api/idiom';
import { useLexicon } from '../composables/useLexicon';
import { QWEN_SETTING_KEYS, type QwenKeyPlan } from '../engine/qwen';
import ImageIntake from './ImageIntake.vue';
import IdiomResultList from './IdiomResultList.vue';
import OcrReview from './OcrReview.vue';
import QwenConfig from './QwenConfig.vue';

const props = defineProps<{
  index: IndexedIdiom[];
  wordMap: Map<string, IndexedIdiom>;
}>();

const { reload: reloadLexicon } = useLexicon();

function yieldToUi(): Promise<void> {
  return new Promise((resolve) => {
    requestAnimationFrame(() => {
      requestAnimationFrame(() => resolve());
    });
  });
}

const wordInput = ref('');
const pinyinInput = ref('');
const showPinyinBar = ref(false);
const deduceGuesses = ref<DeduceGuess[]>([]);
const deduceDraft = ref<DeduceGuess | null>(null);
const commitMsg = ref('');
const commitMsgClass = ref('');
const debugPaste = ref('');
const debugPasteMsg = ref('');
const debugPasteMsgClass = ref('');
const ocrBusy = ref(false);
const ocrStatusText = ref('');
const ocrReviewRows = ref<OcrReviewRow[] | null>(null);
const ocrWarnings = ref<string[]>([]);
const ocrPreviewUrl = ref('');
const ocrLastRaw = ref('');
const ocrLastMeta = ref('');
const ocrShowStream = ref(false);
const ocrStreamText = ref('');
const qwenConfig = ref({
  apiKey: '',
  keyPlan: 'dashscope' as QwenKeyPlan,
  model: '',
  prompt: '',
});
const qwenConfigRef = ref<InstanceType<typeof QwenConfig> | null>(null);
const lexiconBusy = ref(false);
const lexiconBusyText = ref('');
const showDetailPanel = ref(false);
const markPreviewRef = ref<HTMLElement | null>(null);
const persistDialog = ref<{
  mode: 'add' | 'update';
  word: string;
  pinyin: string;
  digit: string;
  oldDigit?: string;
} | null>(null);

let ocrChunkUnlisten: UnlistenFn | null = null;

void getSetting(QWEN_SETTING_KEYS.streamPreview).then((v) => {
  ocrShowStream.value = v === '1';
});

async function onStreamPreviewChange(value: boolean) {
  ocrShowStream.value = value;
  await setSetting(QWEN_SETTING_KEYS.streamPreview, value ? '1' : '0');
}

async function attachOcrChunkListener() {
  await detachOcrChunkListener();
  ocrChunkUnlisten = await listen<{ delta: string; text: string }>('qwen-ocr-chunk', (event) => {
    ocrStreamText.value = event.payload?.text || '';
  });
}

async function detachOcrChunkListener() {
  if (ocrChunkUnlisten) {
    ocrChunkUnlisten();
    ocrChunkUnlisten = null;
  }
}

onUnmounted(() => {
  void detachOcrChunkListener();
});

const activeGuesses = computed(() => getActiveDeduceGuesses(deduceGuesses.value, deduceDraft.value));
const results = computed(() =>
  activeGuesses.value.length ? deduceFilter(props.index, activeGuesses.value) : []
);
const displayResults = computed(() => results.value.slice(0, 200));
const resultsMeta = computed(() => {
  if (!activeGuesses.value.length) return '';
  const draftHint = deduceDraft.value && guessHasMark(deduceDraft.value) ? '（含当前编辑预览）' : '';
  return `合并 ${activeGuesses.value.length} 条猜测后，共 ${results.value.length} 条候选${draftHint}`;
});

function setMsg(text: string, cls = 'ok') {
  commitMsg.value = text;
  commitMsgClass.value = cls;
}

function loadWord(wordOverride?: string) {
  const word = (typeof wordOverride === 'string' ? wordOverride : wordInput.value).trim().replace(/\s/g, '');
  wordInput.value = word;
  setMsg('');
  showPinyinBar.value = false;
  if (!word) {
    setMsg('请输入成语', 'err');
    return;
  }
  // 当前草稿就是该词，保留已选标记
  if (deduceDraft.value?.word === word) {
    showPinyinBar.value = true;
    pinyinInput.value =
      formatPinyinAsDigit(deduceDraft.value.pinyin) || charsToDigitPinyin(deduceDraft.value.chars);
    return;
  }
  // 已有带标记的草稿时先提交保留，再加载新词
  if (deduceDraft.value && guessHasMark(deduceDraft.value)) {
    if (!commitGuess()) return;
    wordInput.value = word;
  }
  const existingIdx = deduceGuesses.value.findIndex((g) => g.word === word);
  if (existingIdx >= 0) {
    editGuess(existingIdx);
    return;
  }
  const found = props.wordMap.get(word);
  if (found) {
    deduceDraft.value = {
      word: found.word,
      pinyin: found.pinyin,
      chars: found.chars.map((ch) => ({ ...ch })),
      marks: createEmptyMarks(found.chars.length),
    };
    showPinyinBar.value = true;
    pinyinInput.value = formatPinyinAsDigit(found.pinyin);
    setMsg('已加载词库读音，如有误请修改拼音后点「应用读音」', 'warn');
    return;
  }
  deduceDraft.value = null;
  showPinyinBar.value = true;
  pinyinInput.value = '';
  setMsg(`词库中未找到「${word}」，请填写拼音`, 'warn');
}

function askPersistPinyin(word: string, pinyin: string) {
  const digit = formatPinyinAsDigit(pinyin) || pinyin;
  const found = props.wordMap.get(word);
  if (!found) {
    persistDialog.value = { mode: 'add', word, pinyin, digit };
    setMsg(`读音已改成 ${digit}，请确认是否写入词库`, 'warn');
    return;
  }
  const oldDigit = formatPinyinAsDigit(found.pinyin);
  if (oldDigit === digit) {
    setMsg(`读音 ${digit} 与词库一致，无需更新`, 'ok');
    return;
  }
  persistDialog.value = { mode: 'update', word, pinyin, digit, oldDigit };
  setMsg(`读音已从 ${oldDigit} 改为 ${digit}，请确认是否更新词库`, 'warn');
}

function cancelPersistPinyin() {
  persistDialog.value = null;
  setMsg('已仅更新当前展示，未改词库', 'warn');
}

async function confirmPersistPinyin() {
  const dialog = persistDialog.value;
  if (!dialog) return;
  persistDialog.value = null;

  lexiconBusy.value = true;
  lexiconBusyText.value = dialog.mode === 'add' ? '正在写入词库…' : '正在更新词库读音…';
  await yieldToUi();
  try {
    if (dialog.mode === 'add') {
      const result = await addIdiom({ word: dialog.word, pinyin: dialog.pinyin, explanation: '' });
      if (result.added || result.updated) {
        lexiconBusyText.value = '正在重新加载词库…';
        await yieldToUi();
        await reloadLexicon('正在重新加载词库…');
        setMsg(`已新增「${dialog.word}」到词库（${dialog.digit}）`, 'ok');
      } else {
        setMsg(result.errors[0] || '写入词库失败', 'err');
      }
      return;
    }

    const result = await updateIdiomPinyin(dialog.word, dialog.pinyin);
    if (result.updated) {
      lexiconBusyText.value = '正在重新加载词库…';
      await yieldToUi();
      await reloadLexicon('正在重新加载词库…');
      setMsg(`已更新词库「${dialog.word}」读音为 ${dialog.digit}`, 'ok');
    } else {
      setMsg(result.errors[0] || '更新词库失败', 'warn');
    }
  } catch (e) {
    setMsg(
      (dialog.mode === 'add' ? '写入' : '更新') +
        '词库失败：' +
        (e instanceof Error ? e.message : String(e)),
      'err'
    );
  } finally {
    lexiconBusy.value = false;
    lexiconBusyText.value = '';
  }
}

function applyPinyin() {
  const word = deduceDraft.value?.word || wordInput.value.trim().replace(/\s/g, '');
  const pinyin = pinyinInput.value.trim();
  if (!word) {
    setMsg('请先输入成语', 'err');
    return;
  }
  const chars = charsFromWordPinyin(word, pinyin);
  if (!chars) {
    setMsg('拼音音节数与字数不一致', 'err');
    return;
  }
  const marks = deduceDraft.value ? deduceDraft.value.marks : createEmptyMarks(chars.length);
  const editIndex = deduceDraft.value?.editIndex;
  const normalized = normalizePinyinInput(pinyin);
  deduceDraft.value = {
    word,
    pinyin: normalized,
    chars,
    marks,
    ...(editIndex !== undefined ? { editIndex } : {}),
  };
  setMsg(
    Number.isInteger(editIndex) ? '读音已应用，可继续改标记后点「更新本条并筛选」' : '读音已应用，请标记后点「确认本条并筛选」',
    'ok'
  );
  askPersistPinyin(word, normalized);
}

function onSyllableEdit(pos: number, raw: string) {
  if (!deduceDraft.value?.chars[pos]) return;
  const trimmed = raw.trim();
  if (!trimmed) return;
  const parsed = parseSyllable(trimmed);
  if (!parsed.py) {
    setMsg('无法解析读音，请使用如 jian4 / xián 格式', 'err');
    return;
  }
  const ch = deduceDraft.value.chars[pos];
  ch.py = parsed.py;
  ch.sm = parsed.sm;
  ch.ym = parsed.ym;
  ch.tone = parsed.tone;
  const digit = charsToDigitPinyin(deduceDraft.value.chars);
  pinyinInput.value = digit;
  deduceDraft.value.pinyin = normalizePinyinInput(digit);
  setMsg(`第 ${pos + 1} 字读音已改为 ${parsed.py}${toneDigit(parsed.tone)}`, 'ok');
  askPersistPinyin(deduceDraft.value.word, deduceDraft.value.pinyin);
}

function clearTextSelection() {
  const sel = window.getSelection?.();
  if (sel && sel.rangeCount) sel.removeAllRanges();
}

function toggleNodeMark(pos: number, attr: 'char' | PronunciationAttr) {
  // 左键：对 ↔ 无
  clearTextSelection();
  if (!deduceDraft.value) return;
  const marksAt = deduceDraft.value.marks[pos];
  if (attr !== 'char' && isAutoPronunciationAttr(marksAt, attr)) {
    setAutoPronunciationAttr(marksAt, attr, false);
  }
  const cur = marksAt[attr] as MarkState;
  marksAt[attr] = cur === 'hit' ? 'absent' : 'hit';
  if (attr === 'char') syncPronunciationMarksForChar(marksAt);
}

function presentNodeMark(pos: number, attr: 'char' | PronunciationAttr, event: MouseEvent) {
  // 右键：偏（已是偏则取消为无）
  event.preventDefault();
  event.stopPropagation();
  clearTextSelection();
  if (!deduceDraft.value) return;
  const marksAt = deduceDraft.value.marks[pos];
  if (attr !== 'char' && isAutoPronunciationAttr(marksAt, attr)) {
    setAutoPronunciationAttr(marksAt, attr, false);
  }
  marksAt[attr] = marksAt[attr] === 'present' ? 'absent' : 'present';
  if (attr === 'char') syncPronunciationMarksForChar(marksAt);
}

function onNodePointerDown(event: MouseEvent) {
  // 右键按下时禁止浏览器选中文字
  if (event.button === 2) {
    event.preventDefault();
    clearTextSelection();
  }
}

function setMark(pos: number, attr: 'char' | PronunciationAttr, mark: 'hit' | 'present' | 'absent') {
  if (!deduceDraft.value) return;
  const marksAt = deduceDraft.value.marks[pos];
  if (attr !== 'char') setAutoPronunciationAttr(marksAt, attr, false);
  marksAt[attr] = marksAt[attr] === mark ? null : mark;
  if (attr === 'char') syncPronunciationMarksForChar(marksAt);
}

function setTone(pos: number, toneVal: string) {
  if (!deduceDraft.value?.chars[pos]) return;
  const t = parseInt(toneVal, 10);
  deduceDraft.value.chars[pos].tone = t;
  const digit = charsToDigitPinyin(deduceDraft.value.chars);
  pinyinInput.value = digit;
  deduceDraft.value.pinyin = normalizePinyinInput(digit);
  askPersistPinyin(deduceDraft.value.word, deduceDraft.value.pinyin);
}

function commitGuess(): boolean {
  if (!deduceDraft.value) return false;
  if (pinyinInput.value.trim()) {
    const chars = charsFromWordPinyin(deduceDraft.value.word, pinyinInput.value);
    if (!chars) {
      setMsg('拼音音节数与字数不一致，请先修正读音', 'err');
      return false;
    }
    deduceDraft.value.chars = chars;
    deduceDraft.value.pinyin = normalizePinyinInput(pinyinInput.value);
  }
  const payload: DeduceGuess = {
    word: deduceDraft.value.word,
    pinyin: deduceDraft.value.pinyin,
    chars: deduceDraft.value.chars.map((ch) => ({ ...ch })),
    marks: finalizeMarks(deduceDraft.value.marks),
  };
  const editIndex = deduceDraft.value.editIndex;
  if (Number.isInteger(editIndex) && editIndex! >= 0 && editIndex! < deduceGuesses.value.length) {
    deduceGuesses.value[editIndex!] = payload;
  } else {
    const existing = deduceGuesses.value.findIndex((g) => g.word === payload.word);
    if (existing >= 0) deduceGuesses.value.splice(existing, 1);
    deduceGuesses.value.push(payload);
  }
  deduceDraft.value = null;
  wordInput.value = '';
  showPinyinBar.value = false;
  setMsg(Number.isInteger(editIndex) ? '已更新筛选条件' : '已加入筛选条件', 'ok');
  return true;
}

function editGuess(idx: number) {
  const g = deduceGuesses.value[idx];
  if (!g) return;
  deduceDraft.value = {
    word: g.word,
    pinyin: g.pinyin,
    chars: g.chars.map((ch) => ({ ...ch })),
    marks: g.marks.map((m) => ({ ...m })),
    editIndex: idx,
  };
  wordInput.value = g.word;
  showPinyinBar.value = true;
  pinyinInput.value = formatPinyinAsDigit(g.pinyin) || charsToDigitPinyin(g.chars);
  setMsg('正在修改该条：可改读音/标记，确认后更新筛选', 'warn');
}

function removeGuess(idx: number) {
  deduceGuesses.value.splice(idx, 1);
}

function clearAll() {
  deduceGuesses.value = [];
  deduceDraft.value = null;
  wordInput.value = '';
  showPinyinBar.value = false;
  ocrLastRaw.value = '';
  ocrLastMeta.value = '';
  ocrStreamText.value = '';
  cancelOcrReview();
  setMsg('');
}

async function copyDebug() {
  if (!deduceGuesses.value.length && !(deduceDraft.value && guessHasMark(deduceDraft.value))) {
    setMsg('暂无猜测记录可复制', 'warn');
    return;
  }
  const dump = buildDeduceDebugDump(props.index, deduceGuesses.value, deduceDraft.value);
  try {
    await writeText(dump);
    setMsg(`已复制调试信息（${deduceGuesses.value.length} 条猜测，候选 ${results.value.length}）`, 'ok');
  } catch {
    setMsg('复制失败', 'err');
  }
}

async function copyAllCandidates() {
  if (!results.value.length) {
    setMsg('暂无待选成语可复制', 'warn');
    return;
  }
  const text = results.value.map((item) => item.word).join('\n');
  try {
    await writeText(text);
    setMsg(`已复制 ${results.value.length} 条待选成语`, 'ok');
  } catch {
    setMsg('复制失败', 'err');
  }
}

function applyDebugPaste() {
  const parsed = applyDeduceDebugPaste(debugPaste.value);
  if (!parsed) {
    debugPasteMsg.value = '未能解析调试数据';
    debugPasteMsgClass.value = 'err';
    return;
  }
  deduceGuesses.value = parsed.guesses;
  deduceDraft.value = parsed.draft;
  wordInput.value = parsed.draft?.word || '';
  showPinyinBar.value = !!parsed.draft;
  if (parsed.draft) {
    pinyinInput.value = formatPinyinAsDigit(parsed.draft.pinyin) || charsToDigitPinyin(parsed.draft.chars);
  }
  const failHint = parsed.failed.length ? `；第 ${parsed.failed.join(',')} 条解析失败已跳过` : '';
  const text = `已还原 ${parsed.guesses.length} 条猜测${parsed.draft ? ' + 草稿' : ''}，候选 ${results.value.length} 条${failHint}`;
  debugPasteMsg.value = text;
  debugPasteMsgClass.value = 'ok';
  setMsg(text, 'ok');
}

async function onCandidateCopy(word: string) {
  try {
    await writeText(word);
    setMsg(`已复制「${word}」`, 'ok');
  } catch {
    setMsg('复制失败', 'err');
  }
}

async function onCandidateSelect(word: string) {
  loadWord(word);
  if (deduceDraft.value?.word === word) {
    setMsg(`已选中「${word}」`, 'ok');
  }
  await nextTick();
  // 草稿卡片可能刚挂载，再等一帧确保布局完成
  await new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));
  scrollToMarkPreview();
}

function scrollToMarkPreview() {
  const target = markPreviewRef.value;
  if (!target) return;
  const scroller = document.getElementById('app');
  if (scroller) {
    const offset = 12;
    const top =
      scroller.scrollTop +
      target.getBoundingClientRect().top -
      scroller.getBoundingClientRect().top -
      offset;
    scroller.scrollTo({ top: Math.max(0, top), behavior: 'smooth' });
    return;
  }
  target.scrollIntoView({ behavior: 'smooth', block: 'start' });
}

function markClass(state: string | null, isAuto = false) {
  if (!state) return '';
  return isAuto ? ` mark-${state} mark-auto` : ` mark-${state}`;
}

function attrMarkClass(marks: DeduceGuess['marks'][0] | undefined, attr: 'char' | PronunciationAttr) {
  if (!marks) return '';
  const state = marks[attr] || null;
  if (attr === 'char') return markClass(state);
  return markClass(state, isAutoPronunciationAttr(marks, attr));
}

async function persistOcrResult(params: {
  model: string;
  ok: boolean;
  error?: string;
  result?: BoardParseResult | null;
  rawResponse?: string;
}) {
  const entry = createHistoryEntry(params);
  ocrLastRaw.value = entry.rawResponse || params.error || '';
  ocrLastMeta.value = entry.ok
    ? `${entry.guessWords.length ? entry.guessWords.join('、') : '无成语'} · ${new Date(entry.createdAt).toLocaleTimeString('zh-CN', { hour12: false })}`
    : `失败 · ${params.error || '未知错误'}`;
  try {
    await appendOcrHistory(entry);
  } catch (e) {
    console.warn('保存识别历史失败', e);
  }
}

function onOcrStart() {
  ocrBusy.value = true;
  ocrStatusText.value = '正在读取图片…';
  ocrStreamText.value = '';
  setMsg('');
}

async function onOcrImage(payload: { base64: string; mime: string; previewUrl: string }) {
  const cfg = qwenConfigRef.value?.getConfig() || qwenConfig.value;
  if (!cfg.apiKey) {
    ocrBusy.value = false;
    ocrStatusText.value = '';
    setMsg('请先在上方填写千问 API Key', 'err');
    URL.revokeObjectURL(payload.previewUrl);
    return;
  }
  if (!cfg.model) {
    ocrBusy.value = false;
    ocrStatusText.value = '';
    setMsg('请先选择或填写模型', 'err');
    URL.revokeObjectURL(payload.previewUrl);
    return;
  }

  const useStream = ocrShowStream.value;
  ocrBusy.value = true;
  ocrStatusText.value = useStream ? '千问流式识别中…' : '千问识别中…';
  ocrStreamText.value = '';
  if (ocrPreviewUrl.value) URL.revokeObjectURL(ocrPreviewUrl.value);
  ocrPreviewUrl.value = payload.previewUrl;
  setMsg('');
  await nextTick();

  if (useStream) {
    await attachOcrChunkListener();
  }

  try {
    const result = await parseGuessBoard({
      imageB64: payload.base64,
      mime: payload.mime,
      apiKey: cfg.apiKey,
      keyPlan: cfg.keyPlan || 'dashscope',
      model: cfg.model,
      prompt: cfg.prompt,
      stream: useStream,
    });
    ocrWarnings.value = result.warnings || [];
    ocrReviewRows.value = boardResultToReviewRows(result);
    if (result.raw_response) {
      ocrStreamText.value = result.raw_response;
    }
    await persistOcrResult({
      model: cfg.model,
      ok: true,
      result,
      rawResponse: result.raw_response,
    });
    if (!result.guesses.length) {
      setMsg('未识别到猜测，请检查截图或调整 Prompt', 'warn');
    } else {
      setMsg(`识别到 ${result.guesses.length} 条猜测，请复核后确认`, 'ok');
    }
  } catch (e) {
    const msg = e instanceof Error ? e.message : String(e);
    setMsg(msg, 'err');
    ocrReviewRows.value = null;
    await persistOcrResult({
      model: cfg.model,
      ok: false,
      error: msg,
      rawResponse: ocrStreamText.value || msg,
    });
  } finally {
    await detachOcrChunkListener();
    ocrBusy.value = false;
    ocrStatusText.value = '';
  }
}

function onQwenConfigChange(config: {
  apiKey: string;
  keyPlan: QwenKeyPlan;
  model: string;
  prompt: string;
}) {
  qwenConfig.value = config;
}

function onOcrError(message: string) {
  ocrBusy.value = false;
  ocrStatusText.value = '';
  setMsg(message, 'err');
}

function cancelOcrReview() {
  ocrReviewRows.value = null;
  ocrWarnings.value = [];
  if (ocrPreviewUrl.value) {
    URL.revokeObjectURL(ocrPreviewUrl.value);
    ocrPreviewUrl.value = '';
  }
}

function applyOcrReview(rows: OcrReviewRow[]) {
  const { guesses, skipped } = reviewRowsToDeduceGuesses(rows, props.wordMap);
  for (const g of guesses) {
    const existing = deduceGuesses.value.findIndex((item) => item.word === g.word);
    if (existing >= 0) deduceGuesses.value.splice(existing, 1);
    deduceGuesses.value.push(g);
  }
  cancelOcrReview();
  const skipHint = skipped.length ? `；词库未收录：${skipped.join('、')}` : '';
  setMsg(`已写入 ${guesses.length} 条猜测${skipHint}`, skipped.length ? 'warn' : 'ok');
}


defineExpose({ loadWord });
</script>

<template>
  <section class="panel" @contextmenu.prevent>
    <div v-if="persistDialog" class="lexicon-busy-overlay persist-dialog-overlay" role="dialog" aria-modal="true">
      <div class="persist-dialog-card">
        <h3>{{ persistDialog.mode === 'add' ? '写入词库？' : '更新词库读音？' }}</h3>
        <p v-if="persistDialog.mode === 'add'">
          词库中没有「{{ persistDialog.word }}」。是否按读音
          <code>{{ persistDialog.digit }}</code>
          新增？
        </p>
        <p v-else>
          是否将「{{ persistDialog.word }}」的读音从
          <code>{{ persistDialog.oldDigit }}</code>
          更新为
          <code>{{ persistDialog.digit }}</code>
          ？
        </p>
        <div class="persist-dialog-actions">
          <button type="button" @click="confirmPersistPinyin">
            {{ persistDialog.mode === 'add' ? '写入词库' : '更新词库' }}
          </button>
          <button type="button" class="ghost" @click="cancelPersistPinyin">只改展示，不写库</button>
        </div>
      </div>
    </div>

    <div v-if="lexiconBusy" class="lexicon-busy-overlay" role="status" aria-live="polite">
      <div class="lexicon-busy-card">
        <span class="image-intake-spinner" aria-hidden="true"></span>
        <div class="lexicon-busy-card-text">
          <strong>{{ lexiconBusyText || '词库处理中…' }}</strong>
          <span>界面可继续显示，请稍候</span>
        </div>
      </div>
    </div>

    <p class="hint panel-intro">
      输入已猜过的成语并标记。未标记在筛选时视为「无」。字标为存在时会自动带出声韵调（浅色）。
      读音在字下方输入框修改（如 <span class="example">jian4</span>），改完后可确认是否写入词库。
    </p>
    <p class="hint deduce-click-hint">
      <strong>标记操作：</strong>左键在「对 / 无」之间切换；右键标「偏」（再右键取消为「无」）。绿=对 · 紫=偏 · 灰=无。
    </p>

    <QwenConfig ref="qwenConfigRef" @change="onQwenConfigChange" />

    <ImageIntake
      :busy="ocrBusy"
      :status-text="ocrStatusText"
      :show-stream-preview="ocrShowStream"
      :stream-text="ocrStreamText"
      @update:show-stream-preview="onStreamPreviewChange"
      @start="onOcrStart"
      @image="onOcrImage"
      @error="onOcrError"
    />

    <details v-if="ocrLastRaw" class="ocr-raw-panel">
      <summary>
        本次识别返回内容
        <span v-if="ocrLastMeta" class="ocr-raw-meta">{{ ocrLastMeta }}</span>
      </summary>
      <pre class="ocr-raw-pre">{{ ocrLastRaw }}</pre>
    </details>

    <OcrReview
      v-if="ocrReviewRows"
      :rows="ocrReviewRows"
      :preview-url="ocrPreviewUrl"
      :warnings="ocrWarnings"
      @confirm="applyOcrReview"
      @cancel="cancelOcrReview"
    />

    <div class="row">
      <input v-model="wordInput" type="text" placeholder="输入已猜成语，如：弹无虚发" spellcheck="false" @keydown.enter="loadWord()" />
      <button type="button" @click="loadWord()">加载</button>
    </div>

    <div v-if="showPinyinBar" class="deduce-pinyin-bar">
      <label>读音（数字标调如 dan4 wu2 xu1 fa1）</label>
      <div class="row">
        <input v-model="pinyinInput" type="text" placeholder="dan4 wu2 xu1 fa1" spellcheck="false" />
        <button type="button" class="ghost" @click="applyPinyin">应用读音</button>
      </div>
    </div>

    <div v-if="deduceDraft" ref="markPreviewRef" class="deduce-guess-card">
      <div class="deduce-card-head">
        <div>
          <div class="deduce-word-title">{{ deduceDraft.word }}</div>
          <div class="deduce-pinyin-line">{{ charsToDigitPinyin(deduceDraft.chars) || deduceDraft.pinyin }}</div>
        </div>
        <label class="deduce-detail-toggle">
          <input v-model="showDetailPanel" type="checkbox" />
          显示详细面板
        </label>
      </div>
      <p class="hint deduce-click-hint compact">左键：对 ↔ 无 · 右键：偏</p>
      <div class="deduce-py-row preview-row">
        <div v-for="(ch, pos) in deduceDraft.chars" :key="pos" class="deduce-syl">
          <button
            type="button"
            class="deduce-node char-node"
            :class="attrMarkClass(deduceDraft.marks[pos], 'char')"
            title="左键：对/无 · 右键：偏"
            @mousedown="onNodePointerDown"
            @click="toggleNodeMark(pos, 'char')"
            @contextmenu="presentNodeMark(pos, 'char', $event)"
          >
            {{ ch.c }}
          </button>
          <input
            class="deduce-syl-input"
            type="text"
            spellcheck="false"
            :value="ch.py + toneDigit(ch.tone)"
            title="直接改读音，如 jian4"
            @change="onSyllableEdit(pos, ($event.target as HTMLInputElement).value)"
            @keydown.enter.prevent="onSyllableEdit(pos, ($event.target as HTMLInputElement).value)"
          />
          <div class="deduce-py-nodes">
            <button
              v-if="ch.sm"
              type="button"
              class="deduce-node"
              :class="attrMarkClass(deduceDraft.marks[pos], 'sm')"
              title="左键：对/无 · 右键：偏"
              @mousedown="onNodePointerDown"
              @click="toggleNodeMark(pos, 'sm')"
              @contextmenu="presentNodeMark(pos, 'sm', $event)"
            >
              {{ ch.sm }}
            </button>
            <button
              type="button"
              class="deduce-node"
              :class="attrMarkClass(deduceDraft.marks[pos], 'ym')"
              title="左键：对/无 · 右键：偏"
              @mousedown="onNodePointerDown"
              @click="toggleNodeMark(pos, 'ym')"
              @contextmenu="presentNodeMark(pos, 'ym', $event)"
            >
              {{ ch.ym || ch.py }}
            </button>
            <button
              type="button"
              class="deduce-node"
              :class="attrMarkClass(deduceDraft.marks[pos], 'tone')"
              title="左键：对/无 · 右键：偏"
              @mousedown="onNodePointerDown"
              @click="toggleNodeMark(pos, 'tone')"
              @contextmenu="presentNodeMark(pos, 'tone', $event)"
            >
              {{ toneDigit(ch.tone) }}
            </button>
          </div>
        </div>
      </div>

      <div v-if="showDetailPanel" class="deduce-pos-grid">
        <div v-for="(ch, pos) in deduceDraft.chars" :key="'col-' + pos" class="deduce-pos-col">
          <div class="pos-label">第 {{ pos + 1 }} 字 · {{ ch.py }}{{ toneDigit(ch.tone) }}</div>
          <div v-for="attr in DEDUCE_ATTRS" :key="attr.key" class="deduce-attr">
            <div class="attr-name">
              {{ attr.label }}
              <span
                v-if="attr.key !== 'char' && isAutoPronunciationAttr(deduceDraft.marks[pos], attr.key)"
                class="auto-tag"
              >
                自动
              </span>
            </div>
            <div class="attr-val" :class="attrMarkClass(deduceDraft.marks[pos], attr.key)">
              {{ attr.get(ch) }}
            </div>
            <select
              v-if="attr.key === 'tone'"
              class="deduce-tone-select"
              :value="ch.tone"
              @change="setTone(pos, ($event.target as HTMLSelectElement).value)"
            >
              <option v-for="t in [1, 2, 3, 4, 5]" :key="t" :value="t">{{ t === 5 ? '轻' : t }}</option>
            </select>
            <div
              v-if="attr.key !== 'char' && isAutoPronunciationAttr(deduceDraft.marks[pos], attr.key)"
              class="attr-lock-hint"
            >
              随字自动（浅色）；点下方按钮可改为人选
            </div>
            <div class="mark-btns">
              <button
                type="button"
                :class="{
                  'active-hit':
                    deduceDraft.marks[pos]?.[attr.key] === 'hit' &&
                    (attr.key === 'char' || !isAutoPronunciationAttr(deduceDraft.marks[pos], attr.key)),
                }"
                @click="setMark(pos, attr.key, 'hit')"
              >
                对
              </button>
              <button
                type="button"
                :class="{
                  'active-present':
                    deduceDraft.marks[pos]?.[attr.key] === 'present' &&
                    (attr.key === 'char' || !isAutoPronunciationAttr(deduceDraft.marks[pos], attr.key)),
                }"
                @click="setMark(pos, attr.key, 'present')"
              >
                偏
              </button>
              <button
                type="button"
                :class="{
                  'active-absent':
                    deduceDraft.marks[pos]?.[attr.key] === 'absent' &&
                    (attr.key === 'char' || !isAutoPronunciationAttr(deduceDraft.marks[pos], attr.key)),
                }"
                @click="setMark(pos, attr.key, 'absent')"
              >
                无
              </button>
            </div>
          </div>
        </div>
      </div>
    </div>

    <div class="deduce-actions">
      <button type="button" :disabled="!deduceDraft" @click="commitGuess()">
        {{ Number.isInteger(deduceDraft?.editIndex) ? '更新本条并筛选' : '确认本条并筛选' }}
      </button>
      <button type="button" class="secondary" @click="clearAll">清空全部条件</button>
      <button type="button" class="ghost" @click="copyDebug">复制调试信息</button>
    </div>
    <div v-if="commitMsg" class="msg" :class="commitMsgClass">{{ commitMsg }}</div>

    <details class="debug-paste">
      <summary>粘贴调试信息还原筛选</summary>
      <textarea v-model="debugPaste" placeholder="在此粘贴调试信息…" spellcheck="false" />
      <div class="row">
        <button type="button" @click="applyDebugPaste">应用并筛选</button>
        <button type="button" class="ghost" @click="debugPaste = ''">清空输入</button>
      </div>
      <div v-if="debugPasteMsg" class="msg" :class="debugPasteMsgClass">{{ debugPasteMsg }}</div>
    </details>

    <div v-if="deduceGuesses.length" class="deduce-history">
      <h3>已记录猜测</h3>
      <div v-for="(g, idx) in deduceGuesses" :key="g.word + idx" class="deduce-history-item">
        <div class="deduce-card-head">
          <div class="deduce-word-title">{{ g.word }}</div>
          <div class="deduce-card-actions">
            <button type="button" class="ghost small" @click="editGuess(idx)">改标记</button>
            <button type="button" class="ghost small" @click="removeGuess(idx)">删除</button>
          </div>
        </div>
        <div class="deduce-py-row">
          <div v-for="(ch, pos) in g.chars" :key="pos" class="deduce-syl">
            <span class="deduce-node char-node" :class="attrMarkClass(g.marks[pos], 'char')">{{ ch.c }}</span>
            <div class="deduce-py-nodes">
              <span v-if="ch.sm" class="deduce-node" :class="attrMarkClass(g.marks[pos], 'sm')">{{ ch.sm }}</span>
              <span class="deduce-node" :class="attrMarkClass(g.marks[pos], 'ym')">{{ ch.ym || ch.py }}</span>
              <span class="deduce-node" :class="attrMarkClass(g.marks[pos], 'tone')">{{ toneDigit(ch.tone) }}</span>
            </div>
          </div>
        </div>
      </div>
    </div>

    <div v-if="activeGuesses.length" class="results-meta results-meta-row">
      <span>{{ resultsMeta }}</span>
      <button
        v-if="results.length"
        type="button"
        class="ghost small"
        @click="copyAllCandidates"
      >
        复制全部待选
      </button>
    </div>
    <IdiomResultList
      :items="displayResults"
      clickable
      :empty-text="activeGuesses.length ? '无匹配成语，请检查标记是否矛盾' : '添加猜测并标记后，这里显示符合条件的成语'"
      @copy="onCandidateCopy"
      @select="onCandidateSelect"
    />
    <p v-if="results.length > 200" class="hint">… 另有 {{ results.length - 200 }} 条未显示</p>
  </section>
</template>
