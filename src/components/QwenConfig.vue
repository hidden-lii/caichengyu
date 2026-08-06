<script setup lang="ts">
import { computed, nextTick, onMounted, ref, watch } from 'vue';
import {
  getQwenDefaultPrompt,
  getQwenPromptSchema,
  getSetting,
  listQwenModels,
  setSetting,
} from '../api/idiom';
import {
  DEFAULT_QWEN_PROMPT,
  FALLBACK_QWEN_MODELS,
  parseKeyPlan,
  QWEN_KEY_PLAN_OPTIONS,
  QWEN_SETTING_KEYS,
  type QwenKeyPlan,
} from '../engine/qwen';

const apiKey = ref('');
const keyPlan = ref<QwenKeyPlan>('dashscope');
const model = ref(FALLBACK_QWEN_MODELS.dashscope[0]);
const prompt = ref(DEFAULT_QWEN_PROMPT);
const schemaSuffix = ref('');
const models = ref<string[]>([...FALLBACK_QWEN_MODELS.dashscope]);
const showKey = ref(false);
const loadingModels = ref(false);
const saving = ref(false);
const msg = ref('');
const msgClass = ref('');
const detailsEl = ref<HTMLDetailsElement | null>(null);

const emit = defineEmits<{
  change: [config: { apiKey: string; keyPlan: QwenKeyPlan; model: string; prompt: string }];
}>();

const ready = computed(() => !!apiKey.value.trim() && !!model.value.trim());

const keyPlanHint = computed(
  () => QWEN_KEY_PLAN_OPTIONS.find((o) => o.value === keyPlan.value)?.hint || '',
);

const apiKeyPlaceholder = computed(() =>
  keyPlan.value === 'dashscope' ? 'sk-...' : 'sk-sp-...',
);

function fallbackModels(plan: QwenKeyPlan = keyPlan.value): string[] {
  return [...FALLBACK_QWEN_MODELS[plan]];
}

function setMsg(text: string, cls = 'ok') {
  msg.value = text;
  msgClass.value = cls;
}

function emitChange() {
  emit('change', {
    apiKey: apiKey.value.trim(),
    keyPlan: keyPlan.value,
    model: model.value.trim(),
    prompt: prompt.value,
  });
}

function parseSavedModels(raw: string | null): string[] {
  if (!raw?.trim()) return [];
  try {
    const parsed = JSON.parse(raw) as unknown;
    if (!Array.isArray(parsed)) return [];
    return parsed.filter((m): m is string => typeof m === 'string' && m.trim().length > 0);
  } catch {
    return [];
  }
}

async function persistModels(list: string[]) {
  await setSetting(QWEN_SETTING_KEYS.models, JSON.stringify(list));
}

function applyPlanDefaults(plan: QwenKeyPlan, preferModel?: string) {
  const list = fallbackModels(plan);
  models.value = list;
  if (preferModel && list.includes(preferModel)) {
    model.value = preferModel;
  } else if (!list.includes(model.value)) {
    model.value = list[0] || '';
  }
}

async function load() {
  const [savedKey, savedPlan, savedModel, savedPrompt, savedModels, visited, schema, defaultPrompt] =
    await Promise.all([
      getSetting(QWEN_SETTING_KEYS.apiKey),
      getSetting(QWEN_SETTING_KEYS.keyPlan),
      getSetting(QWEN_SETTING_KEYS.model),
      getSetting(QWEN_SETTING_KEYS.prompt),
      getSetting(QWEN_SETTING_KEYS.models),
      getSetting(QWEN_SETTING_KEYS.configVisited),
      getQwenPromptSchema().catch(() => ''),
      getQwenDefaultPrompt().catch(() => DEFAULT_QWEN_PROMPT),
    ]);

  keyPlan.value = parseKeyPlan(savedPlan);

  const cachedModels = parseSavedModels(savedModels);
  if (cachedModels.length) {
    models.value = cachedModels;
  } else {
    models.value = fallbackModels(keyPlan.value);
  }

  if (savedKey) apiKey.value = savedKey;
  if (savedModel) {
    model.value = savedModel;
    if (!models.value.includes(savedModel)) {
      models.value = [savedModel, ...models.value];
    }
  } else if (!models.value.includes(model.value)) {
    model.value = models.value[0] || fallbackModels(keyPlan.value)[0];
  }
  prompt.value = savedPrompt || defaultPrompt || DEFAULT_QWEN_PROMPT;
  schemaSuffix.value = schema;

  // 仅首次默认展开；之后默认折叠
  const firstVisit = !visited;
  await nextTick();
  if (detailsEl.value) {
    detailsEl.value.open = firstVisit;
  }
  if (firstVisit) {
    void setSetting(QWEN_SETTING_KEYS.configVisited, '1');
  }

  emitChange();
}

async function save() {
  saving.value = true;
  try {
    await Promise.all([
      setSetting(QWEN_SETTING_KEYS.apiKey, apiKey.value.trim()),
      setSetting(QWEN_SETTING_KEYS.keyPlan, keyPlan.value),
      setSetting(QWEN_SETTING_KEYS.model, model.value.trim()),
      setSetting(QWEN_SETTING_KEYS.prompt, prompt.value),
      persistModels(models.value),
    ]);
    emitChange();
    setMsg('千问设置已保存', 'ok');
  } catch (e) {
    setMsg('保存失败：' + (e instanceof Error ? e.message : String(e)), 'err');
  } finally {
    saving.value = false;
  }
}

