# 猜成语

本地猜成语解题助手（Tauri + Vue）。截图识别通过阿里云百炼（通义千问）多模态 API 完成。

## 开发

```bash
npm install
npm run tauri dev
```

## 截图识别（千问）

在「猜成语」面板：

1. 填写 DashScope API Key
2. 选择或刷新模型列表（也支持手动填写模型 ID）
3. 可按需调整 Prompt；固定 JSON 输出约束会始终追加，无法关闭
4. 拖入 / 粘贴 / 选择截图，复核识别结果后写入筛选条件

API 使用 OpenAI 兼容接口：`https://dashscope.aliyuncs.com/compatible-mode/v1`
