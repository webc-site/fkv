# 虛擬捲軸

- **雙向滾動**：提供垂直（`c-vs`）與水平（`c-hs`）滾動。
- **尺寸響應**：監聽內容與區域尺寸變化，自動更新捲軸尺寸與位置，無溢出時自動隱藏。
- **互動支援**：支援滾輪滾動、滑桿拖曳與軌道點擊定位。
- **狀態顯隱**：懸停或滾動時顯示並加寬滑塊，靜止時自動隱藏。

## 垂直滾動

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

## 水平捲動

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

## 樣式部位

透過 `::part()` 自訂 Shadow DOM 內部元素的樣式：

- `::part(scroll)`：滾動內容區域。
- `::part(bar)`：滾動條軌。
- `::part(si)`：滾動條滑桿。
- `::part(i)`：內部包裹層。
- `::part(drag)`：拖曳狀態下的捲軸軌道。

## 樣式類

- `.drag`：應用於 `body` 元素，表示處於拖曳滾動狀態。

## 自訂屬性

- `--si-bg`：滑桿背景色。
- `--si-anim`：滑桿動畫。
- `--cursorScrollhSvg`：水平捲軸懸停時的滑鼠遊標。
- `--cursorScrollvSvg`：垂直滾動條懸停時的滑鼠遊標。