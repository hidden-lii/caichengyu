# 猜成语

本地猜成语解题助手（Tauri + Vue）。截图识别通过阿里云百炼（通义千问）多模态 API 完成。

## 开发

```bash
npm install
npm run tauri dev
```

## 截图识别（千问）

在「猜成语」面板：

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

## 变更记录

### v0.3.0

- **猜测交互**：左键切换「对/无」，右键切换「偏/无」；字命中后自动带出浅色声韵调，人选标记优先
- **读音编辑**：支持按音节改读音；改完可确认写入/更新词库，或仅改当前展示
- **词库体验**：加载/导入分块建索引并显示进度，避免界面卡死；刷新词库不卸载面板
- **性能**：SQLite 相关 Tauri 命令改为 `spawn_blocking` 异步执行，避免阻塞 UI 主线程
- **细节**：按钮右键不再误选文字；词库忙碌遮罩与写入确认对话框

### v0.2.0

- 支持按量付费 / Token Plan / Coding Plan 三类 Key，并路由到对应 OpenAI 兼容端点
