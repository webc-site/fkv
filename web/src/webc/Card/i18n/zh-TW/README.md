# 懸浮顯示右箭頭的膠囊卡片

- 懸浮顯示右側箭頭
- 激活態藍色高亮
- 點擊縮放回饋

## 使用演示

```html
<main style="display:flex;gap:16px">
  <a class="Card A" href="#">
    <h2>標題 1</h2>
    <i>描述 1</i>
  </a>
  <a class="Card" href="#">
    <h2>標題 2</h2>
    <i>描述 2</i>
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

## 樣式類

- `.Card`：基礎卡片
- `.Card.A`：活化高亮
- `> h2`：標題
- `> i`：描述