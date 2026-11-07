# 渲染元件或工具函數的文件與原始碼

- 雙欄響應式佈局，左側顯示文檔，右側顯示源碼
- 文件與源碼支援 Edit 和 Code 快捷按鈕跳到來源位址
- 頂部展示標題與簡介，支援 i18n 自動翻譯

## 使用演示

```html
<c-webc-js src="/src/js/dom.js" href="/doc/{code}/js/dom.md"></c-webc-js>
```

## 屬性參數

- `src`: 原始碼的 URL 位址。
- `href`: 文件 markdown 的 URL 位址，支援用 `{code}` 佔位符適配目前語言。