<script setup lang="ts">
import { onMounted, ref } from 'vue';
import BlindPanel from '../components/BlindPanel.vue';
import DeducePanel from '../components/DeducePanel.vue';
import LexiconPanel from '../components/LexiconPanel.vue';
import OcrHistoryPanel from '../components/OcrHistoryPanel.vue';
import QueryPanel from '../components/QueryPanel.vue';
import { useLexicon } from '../composables/useLexicon';
import appLogo from '../assets/logo.png';

type TabKey = 'blind' | 'deduce' | 'query' | 'ocr-history' | 'settings';

const activeTab = ref<TabKey>('blind');
const showSettings = ref(false);
const queryPanel = ref<InstanceType<typeof QueryPanel> | null>(null);
const deducePanel = ref<InstanceType<typeof DeducePanel> | null>(null);
const pendingQuery = ref('');
const pendingDeduceWord = ref('');

const { index, wordMap, skipped, loading, loadingText, ready, error, blindPool, total, blindCount, reload } =
  useLexicon();

const tabs = [
  { key: 'blind' as const, label: '随机成语' },
  { key: 'deduce' as const, label: '猜成语' },
  { key: 'query' as const, label: '筛选成语' },
  { key: 'ocr-history' as const, label: '识别记录' },
];

onMounted(async () => {
  await reload();
});

function switchTab(key: TabKey) {
  activeTab.value = key;
  showSettings.value = false;
  if (key === 'query' && pendingQuery.value) {
    setTimeout(() => {
      queryPanel.value?.setQuery(pendingQuery.value);
    }, 0);
  }
  if (key === 'deduce' && pendingDeduceWord.value) {
    const w = pendingDeduceWord.value;
    pendingDeduceWord.value = '';
    setTimeout(() => deducePanel.value?.loadWord(w), 0);
  }
}

function onFillQuery(q: string) {
  pendingQuery.value = q;
  switchTab('query');
}

function onFillDeduce(word: string) {
  pendingDeduceWord.value = word;
  switchTab('deduce');
}

function statusText() {
  if (loading.value) return loadingText.value || '词库加载中…';
  if (error.value) return `加载失败：${error.value}`;
  if (!total.value) return '词库为空，请在设置中导入';
  let text = `词库已就绪（${total.value} 条`;
  if (skipped.value) text += `，跳过 ${skipped.value} 条`;
  text += ` · 可随机 ${blindCount.value} 条）`;
  return text;
}
</script>

<template>
  <div class="wrap">
    <header class="app-header">
      <div class="brand">
        <img class="brand-logo" :src="appLogo" alt="猜成语" width="48" height="48" />
        <div class="brand-text">
          <h1>猜成语</h1>
          <p class="subtitle">本地词库 · 标记筛选 · 拼音通配查询</p>
        </div>
      </div>
      <button type="button" class="ghost settings-btn" @click="showSettings = !showSettings">
        {{ showSettings ? '返回' : '词库设置' }}
      </button>
    </header>

    <div class="status" :class="{ ok: total && !error, warn: !total || error, loading: loading }">
      <span v-if="loading" class="image-intake-spinner sm" aria-hidden="true"></span>
      <span>{{ statusText() }}</span>
    </div>

    <nav v-if="!showSettings" class="tab-bar">
      <button
        v-for="tab in tabs"
        :key="tab.key"
        type="button"
        class="tab-btn"
        :class="{ active: activeTab === tab.key }"
        @click="switchTab(tab.key)"
      >
        {{ tab.label }}
      </button>
    </nav>

    <main class="main-content">
      <LexiconPanel v-if="showSettings" />
      <template v-else-if="ready">
        <BlindPanel
          v-show="activeTab === 'blind'"
          :index="index"
          :blind-pool="blindPool"
          @fill-query="onFillQuery"
          @fill-deduce="onFillDeduce"
        />
        <DeducePanel
          v-show="activeTab === 'deduce'"
          ref="deducePanel"
          :index="index"
          :word-map="wordMap"
        />
        <QueryPanel
          v-show="activeTab === 'query'"
          ref="queryPanel"
          :index="index"
          :initial-query="pendingQuery"
        />
        <OcrHistoryPanel v-if="activeTab === 'ocr-history'" />
      </template>
      <div v-else class="loading">
        <span class="image-intake-spinner" aria-hidden="true"></span>
        <span>{{ loadingText || '正在加载词库并建立索引…' }}</span>
      </div>
    </main>
  </div>
</template>
