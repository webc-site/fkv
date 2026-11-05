# 普通按鈕與圓形圖示按鈕

## 類別名

- 普通按鈕 `.Btn`
- 圖標按鈕 `.BtnC`
- 主要高亮狀態 `.Btn.Main`

## 使用

為元素新增類別名稱：

```html
<!-- 普通按鈕 -->
<button class="Btn">確認</button>

<!-- 主要高亮 -->
<button class="Btn Main">提交</button>

<!-- 連結按鈕 -->
<a class="Btn" href="https://github.com">連結按鈕</a>
```

## 圖示樣式

可透過 `background-image` 自訂 `BtnC` 的圖示：

```html
<style>
.BtnC.add .Ico {
  background-image: url("data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 10 10' stroke='%23000'><path d='M5 1v8M1 5h8'/></svg>");
}
</style>
<button class="BtnC add"><i class="Ico"></i></button>
```