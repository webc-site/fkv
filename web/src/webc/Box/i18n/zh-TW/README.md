# 建立並顯示彈出層

- 函數式呼叫以建立並顯示彈出層
- 關閉時自動從 DOM 移除元素
- 阻止 Esc 鍵取消行為

## 使用演示

```html
<button>點擊彈出</button>

<script type="module">
import Box from "webc.site/Box.js";

document.querySelector("button").onclick = () => {
  const dialog = Box(),
    content = document.createElement("div"),
    title = document.createElement("h3"),
    description = document.createElement("p"),
    button = document.createElement("button");

  content.className = "Lg";
  title.textContent = "標題";
  description.textContent = "文字";
  button.className = "Btn Main";
  button.textContent = "確定";
  button.onclick = () => dialog.close();
  content.append(title, description, button);
  dialog.append(content);
};
</script>
```

## 介面參數

### 預設導出函數

`Box()`

- **傳回值**：`HTMLDialogElement`，已新增 `Box` 樣式類別的對話方塊元素。

## 樣式類

### `.Box`

應用於 `dialog` 元素，設定定位、居中、背景遮罩模糊。

### `.Lg`

套用於內部內容的樣式類，設定背景色、圓角、陰影與垂直佈局，並定義內部 `h3` 和 `p` 的排版。