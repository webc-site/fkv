# 虚拟滚动条

- **双向滚动**：提供垂直（`c-vs`）与水平（`c-hs`）滚动。
- **尺寸响应**：监听内容与区域尺寸变化，自动更新滚动条尺寸与位置，无溢出时自动隐藏。
- **交互支持**：支持滚轮滚动、滑块拖动与轨道点击定位。
- **状态显隐**：悬停或滚动时显示并加宽滑块，静止时自动隐藏。

## 垂直滚动

```html
<div style="height:300px;border:1px solid #000">
  <c-vs>
    <b>1</b>
    <b>2</b>
    <b>3</b>
  </c-vs>
</div>
<style>b { display: flex; border: 2px solid #090; padding: 30px; margin: 30px; }</style>
```

## 水平滚动

```html
<div style="width:300px;border:1px solid #000">
  <c-hs>
    <b>1</b>
    <b>2</b>
    <b>3</b>
  </c-hs>
</div>
<style>b { display: flex; border: 2px solid #090; margin: 30px; padding: 30px; }</style>
```

## 样式部位

通过 `::part()` 自定义 Shadow DOM 内部元素的样式：

- `::part(scroll)`：滚动内容区域。
- `::part(bar)`：滚动条轨道。
- `::part(si)`：滚动条滑块。
- `::part(i)`：内部包裹层。
- `::part(drag)`：拖动状态下的滚动条轨道。

## 样式类

- `.drag`：应用于 `body` 元素，表示处于拖动滚动状态。

## 自定义属性

- `--si-bg`：滑块背景色。
- `--si-anim`：滑块动画。
- `--cursorScrollhSvg`：水平滚动条悬停时的鼠标光标。
- `--cursorScrollvSvg`：垂直滚动条悬停时的鼠标光标。