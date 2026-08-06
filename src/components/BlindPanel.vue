<script setup lang="ts">
import { ref } from 'vue';
import type { IndexedIdiom } from '../entity/idiom';
import { buildQueryFromIdiom } from '../engine/index';

const props = defineProps<{
  index: IndexedIdiom[];
  blindPool: IndexedIdiom[];
}>();

const emit = defineEmits<{
  fillQuery: [query: string];
  fillDeduce: [word: string];
}>();

const current = ref<IndexedIdiom | null>(null);

function pickRandom() {
  if (!props.blindPool.length) {
    current.value = null;
    return;
  }
  current.value = props.blindPool[Math.floor(Math.random() * props.blindPool.length)]!;
}

function fillQuery() {
  if (!current.value) return;
  emit('fillQuery', buildQueryFromIdiom(current.value));
}

function fillDeduce() {
  if (!current.value) return;
  emit('fillDeduce', current.value.word);
}
</script>

<template>
  <section class="panel">
    <p class="hint panel-intro">
      随机展示一个词库中的成语（四字的声母、韵母、音调、汉字均互不相同），可作为猜题起点。
    </p>
    <p class="hint">{{ index.length ? `词库共 ${index.length} 条 · 可随机 ${blindPool.length} 条` : '请先加载词库' }}</p>
    <div class="row">
      <button type="button" @click="pickRandom">随机一个</button>
      <button type="button" class="ghost" :disabled="!current" @click="fillQuery">填入筛选框</button>
      <button type="button" class="ghost" :disabled="!current" @click="fillDeduce">填入猜成语</button>
    </div>
    <div class="blind-card" :class="{ empty: !current }">
      <template v-if="current">
        <div class="blind-show">
          <div class="result-word">{{ current.word }}</div>
          <div class="result-pinyin">{{ current.pinyin }}</div>
          <div v-if="current.explanation" class="result-exp">{{ current.explanation }}</div>
        </div>
        <div class="clue-grid">
          <div v-for="(ch, i) in current.chars" :key="i" class="clue-cell">
            <div class="pos">第 {{ i + 1 }} 字</div>
            <div class="val">{{ ch.c }}</div>
            <div class="val muted">{{ ch.sm || '零' }} · {{ ch.ym }} · {{ ch.tone }}</div>
          </div>
        </div>
      </template>
      <template v-else>点击「随机一个」开始</template>
    </div>
  </section>
</template>
