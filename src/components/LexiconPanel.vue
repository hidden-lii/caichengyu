<script setup lang="ts">
import { ref } from 'vue';
import { open } from '@tauri-apps/api/dialog';
import { readTextFile } from '@tauri-apps/api/fs';
import {
  addIdiom,
  getSetting,
  importLexiconFromUrl,
  setSetting,
  updateIdiomPinyin,
  upsertIdioms,
} from '../api/idiom';
import { useLexicon } from '../composables/useLexicon';
import { normalizeCorpus } from '../engine/pinyin';

const { reload: reloadLexicon } = useLexicon();

function yieldToUi(): Promise<void> {
  return new Promise((resolve) => {
    requestAnimationFrame(() => {
      requestAnimationFrame(() => resolve());
    });
  });
}

const DEFAULT_URL =
  'https://raw.githubusercontent.com/pwxcoo/chinese-xinhua/master/data/idiom.json';

const urlInput = ref(DEFAULT_URL);
const urlMsg = ref('');
const urlMsgClass = ref('');
const uploadMsg = ref('');
const uploadMsgClass = ref('');
const singleWord = ref('');
const singlePinyin = ref('');
const singleExp = ref('');
const singleMsg = ref('');
const singleMsgClass = ref('');
const editWord = ref('');
const editPinyin = ref('');
const editMsg = ref('');
const editMsgClass = ref('');
const multiInput = ref('');
const multiMsg = ref('');
const multiMsgClass = ref('');
const busy = ref(false);
const busyText = ref('');

async function loadSavedUrl() {
  const saved = await getSetting('lexicon_url');
  if (saved) urlInput.value = saved;
}

loadSavedUrl();

function formatResult(prefix: string, result: { added: number; updated: number; skipped: number; errors: string[] }) {
  const err = result.errors.length ? `；${result.errors.slice(0, 3).join('；')}` : '';
  return `${prefix}：新增 ${result.added}，更新 ${result.updated}，跳过 ${result.skipped}${err}`;
}

/** fn 返回 true 时才会在遮罩仍显示时重载词库 */
async function withBusy(text: string, fn: () => Promise<boolean>): Promise<void> {
  if (busy.value) return;
  busy.value = true;
  busyText.value = text;
  await yieldToUi();
  try {
    const needReload = await fn();
    if (needReload) {
      busyText.value = '正在重新加载词库…';
      await yieldToUi();
      await reloadLexicon('正在重新加载词库…');
    }
  } finally {
    busy.value = false;
    busyText.value = '';
  }
}

async function reloadFromUrl() {
  urlMsg.value = '';
  urlMsgClass.value = '';
  const url = urlInput.value.trim();
  if (!url) {
    urlMsg.value = '请填写 URL';
    urlMsgClass.value = 'err';
    return;
  }
  await withBusy('正在从网络加载并替换词库…', async () => {
    try {
      const result = await importLexiconFromUrl(url);
      await setSetting('lexicon_url', url);
      urlMsg.value = formatResult(`已从网络加载`, result);
      urlMsgClass.value = 'ok';
      return true;
    } catch (e) {
      urlMsg.value = '加载失败：' + (e instanceof Error ? e.message : String(e));
      urlMsgClass.value = 'err';
      return false;
    }
  });
}

async function uploadFile() {
  uploadMsg.value = '';
  try {
    const selected = await open({
      multiple: false,
      filters: [{ name: 'JSON', extensions: ['json', 'txt'] }],
    });
    if (!selected || Array.isArray(selected)) return;
    await withBusy('正在导入词库文件…', async () => {
      try {
        const text = await readTextFile(selected);
        const data = JSON.parse(text);
        const items = normalizeCorpus(data);
        const result = await upsertIdioms(items);
        uploadMsg.value = formatResult('上传完成', result);
        uploadMsgClass.value = 'ok';
        return true;
      } catch (e) {
        uploadMsg.value = '上传失败：' + (e instanceof Error ? e.message : String(e));
        uploadMsgClass.value = 'err';
        return false;
      }
    });
  } catch (e) {
    uploadMsg.value = '选择文件失败：' + (e instanceof Error ? e.message : String(e));
    uploadMsgClass.value = 'err';
  }
}

async function addSingle() {
  await withBusy('正在添加成语…', async () => {
    const result = await addIdiom({
      word: singleWord.value,
      pinyin: singlePinyin.value,
      explanation: singleExp.value,
    });
    singleMsg.value = formatResult('添加完成', result);
    singleMsgClass.value = result.skipped ? 'warn' : 'ok';
    if (result.added || result.updated) {
      singleWord.value = '';
      singlePinyin.value = '';
      singleExp.value = '';
      return true;
    }
    return false;
  });
}

