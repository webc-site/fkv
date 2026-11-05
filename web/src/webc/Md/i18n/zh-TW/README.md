# 將 Markdown 文字安全解析並渲染為 HTML。

- **安全淨化**：使用原生 `setHTML` API 或 `DOMParser` 清洗器防禦 XSS 攻擊
- **內容擷取**：初始化時自動讀取預設 slot 的文字內容並進行渲染
- **內建排版**：預設清單、表格、程式碼區塊、引用及警告框樣式

## 使用演示

### 靜態渲染

```html
<c-md>
# 標題
這是一段 Markdown 內容
</c-md>
```

### 動態賦值

```html
<c-md id="md"></c-md>

<script type="module">
import "webc.site/Md.js";
document.querySelector("#md").value = "# 新標題\n新內容";
</script>
```

## 介面參數

直接在標籤內傳入 Markdown 文字（作為預設 slot），或透過 `value` 動態更新。

## 樣式說明

元件初始化時自動加入 `.Md` 樣式類別。支援以下 Markdown 拓展警告框排版（透過引用語法觸發）：

- `[!NOTE]`：提示訊息
- `[!TIP]`：建議/提示
- `[!IMPORTANT]`：重要關注
- `[!WARNING]`：風險警告
- `[!CAUTION]`：極高風險警告

## 安全機制

渲染時優先呼叫原生 `setHTML` API 安全注入 HTML。若瀏覽器不支援，自動降級為基於 `DOMParser` 的遞歸清洗器，執行下列安全策略：
- 移除非安全標籤（如 `script`、`iframe` 等）
- 清除 `on*` 事件屬性
- 過濾以 `javascript:`、`vbscript:` 和 `data:`（`data:image/` 除外）等危險協定開頭的屬性，防禦 XSS 攻擊。