# 下拉选择框与按钮的组合

- 将选择框与按钮水平拼接
- 自动保存选中状态至 localStorage
- 拦截按钮点击并派发自定义事件

## 使用演示

```html
<c-select-btn key="my-select-key" value="option1">
  <c-select>
    <option value="option1">选项 1</option>
    <option value="option2">选项 2</option>
  </c-select>
  <button>运行</button>
</c-select-btn>

<script type="module">
  import "webc.site/SelectBtn.js";
  const el = document.querySelector("c-select-btn");
  el.addEventListener("submit", (e) => {
    console.log(e.value);
  });
</script>
```

## 属性

- `key`: 用于 localStorage 持久化的键名
- `value`: 当前选中的值

## 事件

- `submit`: 按钮被点击时派发，`e.value` 携带当前选中的值