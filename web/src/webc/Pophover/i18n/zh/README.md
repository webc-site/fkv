# 悬浮气泡提示

在鼠标悬停或元素激活时展示悬浮提示信息。

- 基于原生 Web Component，轻量且高性能
- 支持上下左右（class="top/bottom/left/right"）四个方向自适应空间定位
- 采用黑色磨砂玻璃质感渐变背景与白色字符，配以精致的物理边缘轮廓高光
- 内置位置自适应计算，根据视口剩余空间自动调整弹出方向
- 配合 `slot="pop"` 显示悬浮提示内容

## 使用演示

```html
<!-- 自动计算方向 (推荐，无须声明 class) -->
<c-pophover>
  <button>自动</button>
  <div slot="pop">这是一个气泡提示</div>
</c-pophover>

<!-- 固定方向 -->
<c-pophover class="left">
  <button>左</button>
  <div slot="pop">这是一个气泡提示</div>
</c-pophover>

<c-pophover class="right">
  <button>右</button>
  <div slot="pop">这是一个气泡提示</div>
</c-pophover>

<c-pophover class="bottom">
  <button>下</button>
  <div slot="pop">这是一个气泡提示</div>
</c-pophover>

<c-pophover class="top">
  <button>上</button>
  <div slot="pop">这是一个气泡提示</div>
</c-pophover>
```

## CSS 变量

支持通过以下 CSS 变量自定义样式：

| 变量名 | 默认值 | 说明 |
| :--- | :--- | :--- |
| `--pophover-gap` | `8px` | 气泡提示框与触发元素之间的间距 |
| `--pophover-margin` | `24px` | 自适应定位计算时的视口边缘安全边距 |
| `--pophover-bg-top` | `#18181ce0` | 气泡渐变背景顶部颜色 |
| `--pophover-bg-mid` | `#121215e5` | 气泡左右侧箭头背景颜色 |
| `--pophover-bg-bottom` | `#0c0c0feb` | 气泡渐变背景底部及上下侧箭头颜色 |

## 图标提示样式 (`.Ico`)

对于纯图标类型的悬浮提示按钮（如“编辑”、“删除”等），可通过为外层容器或组件本身添加 `.Ico` 类名，以应用统一的图标与 hover 样式。

### 结构示例

```html
<b class="Edit Ico">
  <c-pophover>
    <a></a>
    <div slot="pop">编辑</div>
  </c-pophover>
</b>
```

基础布局、尺寸（18x18px）、hover 变色已在 `.Ico > c-pophover > a` 中统一处理。开发者只需单独指定对应的图标文件，比如:
```css
.Edit > c-pophover > a {
  mask-image: url("./svg/edit.svg")
}
```