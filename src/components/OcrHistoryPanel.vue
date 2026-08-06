<script setup lang="ts">
import { onMounted, ref } from 'vue';
import {
  clearOcrHistory,
  formatHistoryTime,
  loadOcrHistory,
  removeOcrHistory,
  type OcrHistoryEntry,
} from '../engine/ocrHistory';

const list = ref<OcrHistoryEntry[]>([]);
const loading = ref(true);
const msg = ref('');
const msgClass = ref('');

async function reload() {
  loading.value = true;
  try {
    list.value = await loadOcrHistory();
  } catch (e) {
    msg.value = '加载历史失败：' + (e instanceof Error ? e.message : String(e));
    msgClass.value = 'err';
  } finally {
    loading.value = false;
  }
}

async function onRemove(id: string) {
  list.value = await removeOcrHistory(id);
  msg.value = '已删除该条记录';
  msgClass.value = 'ok';
}

async function onClearAll() {
  if (!list.value.length) return;
  if (!window.confirm(`确认清除全部 ${list.value.length} 条识别记录？`)) return;
  list.value = await clearOcrHistory();
  msg.value = '已清空全部记录';
  msgClass.value = 'ok';
}

onMounted(() => {
  void reload();
});
</script>

<template>
  <section class="panel ocr-history-panel">
    <div class="ocr-history-head">
      <div>
        <h3>识别返回记录</h3>
        <p class="hint">每次千问识别的原始返回会保存在此，便于排查与复盘。</p>
      </div>
      <button type="button" class="ghost" :disabled="!list.length" @click="onClearAll">全部清除</button>
    </div>

    <div v-if="msg" class="msg" :class="msgClass">{{ msg }}</div>
    <div v-if="loading" class="ocr-history-empty">加载中…</div>
    <div v-else-if="!list.length" class="ocr-history-empty">暂无识别记录</div>

    <article v-for="item in list" :key="item.id" class="ocr-history-item">
      <div class="ocr-history-item-head">
        <div>
          <span class="ocr-history-badge" :class="item.ok ? 'ok' : 'err'">
            {{ item.ok ? '成功' : '失败' }}
          </span>
          <span class="ocr-history-time">{{ formatHistoryTime(item.createdAt) }}</span>
          <span v-if="item.model" class="ocr-history-model">{{ item.model }}</span>
        </div>
        <button type="button" class="ghost danger-ghost" @click="onRemove(item.id)">删除</button>
      </div>

      <p v-if="item.guessWords?.length" class="ocr-history-words">
        识别成语：{{ item.guessWords.join('、') }}
      </p>
      <p v-else-if="item.ok" class="ocr-history-words muted">未识别到成语</p>
      <p v-if="item.error" class="ocr-history-error">{{ item.error }}</p>
      <ul v-if="item.warnings?.length" class="ocr-warnings">
        <li v-for="(w, i) in item.warnings" :key="i">{{ w }}</li>
      </ul>

      <details class="ocr-raw-details">
        <summary>原始返回内容</summary>
        <pre class="ocr-raw-pre">{{ item.rawResponse || '（空）' }}</pre>
      </details>
    </article>
  </section>
</template>
