# 通常のボタンと丸いアイコンボタン

## クラス名

- 通常ボタン`.Btn`
- アイコン ボタン `.BtnC`
- プライマリ ハイライト状態`.Btn.Main`

## 使用

要素にクラス名を追加します。

```html
<!-- 通常ボタン -->
<button class="Btn">確認する</button>

<!-- 主なハイライト -->
<button class="Btn Main">提出する</button>

<!-- リンクボタン -->
<a class="Btn" href="https://github.com">リンクボタン</a>
```

## アイコンのスタイル

`BtnC` のアイコンは、`background-image` 経由でカスタマイズできます。

```html
<style>
.BtnC.add .Ico {
  background-image: url("data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 10 10' stroke='%23000'><path d='M5 1v8M1 5h8'/></svg>");
}
</style>
<button class="BtnC add"><i class="Ico"></i></button>
```