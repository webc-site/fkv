# HTML および JS コードをオンラインでデバッグする

- コードの下にデバッグ バーをレンダリングし、CodePen および JSFiddle デバッグ ポータルを提供します
- 選択したデバッグ プラットフォームを自動的に記憶します
- `.gen` 属性を介したコールバックの挿入をサポートし、HTML、CSS、および JS コードを動的に生成します
- `language-js` クラス名を持つ子要素を自動的に識別し、JS モードで実行します。

## デモを使用する

```html
<c-htm-demo><pre><code class="language-html">
<h1>デモ</h1>
</code></pre></c-htm-demo>

<script type="module">
import "webc.site/HtmDemo.js";

const el = document.querySelector("c-htm-demo");
el.gen = (type, text) => {
  // [html、css、js] を返します
  return [
    text,
    "h1 { color: red; }",
    "console.log(1)"
  ];
};
</script>
```

## インターフェースパラメータ

### プロパティとメソッド

- `.gen`: 挿入されたコールバック関数。 **デバッグ バーを表示するには、このコールバックを挿入する必要があります。 **
  - パラメータ:
    - `type`: 挿入されたコンテンツ タイプ (`HTML = 1`、`JS = 2`)。
    - `text`: 子要素から抽出されたコード テキスト。
  - 戻り値: `[htm, css, js]`