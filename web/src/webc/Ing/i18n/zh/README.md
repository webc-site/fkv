# 处于加载或处理状态的指示器

- 统一光标为等待状态
- 显示波纹动画指示处理状态
- 为表单应用模糊遮罩，防止重复提交

## 使用演示

```html
<!-- 独立加载指示器 -->
<div class="Ing"></div>

<!-- 带遮罩的加载中表单 -->
<form class="Ing">
  <input type="text" placeholder="输入框">
  <button type="submit">提交</button>
</form>
```

## 样式类

### `.Ing`
- 指针状态为 `wait`。
- 伪元素 `::before` 渲染波纹动画。
- 用于 `form` 元素时，伪元素 `::after` 渲染 1px 模糊遮罩，禁用交互与文本选择。