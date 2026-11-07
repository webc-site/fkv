# 页面边缘的提示浮窗

- **函数调用**：提供普通与错误提示接口
- **自动堆叠**：多条提示自动计算间距并垂直堆叠
- **定时与手动关闭**：支持定时与手动关闭

## 使用演示

```js
import { toast, toastErr } from "webc.site/js/Toast.js";

toast((el) => {
  el.textContent = "提示内容";
});

toastErr((el) => {
  el.textContent = "错误内容";
});

toast(
  (el) => {
    el.textContent = "99秒后自动关闭";
  },
  99
);
```

## 接口参数

### toast(render, timeout)

创建并显示提示。

- `render`: `(el) => void`，渲染回调，`el` 为提示框元素。
- `timeout`: `Number`，超时秒数。默认 9。为 0 不自动关闭。
- 返回值：提示框元素。支持 `el.close()` 关闭。

### toastErr(render, timeout)

创建并显示错误提示。参数同 `toast`，带 `.ERR` 样式类。

## 样式类

- `.Toast`：提示框基类。
- `.ERR`：错误状态。
- `.animated` / `.fadeInR` / `.fadeOutR`：动画过渡。
- `.x`：关闭按钮。