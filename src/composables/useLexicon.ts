import { ref, computed } from 'vue';
import { loadAllIdioms } from '../api/idiom';
import type { IdiomEntry, IndexedIdiom } from '../entity/idiom';
import { buildIndexFromDb, buildWordMap, getBlindPool } from '../engine/index';

const index = ref<IndexedIdiom[]>([]);
const wordMap = ref<Map<string, IndexedIdiom>>(new Map());
const skipped = ref(0);
const loading = ref(false);
const error = ref<string | null>(null);

export function useLexicon() {
  const blindPool = computed(() => getBlindPool(index.value));
  const total = computed(() => index.value.length);
  const blindCount = computed(() => blindPool.value.length);

  async function reload() {
    loading.value = true;
    error.value = null;
    try {
      const items: IdiomEntry[] = await loadAllIdioms();
      const built = buildIndexFromDb(items);
      index.value = built.result;
      skipped.value = built.skipped;
      wordMap.value = buildWordMap(built.result);
    } catch (e) {
      error.value = e instanceof Error ? e.message : String(e);
    } finally {
      loading.value = false;
    }
  }

  return {
    index,
    wordMap,
    skipped,
    loading,
    error,
    blindPool,
    total,
    blindCount,
    reload,
  };
}
