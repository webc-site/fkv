# 側邊滑出的抽屜面板

- 自動建立內部滾動結構
- 點選外部區域自動收起
- 左右滑動手勢收起
- 左右方向側滑定位
- 提供開啟與關閉事件

## 使用演示

```html
<button class="open-btn">打開</button>

<c-side class="right">
  <div>抽屜內容</div>
</c-side>

<script type="module">
import { open } from "webc.site/Side.js";

const btn = document.querySelector(".open-btn"),
  side = document.querySelector("c-side");

btn.onclick = () => open(side);
</script>
```

## 介面

### 方法

- `open(el)`: 導出函數，打開抽屜
- `close(el)`: 導出函數，關閉抽屜
- `el.open()`: 實例方法，打開抽屜
- `el.close()`: 實例方法，關閉抽屜

### 事件

- `open`: 開啟時觸發
- `close`: 關閉時觸發

## 樣式類

- `.right`: 右滑出定位
- `.top`: 上滑出定位
- `.bottom`: 下滑出定位
- （未設定則預設為左滑出）
- `.open`: 開啟狀態
- `.H`: 隱藏狀態
- `.T`: 動畫過渡狀態