# 渲染並展示元件的示範、說明文件與安裝說明

- 擷取並解析 README 說明文件的標題、簡介與內容
- 沙盒運行組件互動演示並隔離樣式
- 轉換文件中的程式碼區塊為互動式演示
- 自適應佈局，寬螢幕分列展示，窄螢幕標籤頁切換展示

## 使用演示

```html
<c-webc-demo></c-webc-demo>
```

```js
import WebCDemo from "./_.js";

const el = document.querySelector("c-webc-demo");

// 初始化並取得操作接口
const [setMod, setMd, unMount] = WebCDemo(el);
```

## 介面參數

### `WebCDemo(el)`

初始化組件。

- **參數**
  - `el`：`HTMLElement`，組件容器。
- **傳回值**：`[setMod, setMd, unMount]`
  - `setMod(mod)`：設定演示模組。
  - `setMd(name, md, lang)`：設定元件名稱、README 文件內容及目前語言。
  - `unMount()`：卸載元件並清理事件監聽器。

### 演示模組 `mod` 屬性

- `CSS`：`string`，元件的樣式表路徑。
- `HTM`：`string`，元件的 HTML 範本。
- `default`：`function(root)`，元件初始化函數，接收 shadow root 容器。

## 樣式類

### `.M`

內容區域。

### `.L`

左側面板，包含使用說明與文件。

### `.R`

右側面板，包含示範。

### `c-nav`

窄螢幕導覽列。