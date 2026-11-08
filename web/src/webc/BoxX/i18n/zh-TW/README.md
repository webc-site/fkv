# 建立並顯示帶有關閉按鈕的彈出層

- 函數式呼叫以建立並顯示彈出層
- 內建關閉按鈕，點擊關閉並從 DOM 移除元素
- 按 Esc 鍵讓輸入框失焦或關閉彈出層

## 使用演示

```html
<button>點擊彈出</button>

<script type="module">
import BoxX from "webc.site/BoxX.js";

document.querySelector("button").onclick = () => {
  const dialog = BoxX(),
    main = document.createElement("main"),
    h3 = document.createElement("h3");
  main.classList.add("Lg");
  h3.textContent = "文字";
  main.appendChild(h3);
  dialog.appendChild(main);
};
</script>
```


## 介面參數

預設匯出函數回傳 `HTMLDialogElement`，已新增 `Box` 和 `X` 樣式類別的對話框元素。

## 樣式類

### `.Box.X`

套用於 `dialog` 元素，指定對話方塊樣式。

### `a.X`

套用於 `a` 元素，指定關閉按鈕樣式。