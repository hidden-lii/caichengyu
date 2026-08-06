<script setup lang="ts">
import { computed, nextTick, onUnmounted, ref } from 'vue';
import { writeText } from '@tauri-apps/api/clipboard';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { getSetting, parseGuessBoard, setSetting } from '../api/idiom';
import type { BoardParseResult, OcrReviewRow } from '../entity/board';
import type { DeduceGuess, IndexedIdiom } from '../entity/idiom';
import { applyDeduceDebugPaste, buildDeduceDebugDump } from '../engine/debug';
import {
  createEmptyMarks,
  deduceFilter,
  finalizeMarks,
  getActiveDeduceGuesses,
  guessHasMark,
  nextMark,
} from '../engine/deduce';
import { boardResultToReviewRows, reviewRowsToDeduceGuesses } from '../engine/ocr';
import { appendOcrHistory, createHistoryEntry } from '../engine/ocrHistory';
import {
  charOwnsPronunciation,
  charsFromWordPinyin,
  charsToDigitPinyin,
  formatPinyinAsDigit,
  normalizePinyinInput,
  syncPronunciationMarksForChar,
  toneDigit,
  DEDUCE_ATTRS,
} from '../engine/pinyin';
import { QWEN_SETTING_KEYS, type QwenKeyPlan } from '../engine/qwen';
import ImageIntake from './ImageIntake.vue';
import IdiomResultList from './IdiomResultList.vue';
import OcrReview from './OcrReview.vue';
import QwenConfig from './QwenConfig.vue';

const props = defineProps<{
  index: IndexedIdiom[];
  wordMap: Map<string, IndexedIdiom>;
}>();

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
  deduceDraft.value = {
    word,
    pinyin: normalizePinyinInput(pinyin),
    chars,
    marks,
    ...(editIndex !== undefined ? { editIndex } : {}),
  };
  setMsg(
    Number.isInteger(editIndex) ? '读音已应用，可继续改标记后点「更新本条并筛选」' : '读音已应用，请标记后点「确认本条并筛选」',
    'ok'
  );
}

function toggleNodeMark(pos: number, attr: keyof DeduceGuess['marks'][0]) {
  if (!deduceDraft.value) return;
  const marksAt = deduceDraft.value.marks[pos];
  if (attr !== 'char' && charOwnsPronunciation(marksAt)) return;
  marksAt[attr] = nextMark(marksAt[attr]);
  if (attr === 'char') syncPronunciationMarksForChar(marksAt);
}

function setMark(pos: number, attr: keyof DeduceGuess['marks'][0], mark: 'hit' | 'present' | 'absent') {
  if (!deduceDraft.value) return;
  const marksAt = deduceDraft.value.marks[pos];
  if (attr !== 'char' && charOwnsPronunciation(marksAt)) return;
  marksAt[attr] = marksAt[attr] === mark ? null : mark;
  if (attr === 'char') syncPronunciationMarksForChar(marksAt);
}

