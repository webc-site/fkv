# 目錄導航元件

- **響應式抽屜**：寬螢幕展示側邊欄，窄螢幕自動收起並配合 `c-side` 抽屜與標題觸發按鈕
- **多語言驅動**：支援物件字典或多語言函數，自動回應語言切換
- **自動子目錄**：自動解析目標容器內的 `h1`~`h6` 標題產生二級目錄並監聽 Hash 視窗滾動高亮
- **非同步載入**：點選目錄項目自動非同步取得 Markdown 文字並注入內容區域，顯示 `Ing` 載入動畫

## 使用演示

```html
<c-toc></c-toc>
<c-md></c-md>

<script type="module">
import "webc.site/Toc.js";

const toc = document.querySelector("c-toc");
toc.target = document.querySelector("c-md");
toc.prefix = "doc";
toc.mdUrl = (key, lang) => "/doc/" + lang + "/" + key + ".md";
toc.li = (lang) => ({
  "README": "WebC.site",
  "use": "使用元件"
});
</script>
```

## 介面參數

### 屬性與方法

- `toc.prefix`：取得或設定路由前綴字串（例如 `"doc"`，結尾斜線可選），`README` 鍵會自動對應為空路徑（即前綴本身）
- `toc.li`：取得或設定目錄多語言產生函數 `(lang) => ({ key: title })`
- `toc.mdUrl`：取得或設定 Markdown URL 生成函數 `(key, lang) => url`
- `toc.target`：取得或設定關聯的目標渲染節點（如 `c-md`），未指定時自動尋找 `c-md` / `main` / `article`
- `toc.load(key)`：手動觸發對應 key 的文件載入與目錄展開
- `toc.sync()`：手動重新解析目前目標節點中的標題，同步二級目錄與高亮狀態