async function updatePinyin() {
  const word = editWord.value.trim();
  const pinyin = editPinyin.value.trim();
  if (!word || !pinyin) {
    editMsg.value = '请填写成语和新拼音';
    editMsgClass.value = 'err';
    return;
  }
  await withBusy('正在更新读音…', async () => {
    const result = await updateIdiomPinyin(word, pinyin);
    editMsg.value = formatResult('修改拼音完成', result);
    editMsgClass.value = result.updated ? 'ok' : 'warn';
    if (result.updated) {
      editWord.value = '';
      editPinyin.value = '';
      return true;
    }
    return false;
  });
}

async function addMulti() {
  await withBusy('正在批量写入…', async () => {
    try {
      const data = JSON.parse(multiInput.value);
      const items = normalizeCorpus(data);
      const result = await upsertIdioms(items);
      multiMsg.value = formatResult('批量添加完成', result);
      multiMsgClass.value = 'ok';
      return true;
    } catch (e) {
      multiMsg.value = '解析失败：' + (e instanceof Error ? e.message : String(e));
      multiMsgClass.value = 'err';
      return false;
    }
  });
}
</script>

<template>
  <section class="panel lexicon-panel">
    <p class="hint">词库保存在本地 SQLite。首次启动会自动导入内置词库。</p>

    <div v-if="busy" class="lexicon-busy-overlay" role="status" aria-live="polite">
      <div class="lexicon-busy-card">
        <span class="image-intake-spinner" aria-hidden="true"></span>
        <div class="lexicon-busy-card-text">
          <strong>{{ busyText || '处理中…' }}</strong>
          <span>界面可继续显示，请稍候</span>
        </div>
      </div>
    </div>

    <h3>词库来源</h3>
    <label>词库 JSON 地址</label>
    <div class="row">
      <input v-model="urlInput" type="url" spellcheck="false" :disabled="busy" />
      <button type="button" :disabled="busy" @click="reloadFromUrl">从网络加载</button>
    </div>
    <div v-if="urlMsg" class="msg" :class="urlMsgClass">{{ urlMsg }}</div>

    <h3>上传词库</h3>
    <button type="button" class="secondary" :disabled="busy" @click="uploadFile">选择 JSON 文件</button>
    <div v-if="uploadMsg" class="msg" :class="uploadMsgClass">{{ uploadMsg }}</div>

    <h3>单条添加</h3>
    <div class="row">
      <div>
        <label>成语</label>
        <input v-model="singleWord" type="text" placeholder="一骑绝尘" :disabled="busy" />
      </div>
      <div>
        <label>拼音</label>
        <input v-model="singlePinyin" type="text" placeholder="yi1 qi2 jue2 chen2" :disabled="busy" />
      </div>
    </div>
    <input v-model="singleExp" type="text" placeholder="可选释义" :disabled="busy" />
    <button type="button" :disabled="busy" @click="addSingle">添加单条</button>
    <div v-if="singleMsg" class="msg" :class="singleMsgClass">{{ singleMsg }}</div>

    <h3>修改拼音</h3>
    <p class="hint">
      仅更新已有成语的拼音，不影响释义。支持音标
      <span class="example">xián zhāng chě jù</span>
      或注音
      <span class="example">xian2 zhang1 che3 ju4</span>
    </p>
    <div class="row">
      <div>
        <label>成语</label>
        <input v-model="editWord" type="text" placeholder="挦章撦句" :disabled="busy" />
      </div>
      <div>
        <label>新拼音</label>
        <input
          v-model="editPinyin"
          type="text"
          placeholder="xián zhāng chě jù"
          spellcheck="false"
          :disabled="busy"
          @keydown.enter="updatePinyin"
        />
      </div>
    </div>
    <button type="button" :disabled="busy" @click="updatePinyin">保存拼音</button>
    <div v-if="editMsg" class="msg" :class="editMsgClass">{{ editMsg }}</div>

    <h3>批量添加</h3>
    <textarea
      v-model="multiInput"
      placeholder='[{"word":"...","pinyin":"...","explanation":""}]'
      spellcheck="false"
      :disabled="busy"
    />
    <button type="button" :disabled="busy" @click="addMulti">添加多条</button>
    <div v-if="multiMsg" class="msg" :class="multiMsgClass">{{ multiMsg }}</div>
  </section>
</template>
