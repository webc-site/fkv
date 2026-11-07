# 線上偵錯 HTML 與 JS 程式碼

- 在程式碼下方渲染偵錯條，提供 CodePen 與 JSFiddle 調試入口
- 自動記憶選取的偵錯平台
- 支援透過 `.gen` 屬性注入回呼以動態產生 HTML、CSS 與 JS 程式碼
- 自動辨識含 `language-js` 類別名稱的子元素並以 JS 模式運行

## 使用演示

```html
<c-htm-demo><pre><code class="language-html">
<h1>示範</h1>
</code></pre></c-htm-demo>

<script type="module">
import "webc.site/HtmDemo.js";

const el = document.querySelector("c-htm-demo");
el.gen = (type, text) => {
  // 回傳 [html, css, js]
  return [
    text,
    "h1 { color: red; }",
    "console.log(1)"
  ];
};
</script>
```

## 介面參數

### 屬性與方法

- `.gen`: 注入的回呼函數。 **必須注入此回調，偵錯條才會顯示。 **
  - 參數:
    - `type`：注入的內容類型（`HTML = 1`，`JS = 2`）。
    - `text`：子元素中提取的程式碼文字。
  - 傳回值: `[htm, css, js]`