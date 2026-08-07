# 猜成语

本地猜成语解题助手（Tauri + Vue）。截图识别支持**通义千问（云端）**与**本地 PP-OCRv5（Rust / ONNX）**，可在面板切换。

## 开发

```bash
npm install
npm run tauri dev
```

本地 OCR 需将 ONNX 模型放到 `src-tauri/resources/ocr/`（见该目录 README）。

## 词库

词库保存在本地 SQLite。首次启动自动导入内置**新华成语**词库。

在「词库设置」可切换内置数据源（切换会整库替换）：

| 来源 | 说明 |
| --- | --- |
| 新华成语 | 默认内置词库（`resources/idiom.json`） |
| 汉文学网成语 | 爬虫精简词库，**仅含四字**（`resources/idioms_hwxnet.json`） |

也可从网络 URL 或本地 JSON / JSONL 导入；JSONL 导入时会自动排除非四字成语。

从原始爬虫 JSONL 重新生成汉文学网词库：

```bash
npm run build:hwxnet-lexicon
# 或指定路径
node scripts/build-hwxnet-lexicon.mjs path/to/idioms.jsonl
```

## 截图识别

在「猜成语」面板选择识别引擎：

### 千问（云端）

1. 选择 Key 类型（按量付费 / Token Plan / Coding Plan）
2. 填写对应套餐的 API Key
3. 选择或刷新模型列表（也支持手动填写模型 ID）
4. 可按需调整 Prompt；固定 JSON 输出约束会始终追加，无法关闭
5. 拖入 / 粘贴 / 选择截图，复核识别结果后写入筛选条件

OpenAI 兼容端点（须与 Key 类型配套）：

| Key 类型 | API Key | Base URL |
| --- | --- | --- |
| 按量付费 | `sk-...` | `https://dashscope.aliyuncs.com/compatible-mode/v1` |
| Token Plan | `sk-sp-...` | `https://token-plan.cn-beijing.maas.aliyuncs.com/compatible-mode/v1` |
| Coding Plan | `sk-sp-...` | `https://coding.dashscope.aliyuncs.com/v1` |

### 本地 OCR

1. 切换到「本地 OCR」
2. 按需调整放大倍数（默认 500%）
3. 拖入截图识别：放大 → 绿/紫粉/灰三色通道二值化 → 分通道 OCR 合并
4. 复核后写入筛选条件（无需 API Key）

## 变更记录

### v0.4.0

- **本地 OCR**：支持 PP-OCRv5（Rust / ONNX），可与千问云端识别切换；截图经放大与绿/紫粉/灰三色通道二值化后识别
- **汉文学网词库**：内置四字成语词库，可在词库设置中切换数据源；提供 `build:hwxnet-lexicon` 从爬虫 JSONL 重建

### v0.3.1

- **选中备选项**：点击候选「选中」后自动滚动到上方标记预览区
- **品牌标识**：更新应用 Logo / favicon / 安装包图标

### v0.3.0

- **猜测交互**：左键切换「对/无」，右键切换「偏/无」；字命中后自动带出浅色声韵调，人选标记优先
- **读音编辑**：支持按音节改读音；改完可确认写入/更新词库，或仅改当前展示
- **词库体验**：加载/导入分块建索引并显示进度，避免界面卡死；刷新词库不卸载面板
- **性能**：SQLite 相关 Tauri 命令改为 `spawn_blocking` 异步执行，避免阻塞 UI 主线程
- **细节**：按钮右键不再误选文字；词库忙碌遮罩与写入确认对话框

### v0.2.0

- 支持按量付费 / Token Plan / Coding Plan 三类 Key，并路由到对应 OpenAI 兼容端点
