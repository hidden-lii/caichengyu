/** 与后端 qwen::default_user_prompt 保持一致的前端兜底文案 */
export const DEFAULT_QWEN_PROMPT = `你是「猜成语」游戏截图解析助手。图片是游戏界面截图，包含若干次猜测。
每次猜测是一个四字成语；每个字有四个维度的颜色标记：字、声（声母）、韵（韵母）、调（声调）。
颜色含义：绿色=正确且位置对(hit)，紫色/粉色=存在但位置不对(present)，灰色=不存在(absent)。
请识别每一行猜测的成语文字，以及每个字四个维度的标记状态。`;

export const QWEN_SETTING_KEYS = {
  apiKey: 'qwen_api_key',
  model: 'qwen_model',
  prompt: 'qwen_prompt',
  /** JSON 数组：曾经拉取到的模型 id 列表 */
  models: 'qwen_models',
  /** 非空表示已打开过设置面板，之后默认折叠 */
  configVisited: 'qwen_config_visited',
  /** '1' 表示识别时实时展示流式返回 */
  streamPreview: 'qwen_stream_preview',
} as const;

export const FALLBACK_QWEN_MODELS = [
  'qwen3-vl-plus',
  'qwen3-vl-flash',
  'qwen-vl-max',
  'qwen-vl-plus',
  'qwen-vl-ocr-latest',
  'qwen2.5-vl-72b-instruct',
  'qwen2.5-vl-32b-instruct',
  'qwen2.5-vl-7b-instruct',
];
