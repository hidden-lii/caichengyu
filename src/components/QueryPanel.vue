<script setup lang="ts">
import { ref } from 'vue';
import type { IndexedIdiom } from '../entity/idiom';
import { search } from '../engine/query';
import IdiomResultList from './IdiomResultList.vue';

const props = defineProps<{
  index: IndexedIdiom[];
  initialQuery?: string;
}>();

const queryInput = ref(props.initialQuery || '');
const meta = ref('');
const results = ref<IndexedIdiom[]>([]);
const error = ref<string | null>(null);

function runSearch() {
  const { error: err, results: list } = search(props.index, queryInput.value);
  error.value = err;
  results.value = list;
  meta.value = err || `共 ${list.length} 条`;
}

function setQuery(q: string) {
  queryInput.value = q;
  runSearch();
}

defineExpose({ setQuery });
</script>

<template>
  <section class="panel">
    <label for="query-input">查询串（示例：<span class="example">d?4 wu ? fa1</span> → 弹无虚发）</label>
    <div class="row">
      <input
        id="query-input"
        v-model="queryInput"
        type="text"
        placeholder="d?4 wu ? fa1"
        spellcheck="false"
        @keydown.enter="runSearch"
      />
      <button type="button" @click="runSearch">查询</button>
    </div>
    <p class="hint">
      支持：汉字 · <span class="example">?</span> 任意字 · <span class="example">fa1</span> 完整拼音+调 ·
      <span class="example">d?4</span> 声母+通配 · <span class="example">?an4</span> 韵母+调
    </p>
    <div class="results-meta" :class="{ err: error }">{{ meta }}</div>
    <IdiomResultList :items="results" :empty-text="error || '无匹配成语'" />
  </section>
</template>
