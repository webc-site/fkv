# ページがスクロールすると自動的に折りたたまれたり展開したりする上部のナビゲーション バー

- **自動非表示**: ページを下にスクロールすると自動的に非表示になります。
- **自動展開**: ページが上にスクロールされるか、スクロール バーが消えると自動的に表示されます。
- **パフォーマンスの最適化**: `requestAnimationFrame` を使用してスクロールを調整します。
- **リソースの解放**: イベント リスナーとオブザーバーは、コンポーネントが破棄されると自動的に削除されます。

## デモを使用する

```html
<main>
  <c-vs>
    <nav class="Nav">
      <c-nav>
        <a class="logo" href="#">LOGO</a>
      </c-nav>
      <c-nav>
        <a class="item" href="#">リンク</a>
      </c-nav>
    </nav>
    <div class="content">ナビゲーションを非表示にするには下にスワイプし、表示するには上にスワイプします</div>
  </c-vs>
</main>
<style>
  main {
    position: relative;
    height: 300px;
    border: 1px solid #000;
  }
  nav.Nav > c-nav {
    background: #fffc;
    backdrop-filter: blur(4px);
    border-bottom: 1px solid #ccc;
  }
  .content {
    height: 200dvh;
    background: linear-gradient(to bottom, #fff, #ccc);
    padding: 80px 20px 20px;
    color: #666;
    text-align: center;
  }
</style>
```

## 使用説明書

`<c-vs>` の子要素または子孫要素として使用する必要があります。

## スタイルステータス

コンポーネントは`nav.Nav`で次のクラス名を切り替えます。

- `D`: ステータスを表示します (下にスライドするか上部にスライドします)。
- `I`: 非表示の遷移状態。
- `H`: 非表示ステータス。

## スタイル変数

- `--top`: コンポーネントによって自動的に高さの負の値に更新され、位置決めと遷移に使用されます。