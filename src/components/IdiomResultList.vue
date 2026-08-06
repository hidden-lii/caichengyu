<script setup lang="ts">
import type { IndexedIdiom } from '../entity/idiom';

defineProps<{
  items: IndexedIdiom[];
  emptyText?: string;
  clickable?: boolean;
}>();

const emit = defineEmits<{
  select: [word: string];
  copy: [word: string];
}>();
</script>

<template>
  <ul class="result-list" :class="{ 'result-list-cards': clickable }">
    <li v-if="!items.length" class="empty">{{ emptyText || '无匹配成语' }}</li>
    <li
      v-for="item in items"
      :key="item.word"
      :class="{ 'deduce-candidate-card': clickable }"
    >
      <template v-if="clickable">
        <div class="candidate-card-main">
          <button
            type="button"
            class="candidate-word-btn"
            title="点击复制成语"
            @click="emit('copy', item.word)"
          >
            {{ item.word }}
          </button>
          <button
            type="button"
            class="ghost small candidate-select-btn"
            @click="emit('select', item.word)"
          >
            选中
          </button>
        </div>
        <div class="result-pinyin">{{ item.pinyin }}</div>
        <div v-if="item.explanation" class="result-exp">{{ item.explanation }}</div>
      </template>
      <template v-else>
        <div class="result-word">{{ item.word }}</div>
        <div class="result-pinyin">{{ item.pinyin }}</div>
        <div v-if="item.explanation" class="result-exp">{{ item.explanation }}</div>
      </template>
    </li>
  </ul>
</template>
