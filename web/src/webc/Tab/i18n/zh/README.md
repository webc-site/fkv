# 可切换的标签页

- 标签选中状态与下方内容区域无缝融为一体。
- 结构规范：子元素固定采用 `nav > a` 作为标签导航，直接子元素 `b` 作为内容区域。
- 自动关联 `a[value]` 标签与 `b[slot]` 内容节点。
- 支持通过 `key` 属性自动持久化当前激活页到 `localStorage` 中。
- 支持通过 `value` 属性指定激活的标签页（若无缓存及 `value`，则默认选中带有 `class="A"` 的标签，或首个标签）。
- 轻量化 Light DOM 实现，无 Shadow DOM 阻隔，样式定制自由度高。

## 使用演示

```html
<c-tab key="my-tab-key" value="banana">
  <nav>
    <a value="apple">苹果</a>
    <a value="banana">香蕉</a>
    <a value="orange">橘子</a>
  </nav>
  <b slot="apple">苹果是一种营养丰富的水果</b>
  <b slot="banana">香蕉是热带地区的水果</b>
  <b slot="orange">橘子含有丰富的维生素 C</b>
</c-tab>
```

## 属性

- `key`: 用于 localStorage 持久化的键名
- `value`: 当前选中的值

## 自定义事件

- `change`: 标签页切换时触发，`e.value` 为当前激活标签的 `value` 值。