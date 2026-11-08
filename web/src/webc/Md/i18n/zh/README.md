# 将 Markdown 文本安全解析并渲染为 HTML。

- **安全净化**：使用原生 `setHTML` API 或 `DOMParser` 清洗器防御 XSS 攻击
- **内容提取**：初始化时自动读取默认 slot 的文本内容并进行渲染
- **内置排版**：预设列表、表格、代码块、引用及警告框样式

## 使用演示

### 静态渲染

```html
<c-md>
# 标题
这是一段 Markdown 内容
</c-md>
```

### 动态赋值

```html
<c-md id="md"></c-md>

<script type="module">
import "webc.site/Md.js";
document.querySelector("#md").value = "# 新标题\n新内容";
</script>
```

## 接口参数

直接在标签内传入 Markdown 文本（作为默认 slot），或通过 `value` 动态更新。

## 样式说明

组件初始化时自动添加 `.Md` 样式类。支持以下 Markdown 拓展警告框排版（通过引用语法触发）：

- `[!NOTE]`：提示信息
- `[!TIP]`：建议/提示
- `[!IMPORTANT]`：重要关注
- `[!WARNING]`：风险警告
- `[!CAUTION]`：极高风险警告

## 安全机制

渲染时优先调用原生 `setHTML` API 安全注入 HTML。若浏览器不支持，自动降级为基于 `DOMParser` 的递归清洗器，执行以下安全策略：
- 移除非安全标签（如 `script`、`iframe` 等）
- 清除 `on*` 事件属性
- 过滤以 `javascript:`、`vbscript:` 和 `data:`（`data:image/` 除外）等危险协议开头的属性，防御 XSS 攻击。