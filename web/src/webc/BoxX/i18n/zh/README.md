# 创建并显示带关闭按钮的弹出层

- 函数式调用以创建并显示弹出层
- 内置关闭按钮，点击关闭并从 DOM 中移除元素
- 按 Esc 键让输入框失焦或关闭弹出层

## 使用演示

```html
<button>点击弹出</button>

<script type="module">
import BoxX from "webc.site/BoxX.js";

document.querySelector("button").onclick = () => {
  const dialog = BoxX(),
    main = document.createElement("main"),
    h3 = document.createElement("h3");
  main.classList.add("Lg");
  h3.textContent = "文本";
  main.appendChild(h3);
  dialog.appendChild(main);
};
</script>
```


## 接口参数

默认导出函数返回 `HTMLDialogElement`，已添加 `Box` 和 `X` 样式类的对话框元素。

## 样式类

### `.Box.X`

应用于 `dialog` 元素，指定对话框样式。

### `a.X`

应用于 `a` 元素，指定关闭按钮样式。