# 保留中に右矢印が表示されたカプセルカード

- 右矢印にカーソルを合わせます
- アクティブブルーハイライト
- クリックしてズームするフィードバック

## デモを使用する

```html
<main style="display:flex;gap:16px">
  <a class="Card A" href="#">
    <h2>タイトル1</h2>
    <i>説明1</i>
  </a>
  <a class="Card" href="#">
    <h2>タイトル2</h2>
    <i>説明2</i>
  </a>
</main>

<script type="module">
import "webc.site/Card.js";
import { On } from "webc.site/js/On.js";

const el = document.querySelector("main"),
  CARD = ".Card",
  A = "A",
  CARD_A = CARD + "." + A;

On(el, {
  click: (e) => {
    const card = e.target.closest(CARD);
    if (card) {
      const active = el.querySelector(CARD_A);
      if (active && active !== card) {
        active.classList.remove(A);
      }
      card.classList.add(A);
    }
  },
});
</script>
```

## スタイルクラス

- `.Card`: 基本カード
- `.Card.A`: ハイライト表示を有効にする
- `> h2`: タイトル
- `> i`: 説明