function setTone(pos: number, toneVal: string) {
  if (!deduceDraft.value?.chars[pos]) return;
  const t = parseInt(toneVal, 10);
  deduceDraft.value.chars[pos].tone = t;
  pinyinInput.value = charsToDigitPinyin(deduceDraft.value.chars);
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

function onCandidateSelect(word: string) {
  loadWord(word);
  if (deduceDraft.value?.word === word) {
    setMsg(`已选中「${word}」`, 'ok');
  }
}

function markClass(state: string | null) {
  return state ? ` mark-${state}` : '';
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
  <section class="panel">
    <p class="hint panel-intro">
      输入已猜过的成语，为每字的字/声母/韵母/声调标记：绿=对 · 紫=偏 · 灰=无。未标记在筛选时视为「无」。
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

    <div v-if="deduceDraft" class="deduce-guess-card">
      <div class="deduce-card-head">
        <div>
          <div class="deduce-word-title">{{ deduceDraft.word }}</div>
          <div class="deduce-pinyin-line">{{ charsToDigitPinyin(deduceDraft.chars) || deduceDraft.pinyin }}</div>
        </div>
      </div>
      <div class="deduce-py-row preview-row">
        <div v-for="(ch, pos) in deduceDraft.chars" :key="pos" class="deduce-syl">
          <button
            type="button"
            class="deduce-node char-node"
            :class="markClass(deduceDraft.marks[pos]?.char || null)"
            @click="toggleNodeMark(pos, 'char')"
          >
            {{ ch.c }}
          </button>
          <div class="deduce-py-nodes">
            <button
              v-if="ch.sm"
              type="button"
              class="deduce-node"
              :class="[markClass(deduceDraft.marks[pos]?.sm || null), { 'py-locked': charOwnsPronunciation(deduceDraft.marks[pos]) }]"
              :disabled="charOwnsPronunciation(deduceDraft.marks[pos])"
              @click="toggleNodeMark(pos, 'sm')"
            >
              {{ ch.sm }}
            </button>
            <button
              type="button"
              class="deduce-node"
              :class="[markClass(deduceDraft.marks[pos]?.ym || null), { 'py-locked': charOwnsPronunciation(deduceDraft.marks[pos]) }]"
              :disabled="charOwnsPronunciation(deduceDraft.marks[pos])"
              @click="toggleNodeMark(pos, 'ym')"
            >
              {{ ch.ym || ch.py }}
            </button>
            <button
              type="button"
              class="deduce-node"
              :class="[markClass(deduceDraft.marks[pos]?.tone || null), { 'py-locked': charOwnsPronunciation(deduceDraft.marks[pos]) }]"
              :disabled="charOwnsPronunciation(deduceDraft.marks[pos])"
              @click="toggleNodeMark(pos, 'tone')"
            >
              {{ toneDigit(ch.tone) }}
            </button>
          </div>
        </div>
      </div>

      <div class="deduce-pos-grid">
        <div v-for="(ch, pos) in deduceDraft.chars" :key="'col-' + pos" class="deduce-pos-col">
          <div class="pos-label">第 {{ pos + 1 }} 字 · {{ ch.py }}{{ toneDigit(ch.tone) }}</div>
          <div
            v-for="attr in DEDUCE_ATTRS"
            :key="attr.key"
            class="deduce-attr"
            :class="{ 'attr-locked': attr.key !== 'char' && charOwnsPronunciation(deduceDraft.marks[pos]) }"
          >
            <div class="attr-name">{{ attr.label }}</div>
            <div class="attr-val" :class="markClass(deduceDraft.marks[pos]?.[attr.key] || null)">
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
            <div v-if="attr.key !== 'char' && charOwnsPronunciation(deduceDraft.marks[pos])" class="attr-lock-hint">
              随字（该字声韵调不参与筛选）
            </div>
            <div v-else class="mark-btns">
              <button
                type="button"
                :class="{ 'active-hit': deduceDraft.marks[pos]?.[attr.key] === 'hit' }"
                @click="setMark(pos, attr.key, 'hit')"
              >
                对
              </button>
              <button
                type="button"
                :class="{ 'active-present': deduceDraft.marks[pos]?.[attr.key] === 'present' }"
                @click="setMark(pos, attr.key, 'present')"
              >
                偏
              </button>
              <button
                type="button"
                :class="{ 'active-absent': deduceDraft.marks[pos]?.[attr.key] === 'absent' }"
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
            <span class="deduce-node char-node" :class="markClass(g.marks[pos]?.char || null)">{{ ch.c }}</span>
            <div class="deduce-py-nodes">
              <span v-if="ch.sm" class="deduce-node" :class="markClass(g.marks[pos]?.sm || null)">{{ ch.sm }}</span>
              <span class="deduce-node" :class="markClass(g.marks[pos]?.ym || null)">{{ ch.ym || ch.py }}</span>
              <span class="deduce-node" :class="markClass(g.marks[pos]?.tone || null)">{{ toneDigit(ch.tone) }}</span>
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
