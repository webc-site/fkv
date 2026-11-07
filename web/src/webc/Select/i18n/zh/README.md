# 下拉选择，宽度自适应当前项

原生 `select` 宽度默认由最长选项文本决定，切换短选项时留白过多，影响美观。

本组件将原生 `select` 透明覆盖在 `b` 标签上方，实现宽度随当前选中项文本长度精准自适应。

- 保持原生 `select` 在各端的下拉菜单样式及无障碍访问支持
- 支持完整的键盘 Tab 导航与方向键切换等原生交互操作
- 宽度自适应当前选项，消除多余留白

## 使用演示

```html
<c-select>
  <option value="js">JavaScript</option>
  <option value="rs">Rust</option>
  <option value="py">Python</option>
</c-select>
```