# 创建并显示弹出层

- 函数式调用以创建并显示弹出层
- 关闭时自动从 DOM 中移除元素
- 阻止 Esc 键取消行为

## 使用演示

```html
<button>点击弹出</button>

<script type="module">
import Box from "webc.site/Box.js";

document.querySelector("button").onclick = () => {
  const dialog = Box(),
    content = document.createElement("div"),
    title = document.createElement("h3"),
    description = document.createElement("p"),
    button = document.createElement("button");

  content.className = "Lg";
  title.textContent = "标题";
  description.textContent = "文本";
  button.className = "Btn Main";
  button.textContent = "确定";
  button.onclick = () => dialog.close();
  content.append(title, description, button);
  dialog.append(content);
};
</script>
```

## 接口参数

### 默认导出函数

`Box()`

- **返回值**：`HTMLDialogElement`，已添加 `Box` 样式类的对话框元素。

## 样式类

### `.Box`

应用于 `dialog` 元素，设定定位、居中以及背景遮罩模糊。

### `.Lg`

应用于内部内容的样式类，设定背景色、圆角、阴影与垂直布局，并定义内部 `h3` 和 `p` 的排版。