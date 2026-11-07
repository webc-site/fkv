# การ์ดแคปซูลที่มีลูกศรขวาแสดงอยู่ในระบบแขวน

- เลื่อนลูกศรขวา
- ไฮไลต์สีน้ำเงินที่ใช้งานอยู่
- คลิกเพื่อซูมความคิดเห็น

## ใช้การสาธิต

```html
<main style="display:flex;gap:16px">
  <a class="Card A" href="#">
    <h2>ชื่อเรื่อง 1</h2>
    <i>คำอธิบาย 1</i>
  </a>
  <a class="Card" href="#">
    <h2>ชื่อเรื่อง 2</h2>
    <i>คำอธิบาย 2</i>
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

## คลาสสไตล์

- `.Card`: การ์ดพื้นฐาน
- `.Card.A`: เปิดใช้งานการไฮไลต์
- `> h2`: ชื่อเรื่อง
- `> i`: คำอธิบาย