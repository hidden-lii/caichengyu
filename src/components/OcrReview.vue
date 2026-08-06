<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import type { OcrMarkState, OcrReviewRow } from '../entity/board';
import { DEDUCE_ATTRS } from '../engine/pinyin';

const props = defineProps<{
  rows: OcrReviewRow[];
  previewUrl?: string;
  warnings: string[];
}>();

const emit = defineEmits<{
  confirm: [rows: OcrReviewRow[]];
  cancel: [];
}>();

const localRows = ref<OcrReviewRow[]>([]);

watch(
  () => props.rows,
  (rows) => {
    localRows.value = rows.map((r) => ({
      ...r,
      marks: r.marks.map((m) => ({ ...m })),
    }));
  },
  { immediate: true, deep: true }
);

const selectedCount = computed(() => localRows.value.filter((r) => r.selected).length);

const ATTR_LABELS: Record<string, string> = {
  char: '字',
  sm: '声',
  ym: '韵',
  tone: '调',
};

function markClass(state: OcrMarkState | null) {
  return state ? ` mark-${state}` : '';
}

function cycleMark(rowIdx: number, colIdx: number, attr: keyof OcrReviewRow['marks'][0]) {
  const row = localRows.value[rowIdx];
  if (!row) return;
  const order: OcrMarkState[] = ['hit', 'present', 'absent'];
  const cur = row.marks[colIdx][attr];
  const next = order[(order.indexOf(cur) + 1) % order.length];
  row.marks[colIdx][attr] = next;
  if (attr === 'char' && (next === 'hit' || next === 'present')) {
    row.marks[colIdx].sm = 'absent';
    row.marks[colIdx].ym = 'absent';
    row.marks[colIdx].tone = 'absent';
  }
}

function setMark(
  rowIdx: number,
  colIdx: number,
  attr: keyof OcrReviewRow['marks'][0],
  mark: OcrMarkState
) {
  const row = localRows.value[rowIdx];
  if (!row) return;
  row.marks[colIdx][attr] = row.marks[colIdx][attr] === mark ? 'absent' : mark;
  if (attr === 'char' && (mark === 'hit' || mark === 'present')) {
    row.marks[colIdx].sm = 'absent';
    row.marks[colIdx].ym = 'absent';
    row.marks[colIdx].tone = 'absent';
  }
}

function onConfirm() {
  emit('confirm', localRows.value);
}
</script>

<template>
  <div class="ocr-review">
    <div class="ocr-review-head">
      <h3>识别结果复核</h3>
      <p class="hint">确认字与四维标记后写入筛选。词库中不存在的成语需先手动录入。</p>
    </div>

    <div v-if="previewUrl" class="ocr-preview">
      <img :src="previewUrl" alt="截图预览" />
    </div>

    <ul v-if="warnings.length" class="ocr-warnings">
      <li v-for="(w, i) in warnings" :key="i">{{ w }}</li>
    </ul>

    <div v-if="!localRows.length" class="ocr-empty">未识别到猜测行，请换一张更清晰的截图。</div>

    <div v-for="(row, ri) in localRows" :key="ri" class="ocr-review-row">
      <label class="ocr-row-select">
        <input v-model="row.selected" type="checkbox" />
        <input v-model="row.word" type="text" class="ocr-word-input" spellcheck="false" placeholder="四字成语" />
      </label>

      <div class="ocr-mark-table">
        <div class="ocr-mark-header">
          <span></span>
          <span v-for="ci in 4" :key="'h' + ci">第 {{ ci }} 字</span>
        </div>
        <div v-for="attr in DEDUCE_ATTRS" :key="attr.key" class="ocr-mark-line">
          <span class="ocr-mark-label">{{ ATTR_LABELS[attr.key] }}</span>
          <button
            v-for="ci in 4"
            :key="attr.key + ci"
            type="button"
            class="deduce-node ocr-mark-cell"
            :class="markClass(row.marks[ci - 1]?.[attr.key] || 'absent')"
            @click="cycleMark(ri, ci - 1, attr.key)"
          >
            {{ attr.key === 'char' ? row.word[ci - 1] || '?' : ATTR_LABELS[attr.key] }}
          </button>
        </div>
      </div>

      <details class="ocr-mark-detail">
        <summary>逐格改标记</summary>
        <div class="ocr-attr-grid">
          <div v-for="(marks, ci) in row.marks" :key="'detail-' + ci" class="deduce-pos-col">
            <div class="pos-label">第 {{ ci + 1 }} 字 · {{ row.word[ci] || '?' }}</div>
            <div v-for="attr in DEDUCE_ATTRS" :key="attr.key" class="deduce-attr">
              <div class="attr-name">{{ attr.label }}</div>
              <div class="attr-val" :class="markClass(marks[attr.key])">
                {{ attr.key === 'char' ? row.word[ci] || '?' : ATTR_LABELS[attr.key] }}
              </div>
              <div class="mark-btns">
                <button
                  type="button"
                  :class="{ 'active-hit': marks[attr.key] === 'hit' }"
                  @click="setMark(ri, ci, attr.key, 'hit')"
                >
                  对
                </button>
                <button
                  type="button"
                  :class="{ 'active-present': marks[attr.key] === 'present' }"
                  @click="setMark(ri, ci, attr.key, 'present')"
                >
                  偏
                </button>
                <button
                  type="button"
                  :class="{ 'active-absent': marks[attr.key] === 'absent' }"
                  @click="setMark(ri, ci, attr.key, 'absent')"
                >
                  无
                </button>
              </div>
            </div>
          </div>
        </div>
      </details>
    </div>

    <div class="ocr-review-actions">
      <button type="button" :disabled="!selectedCount" @click="onConfirm">确认写入 {{ selectedCount }} 条猜测</button>
      <button type="button" class="ghost" @click="emit('cancel')">取消</button>
    </div>
  </div>
</template>
