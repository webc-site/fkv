# 悬浮显示右箭头的胶囊卡片

- 悬浮显示右侧箭头
- 激活态蓝色高亮
- 点击缩放反馈

## 使用演示

```html
<main style="display:flex;gap:16px">
  <a class="Card A" href="#">
    <h2>标题 1</h2>
    <i>描述 1</i>
  </a>
  <a class="Card" href="#">
    <h2>标题 2</h2>
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

## 样式类

- `.Card`：基础卡片
- `.Card.A`：激活高亮
- `> h2`：标题
- `> i`：描述