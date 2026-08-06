import { ref, computed } from 'vue';
import { loadAllIdioms } from '../api/idiom';
import type { IdiomEntry, IndexedIdiom } from '../entity/idiom';
import { buildIndexFromDbChunked, buildWordMap, getBlindPool } from '../engine/index';

const index = ref<IndexedIdiom[]>([]);
const wordMap = ref<Map<string, IndexedIdiom>>(new Map());
const skipped = ref(0);
const loading = ref(false);
const loadingText = ref('');
const error = ref<string | null>(null);
/** 首次加载完成后为 true；刷新词库时不卸载面板，避免丢失编辑状态 */
const ready = ref(false);

function yieldToUi(): Promise<void> {
  return new Promise((resolve) => {
    requestAnimationFrame(() => {
      requestAnimationFrame(() => resolve());
    });
  });
}

export function useLexicon() {
  const blindPool = computed(() => getBlindPool(index.value));
  const total = computed(() => index.value.length);
  const blindCount = computed(() => blindPool.value.length);

  async function reload(statusText = '正在加载词库…') {
    loading.value = true;
    loadingText.value = statusText;
    error.value = null;
    // 先让出两帧，确保转圈已经画上
    await yieldToUi();
    try {
      loadingText.value = '正在读取词库…';
      const items: IdiomEntry[] = await loadAllIdioms();
      await yieldToUi();
      loadingText.value = `正在建立索引（${items.length} 条）…`;
      const built = await buildIndexFromDbChunked(items, async (done, totalCount) => {
        loadingText.value = `正在建立索引（${done}/${totalCount}）…`;
        await yieldToUi();
      });
      index.value = built.result;
      skipped.value = built.skipped;
      wordMap.value = buildWordMap(built.result);
      ready.value = true;
    } catch (e) {
      error.value = e instanceof Error ? e.message : String(e);
    } finally {
      loading.value = false;
      loadingText.value = '';
    }
  }

  return {
    index,
    wordMap,
    skipped,
    loading,
    loadingText,
    ready,
    error,
    blindPool,
    total,
    blindCount,
    reload,
  };
}
