# 侧边滑出的抽屉面板

- 自动创建内部滚动结构
- 点击外部区域自动收起
- 左右滑动手势收起
- 左右方向侧滑定位
- 提供打开与关闭事件

## 使用演示

```html
<button class="open-btn">打开</button>

<c-side class="right">
  <div>抽屉内容</div>
</c-side>

<script type="module">
import { open } from "webc.site/Side.js";

const btn = document.querySelector(".open-btn"),
  side = document.querySelector("c-side");

btn.onclick = () => open(side);
</script>
```

## 接口

### 方法

- `open(el)`: 导出函数，打开抽屉
- `close(el)`: 导出函数，关闭抽屉
- `el.open()`: 实例方法，打开抽屉
- `el.close()`: 实例方法，关闭抽屉

### 事件

- `open`: 开启时触发
- `close`: 关闭时触发

## 样式类

- `.right`: 右滑出定位
- `.top`: 上滑出定位
- `.bottom`: 下滑出定位
- （未设置则默认为左滑出）
- `.open`: 开启状态
- `.H`: 隐藏状态
- `.T`: 动画过渡状态