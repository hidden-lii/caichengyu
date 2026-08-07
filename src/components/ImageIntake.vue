<script setup lang="ts">
import { nextTick, onMounted, onUnmounted, ref, watch } from 'vue';
import { readBinaryFile } from '@tauri-apps/api/fs';
import { appWindow } from '@tauri-apps/api/window';
import { fileToBase64 } from '../engine/ocr';

const emit = defineEmits<{
  /** 用户点开始识别后立刻发出，便于父组件先展示 loading */
  start: [];
  image: [payload: { base64: string; mime: string; previewUrl: string }];
  error: [message: string];
  'update:showStreamPreview': [value: boolean];
}>();

const props = defineProps<{
  busy?: boolean;
  /** 识别进行中的阶段文案 */
  statusText?: string;
  /** 开启后展示流式返回，关闭则只显示等待动画 */
  showStreamPreview?: boolean;
  /** 当前累计的流式文本 */
  streamText?: string;
  /** 是否显示流式预览开关（本地 OCR 关闭） */
  allowStreamPreview?: boolean;
  /** 标题引擎名 */
  engineLabel?: string;
}>();

const dragOver = ref(false);
const fileInput = ref<HTMLInputElement | null>(null);
const pendingPreview = ref('');
const pendingMime = ref('image/png');
const pendingFile = ref<File | null>(null);
const pendingBytes = ref<Uint8Array | null>(null);
const hasPending = ref(false);
const streamPre = ref<HTMLPreElement | null>(null);

let unlistenDrop: (() => void) | null = null;

function onStreamToggle(e: Event) {
  const checked = (e.target as HTMLInputElement).checked;
  emit('update:showStreamPreview', checked);
}

watch(
  () => props.streamText,
  async () => {
    await nextTick();
    const el = streamPre.value;
    if (el) el.scrollTop = el.scrollHeight;
  }
);

const IMAGE_EXT = /\.(png|jpe?g|gif|webp|bmp|heic|tiff?)$/i;

function mimeFromName(name: string): string {
  const ext = name.split('.').pop()?.toLowerCase() || '';
  const map: Record<string, string> = {
    png: 'image/png',
    jpg: 'image/jpeg',
    jpeg: 'image/jpeg',
    gif: 'image/gif',
    webp: 'image/webp',
    bmp: 'image/bmp',
    heic: 'image/heic',
    tif: 'image/tiff',
    tiff: 'image/tiff',
  };
  return map[ext] || 'image/png';
}

function isLikelyImage(file: File): boolean {
  if (file.type.startsWith('image/')) return true;
  // QQ 等来源常给空 MIME，只靠扩展名或体积判断
  if (IMAGE_EXT.test(file.name)) return true;
  if (!file.type && file.size > 0) return true;
  return false;
}

function clearPending() {
  if (pendingPreview.value) URL.revokeObjectURL(pendingPreview.value);
  pendingPreview.value = '';
  pendingFile.value = null;
  pendingBytes.value = null;
  pendingMime.value = 'image/png';
  hasPending.value = false;
}

function stageFromFile(file: File) {
  if (props.busy) return;
  if (!isLikelyImage(file)) {
    emit('error', '请选择图片文件');
    return;
  }
  clearPending();
  pendingFile.value = file;
  pendingMime.value = file.type || mimeFromName(file.name);
  pendingPreview.value = URL.createObjectURL(file);
  hasPending.value = true;
}

function stageFromBytes(bytes: Uint8Array, mime: string, nameHint = 'drop.png') {
  if (props.busy) return;
  clearPending();
  pendingBytes.value = bytes;
  pendingMime.value = mime || mimeFromName(nameHint);
  const blob = new Blob([bytes], { type: pendingMime.value });
  pendingPreview.value = URL.createObjectURL(blob);
  hasPending.value = true;
}

async function confirmRecognize() {
  if (props.busy || !hasPending.value) return;
  // 先通知父组件进入 busy，让出一帧再做 base64，避免 UI 卡住无反馈
  emit('start');
  await new Promise<void>((r) => requestAnimationFrame(() => r()));
  await new Promise<void>((r) => setTimeout(r, 0));

  try {
    let base64: string;
    let mime = pendingMime.value;
    let previewUrl = pendingPreview.value;

    if (pendingFile.value) {
      base64 = await fileToBase64(pendingFile.value);
      mime = pendingFile.value.type || mime;
    } else if (pendingBytes.value) {
      const blob = new Blob([pendingBytes.value], { type: mime });
      const file = new File([blob], 'drop.png', { type: mime });
      base64 = await fileToBase64(file);
    } else {
      emit('error', '没有待识别的图片');
      return;
    }

    // 把预览所有权交给父组件，避免这里 revoke
    pendingPreview.value = '';
    pendingFile.value = null;
    pendingBytes.value = null;
    hasPending.value = false;

    emit('image', { base64, mime, previewUrl });
  } catch {
    emit('error', '读取图片失败');
  }
}

