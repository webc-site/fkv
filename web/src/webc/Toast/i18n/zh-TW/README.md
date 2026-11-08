# 頁面邊緣的提示浮窗

- **函數呼叫**：提供普通與錯誤提示接口
- **自動堆疊**：多條提示自動計算間距並垂直堆疊
- **定時與手動關閉**：支援定時與手動關閉

## 使用演示

```js
import { toast, toastErr } from "webc.site/js/Toast.js";

toast((el) => {
  el.textContent = "提示內容";
});

toastErr((el) => {
  el.textContent = "錯誤內容";
});

toast(
  (el) => {
    el.textContent = "99秒後自動關閉";
  },
  99
);
```

## 介面參數

### toast(render, timeout)

建立並顯示提示。

- `render`: `(el) => void`，渲染回調，`el` 為提示框元素。
- `timeout`: `Number`，超時秒數。預設 9。為 0 不自動關閉。
- 傳回值：提示框元素。支援 `el.close()` 關閉。

### toastErr(render, timeout)

建立並顯示錯誤提示。參數同 `toast`，帶 `.ERR` 樣式類別。

## 樣式類

- `.Toast`：提示框基底類別。
- `.ERR`：錯誤狀態。
- `.animated` / `.fadeInR` / `.fadeOutR`：動畫過渡。
- `.x`：關閉按鈕。