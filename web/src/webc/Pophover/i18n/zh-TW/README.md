# 懸浮氣泡提示

在滑鼠懸停或元素啟動時展示懸浮提示訊息。

- 基於原生 Web Component，輕量且高效能
- 支援上下左右（class="top/bottom/left/right"）四個方向自適應空間定位
- 採用黑色磨砂玻璃質感漸變背景與白色字符，配上精緻的物理邊緣輪廓高光
- 內建位置自適應計算，根據視窗剩餘空間自動調整彈出方向
- 配合 `slot="pop"` 顯示懸浮提示內容

## 使用演示

```html
<!-- 自動計算方向 (推薦，無須聲明 class) -->
<c-pophover>
  <button>自動</button>
  <div slot="pop">這是一個氣泡提示</div>
</c-pophover>

<!-- 固定方向 -->
<c-pophover class="left">
  <button>左</button>
  <div slot="pop">這是一個氣泡提示</div>
</c-pophover>

<c-pophover class="right">
  <button>右</button>
  <div slot="pop">這是一個氣泡提示</div>
</c-pophover>

<c-pophover class="bottom">
  <button>下</button>
  <div slot="pop">這是一個氣泡提示</div>
</c-pophover>

<c-pophover class="top">
  <button>上</button>
  <div slot="pop">這是一個氣泡提示</div>
</c-pophover>
```

## CSS 變數

支援透過以下 CSS 變數自訂樣式：

| 變數名 | 預設值 | 說明 |
| :--- | :--- | :--- |
| `--pophover-gap` | `8px` | 氣泡提示框與觸發元素之間的間距 |
| `--pophover-margin` | `24px` | 自適應定位運算時的視窗邊緣安全邊距 |
| `--pophover-bg-top` | `#18181ce0` | 氣泡漸層背景頂部顏色 |
| `--pophover-bg-mid` | `#121215e5` | 氣泡左右側箭頭背景顏色 |
| `--pophover-bg-bottom` | `#0c0c0feb` | 氣泡漸層背景底部及上下側箭頭顏色 |

## 圖示提示樣式 (`.Ico`)

對於純圖示類型的懸浮提示按鈕（如「編輯」、「刪除」等），可透過為外層容器或元件本身新增 `.Ico` 類別名，以套用統一的圖示與 hover 樣式。

### 結構範例

```html
<b class="Edit Ico">
  <c-pophover>
    <a></a>
    <div slot="pop">編輯</div>
  </c-pophover>
</b>
```

基礎佈局、尺寸（18x18px）、hover 變色已在 `.Ico > c-pophover > a` 中統一處理。開發者只需單獨指定對應的圖示文件，例如:
```css
.Edit > c-pophover > a {
  mask-image: url("./svg/edit.svg")
}
```