function pickFromDataTransfer(dt: DataTransfer | null): File | null {
  if (!dt) return null;
  const files = dt.files;
  if (files?.length) {
    for (let i = 0; i < files.length; i++) {
      if (isLikelyImage(files[i])) return files[i];
    }
    return files[0];
  }
  const items = dt.items;
  if (!items) return null;
  for (let i = 0; i < items.length; i++) {
    const item = items[i];
    if (item.kind === 'file') {
      const file = item.getAsFile();
      if (file && isLikelyImage(file)) return file;
    }
  }
  return null;
}

function onDrop(e: DragEvent) {
  e.preventDefault();
  dragOver.value = false;
  const file = pickFromDataTransfer(e.dataTransfer);
  if (file) stageFromFile(file);
}

function onDragOver(e: DragEvent) {
  e.preventDefault();
  if (e.dataTransfer) e.dataTransfer.dropEffect = 'copy';
  dragOver.value = true;
}

function onDragLeave(e: DragEvent) {
  const next = e.relatedTarget as Node | null;
  if (next && (e.currentTarget as HTMLElement).contains(next)) return;
  dragOver.value = false;
}

function onPaste(e: ClipboardEvent) {
  if (props.busy) return;
  const file = pickFromDataTransfer(e.clipboardData);
  if (!file) return;
  e.preventDefault();
  stageFromFile(file);
}

function onDocumentPaste(e: ClipboardEvent) {
  const target = e.target as HTMLElement | null;
  if (target?.closest('input, textarea, [contenteditable="true"]')) return;
  onPaste(e);
}

function onPickClick() {
  fileInput.value?.click();
}

function onFileChange(e: Event) {
  const input = e.target as HTMLInputElement;
  const file = input.files?.[0];
  if (file) stageFromFile(file);
  input.value = '';
}

onMounted(async () => {
  document.addEventListener('paste', onDocumentPaste);
  try {
    unlistenDrop = await appWindow.onFileDropEvent(async (event) => {
      if (event.payload.type === 'hover') {
        dragOver.value = true;
        return;
      }
      if (event.payload.type === 'cancel') {
        dragOver.value = false;
        return;
      }
      dragOver.value = false;
      if (props.busy) return;
      const path = event.payload.paths.find((p) => IMAGE_EXT.test(p)) || event.payload.paths[0];
      if (!path) return;
      if (!IMAGE_EXT.test(path)) {
        emit('error', '请拖入图片文件');
        return;
      }
      try {
        const bytes = await readBinaryFile(path);
        stageFromBytes(bytes, mimeFromName(path), path);
      } catch {
        emit('error', '读取拖入图片失败（QQ 临时文件可能已被清理，请改用复制粘贴）');
      }
    });
  } catch {
    // 非 Tauri 环境忽略
  }
});

onUnmounted(() => {
  document.removeEventListener('paste', onDocumentPaste);
  unlistenDrop?.();
  clearPending();
});
</script>

<template>
  <div
    class="image-intake"
    :class="{ 'image-intake-over': dragOver, 'image-intake-busy': busy }"
    tabindex="0"
    @drop="onDrop"
    @dragover="onDragOver"
    @dragenter.prevent="dragOver = true"
    @dragleave="onDragLeave"
  >
    <input
      ref="fileInput"
      type="file"
      accept="image/*"
      class="image-intake-input"
      @change="onFileChange"
    />
    <p class="image-intake-title">从截图导入猜测（{{ engineLabel || '识别' }}）</p>
    <p class="image-intake-hint">
      拖入 / 粘贴图片后点「开始识别」，或<button type="button" class="linkish" @click="onPickClick">选择本地文件</button>
    </p>
    <label v-if="allowStreamPreview !== false" class="image-intake-stream-opt">
      <input
        type="checkbox"
        :checked="showStreamPreview"
        :disabled="busy"
        @change="onStreamToggle"
      />
      实时展示 AI 返回内容
    </label>

    <div v-if="hasPending" class="image-intake-pending">
      <img :src="pendingPreview" alt="待识别预览" class="image-intake-thumb" />
      <div class="image-intake-pending-actions">
        <button type="button" :disabled="busy" @click="confirmRecognize">
          {{ busy ? '识别中…' : '开始识别' }}
        </button>
        <button type="button" class="ghost" :disabled="busy" @click="clearPending">清除</button>
      </div>
    </div>

    <div v-if="busy && showStreamPreview" class="image-intake-stream" role="status" aria-live="polite">
      <div class="image-intake-stream-head">
        <span class="image-intake-spinner sm" aria-hidden="true" />
        <strong>{{ statusText || '千问识别中…' }}</strong>
      </div>
      <pre ref="streamPre" class="image-intake-stream-pre">{{ streamText || '等待模型开始输出…' }}</pre>
    </div>

    <div v-else-if="busy" class="image-intake-loading" role="status" aria-live="polite">
      <span class="image-intake-spinner" aria-hidden="true" />
      <div class="image-intake-loading-text">
        <strong>{{ statusText || '千问识别中…' }}</strong>
        <span>请稍候，请求可能需要数十秒</span>
      </div>
    </div>
  </div>
</template>
