# ポップアップレイヤーの作成と表示

- ポップアップレイヤーを作成して表示するための関数呼び出し
- DOM を閉じるときに要素を自動的に削除します
- Esc キーのキャンセル動作を防止する

## デモを使用する

```html
<button>クリックするとポップアップが表示されます</button>

<script type="module">
import Box from "webc.site/Box.js";

document.querySelector("button").onclick = () => {
  const dialog = Box(),
    content = document.createElement("div"),
    title = document.createElement("h3"),
    description = document.createElement("p"),
    button = document.createElement("button");

  content.className = "Lg";
  title.textContent = "タイトル";
  description.textContent = "テキスト";
  button.className = "Btn Main";
  button.textContent = "OK";
  button.onclick = () => dialog.close();
  content.append(title, description, button);
  dialog.append(content);
};
</script>
```

## インターフェースパラメータ

### デフォルトのエクスポート関数

`Box()`

- **戻り値**: `HTMLDialogElement`、`Box` スタ​​イル クラスのダイアログ要素が追加されました。

## スタイルクラス

### `.Box`

`dialog` 要素に適用して、位置、中央揃え、背景マスクのぼかしを設定します。

### `.Lg`

内部コンテンツに適用されるスタイル クラス。背景色、角丸、影、垂直レイアウトを設定し、内部 `h3` と `p` のレイアウトを定義します。