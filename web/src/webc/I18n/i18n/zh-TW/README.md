# 切換介面語言的按鈕與彈跳窗

- 點擊按鈕彈出語言選擇浮層。
- 自動高亮目前選取的語言。

## 使用演示

```html
<c-i18n></c-i18n>

<script type="module">
import "webc.site/I18n.js";
// 用下面的 JS 程式碼監聽語言變更
import { onLang } from "webc.site/js/i18n.js";

onLang((code, id) => {
  alert(code + ' : ' + id);
});
</script>
```

## 介面參數

### 自訂標籤 `<c-i18n>`

自訂 HTML 標籤，引入 JS 後在 HTML 中使用。

## 狀態管理

使用 `src/js/i18n.js` 管理全域語言狀態：

- **`LANG_LI`**：語言列表數組，格式為 `[[名稱, ID], ...]`。
- **`lang()`**：取得目前語言 ID。
- **`langCode()`**：取得目前語言編碼。
- **`langSet(id)`**：設置語言 ID 並通知訂閱者。
- **`onLang(func)`**：订阅语言变更，若已设置语言则立即触发回调。返回取消訂閱函數。

## 樣式類

### `.BtnC.lang`

圖標按鈕。

### `.I18n.Lg`

彈跳窗主體佈局，用於包裹語言選項。