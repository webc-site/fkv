# 閉じるボタン付きのポップアップレイヤーを作成して表示する

- ポップアップレイヤーを作成して表示するための関数呼び出し
- 組み込みの閉じるボタン。クリックすると要素が閉じられ、DOM から削除されます。
- Esc キーを押して入力ボックスのフォーカスを外すか、ポップアップ レイヤーを閉じます

## デモを使用する

```html
<button>クリックするとポップアップが表示されます</button>

<script type="module">
import BoxX from "webc.site/BoxX.js";

document.querySelector("button").onclick = () => {
  const dialog = BoxX(),
    main = document.createElement("main"),
    h3 = document.createElement("h3");
  main.classList.add("Lg");
  h3.textContent = "テキスト";
  main.appendChild(h3);
  dialog.appendChild(main);
};
</script>
```


## インターフェースパラメータ

デフォルトのエクスポート関数は `HTMLDialogElement` を返し、`Box` および `X` スタ​​イル クラスがダイアログ要素に追加されています。

## スタイルクラス

### `.Box.X`

`dialog` 要素に適用され、ダイアログ スタイルを指定します。

### `a.X`

`a` 要素に適用され、閉じるボタンのスタイルを指定します。