async function refreshModels() {
  if (!apiKey.value.trim()) {
    setMsg('请先填写 API Key', 'err');
    return;
  }
  loadingModels.value = true;
  setMsg('');
  try {
    const list = await listQwenModels(apiKey.value.trim(), keyPlan.value);
    models.value = list.length ? list : fallbackModels();
    if (!models.value.includes(model.value)) {
      model.value = models.value[0] || fallbackModels()[0];
    }
    await Promise.all([
      persistModels(models.value),
      setSetting(QWEN_SETTING_KEYS.model, model.value.trim()),
      setSetting(QWEN_SETTING_KEYS.apiKey, apiKey.value.trim()),
      setSetting(QWEN_SETTING_KEYS.keyPlan, keyPlan.value),
    ]);
    setMsg(`已加载 ${models.value.length} 个可用模型`, 'ok');
    emitChange();
  } catch (e) {
    models.value = fallbackModels();
    if (!models.value.includes(model.value)) {
      model.value = models.value[0] || '';
    }
    setMsg('拉取模型失败，已使用内置候选：' + (e instanceof Error ? e.message : String(e)), 'warn');
  } finally {
    loadingModels.value = false;
  }
}

function resetPrompt() {
  prompt.value = DEFAULT_QWEN_PROMPT;
  emitChange();
  setMsg('已恢复默认可编辑 Prompt', 'ok');
}

function onKeyPlanChange() {
  applyPlanDefaults(keyPlan.value, model.value);
  void setSetting(QWEN_SETTING_KEYS.keyPlan, keyPlan.value);
  emitChange();
  setMsg(`已切换为「${QWEN_KEY_PLAN_OPTIONS.find((o) => o.value === keyPlan.value)?.label}」，请刷新模型列表`, 'ok');
}

watch([apiKey, keyPlan, model, prompt], () => emitChange());

/** 切换/填写模型时写入上次选择（防抖，避免手动输入每个字都落库） */
let modelSaveTimer: ReturnType<typeof setTimeout> | null = null;
watch(model, (value, oldValue) => {
  if (value === oldValue || !value.trim()) return;
  if (modelSaveTimer) clearTimeout(modelSaveTimer);
  modelSaveTimer = setTimeout(() => {
    void setSetting(QWEN_SETTING_KEYS.model, value.trim());
  }, 400);
});

onMounted(() => {
  void load();
});

defineExpose({
  getConfig: () => ({
    apiKey: apiKey.value.trim(),
    keyPlan: keyPlan.value,
    model: model.value.trim(),
    prompt: prompt.value,
    ready: ready.value,
  }),
  save,
});
</script>

<template>
  <details ref="detailsEl" class="qwen-config">
    <summary>千问识别设置</summary>
    <p class="hint">
      使用阿里云百炼 API。Key 类型须与套餐匹配：按量付费用通用 Key；Token Plan / Coding Plan 用
      <code>sk-sp-</code> 专属 Key 与对应端点。识别时会把下方「可编辑 Prompt」与固定 JSON 约束一并发送。
    </p>

    <label>Key 类型</label>
    <select v-model="keyPlan" @change="onKeyPlanChange">
      <option v-for="opt in QWEN_KEY_PLAN_OPTIONS" :key="opt.value" :value="opt.value">
        {{ opt.label }}
      </option>
    </select>
    <p class="hint">{{ keyPlanHint }}</p>

    <label>API Key</label>
    <div class="row">
      <input
        v-model="apiKey"
        :type="showKey ? 'text' : 'password'"
        :placeholder="apiKeyPlaceholder"
        spellcheck="false"
        autocomplete="off"
      />
      <button type="button" class="ghost" @click="showKey = !showKey">
        {{ showKey ? '隐藏' : '显示' }}
      </button>
    </div>

    <label>模型</label>
    <div class="row">
      <select v-model="model">
        <option v-for="m in models" :key="m" :value="m">{{ m }}</option>
        <option v-if="model && !models.includes(model)" :value="model">{{ model }}（手动）</option>
      </select>
      <button type="button" class="ghost" :disabled="loadingModels" @click="refreshModels">
        {{ loadingModels ? '拉取中…' : '刷新模型列表' }}
      </button>
    </div>
    <p class="hint">点「刷新」按当前 Key 类型对应端点拉取可用模型；也可在下方手动填写模型 ID。</p>
    <input
      v-model="model"
      type="text"
      class="qwen-model-manual"
      placeholder="或手动填写模型 ID，如 qwen3.7-plus"
      spellcheck="false"
    />

    <label>可编辑 Prompt</label>
    <textarea v-model="prompt" class="qwen-prompt" rows="6" spellcheck="false" />
    <div class="row">
      <button type="button" class="ghost" @click="resetPrompt">恢复默认 Prompt</button>
      <button type="button" :disabled="saving" @click="save">{{ saving ? '保存中…' : '保存设置' }}</button>
    </div>

    <label>固定输出约束（不可修改，始终追加）</label>
    <pre class="qwen-schema">{{ schemaSuffix || '加载中…' }}</pre>

    <div v-if="msg" class="msg" :class="msgClass">{{ msg }}</div>
  </details>
</template>
