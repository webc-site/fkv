# 带有下拉选择的标签页

- 支持在标签内嵌入自适应宽度下拉选择框（`<c-select>`），仅在当前标签页激活时展示。
- 结构规范：子元素采用 `nav > a` 作为标签导航，直接子元素 `b` 作为内容区域。
- 自动关联 `a[value]` 标签、带有 `c-select` 的 `a`（依据组的 value 属性与选中 option 值）与 `b[slot]` 内容节点。
- 支持通过 `key` 属性自动持久化当前激活项到 `localStorage` 中。
- 支持使用 `key + '/' + tab` 自动保存和恢复每一组的历史子选择项到 `localStorage`。
- 支持通过 `value` 属性指定激活的标签页。

## 使用演示

```html
<c-tab-select key="my-tab-select-key" value="language/js">
  <nav>
    <a value="language">
      <span>语言</span>
      <c-select>
        <option value="js">JavaScript</option>
        <option value="rs">Rust</option>
      </c-select>
    </a>
    <a value="os">
      <span>系统</span>
      <c-select>
        <option value="mac">macOS</option>
        <option value="win">Windows</option>
      </c-select>
    </a>
  </nav>
  <b slot="language">
    <b slot="js">JavaScript 是一种脚本语言</b>
    <b slot="rs">Rust 是一种系统级编程语言</b>
  </b>
  <b slot="os">
    <b slot="mac">macOS 是基于 Unix 的图形操作系统</b>
    <b slot="win">Windows 是微软开发的操作系统</b>
  </b>
</c-tab-select>
```

## 属性

- `key`: 用于 localStorage 持久化的键名
- `value`: 当前选中的值

## 自定义事件

- `change`: 标签页切换时触发，`e.value` 为当前激活选项的 `value` 值。