# 在线调试 HTML 与 JS 代码

- 在代码下方渲染调试条，提供 CodePen 与 JSFiddle 调试入口
- 自动记忆选中的调试平台
- 支持通过 `.gen` 属性注入回调以动态生成 HTML、CSS 与 JS 代码
- 自动识别含 `language-js` 类名的子元素并以 JS 模式运行

## 使用演示

```html
<c-htm-demo><pre><code class="language-html">
<h1>演示</h1>
</code></pre></c-htm-demo>

<script type="module">
import "webc.site/HtmDemo.js";

const el = document.querySelector("c-htm-demo");
el.gen = (type, text) => {
  // 返回 [html, css, js]
  return [
    text,
    "h1 { color: red; }",
    "console.log(1)"
  ];
};
</script>
```

## 接口参数

### 属性与方法

- `.gen`: 注入的回调函数。**必须注入此回调，调试条才会显示。**
  - 参数:
    - `type`：注入的内容类型（`HTML = 1`，`JS = 2`）。
    - `text`：子元素中提取的代码文本。
  - 返回值: `[htm, css, js]`