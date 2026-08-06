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
