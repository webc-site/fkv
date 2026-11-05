# ບັດແຄບຊູນທີ່ມີລູກສອນຂວາສະແດງຢູ່ໃນ suspension

- ເລື່ອນລູກສອນຂວາ
- ໄຮໄລ້ສີຟ້າທີ່ມີການເຄື່ອນໄຫວ
- ຄລິກເພື່ອຊູມຄຳຕິຊົມ

## ໃຊ້ຕົວຢ່າງ

```html
<main style="display:flex;gap:16px">
  <a class="Card A" href="#">
    <h2>ຫົວຂໍ້ 1</h2>
    <i>ລາຍລະອຽດ 1</i>
  </a>
  <a class="Card" href="#">
    <h2>ຫົວຂໍ້ 2</h2>
    <i>ລາຍລະອຽດ 2</i>
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

## ຫ້ອງຮຽນແບບ

- `.Card`: ບັດພື້ນຖານ
- `.Card.A`: ເປີດໃຊ້ການໄຮໄລ້
- `> h2`: ຊື່
- `> i`: ຄຳອະທິບາຍ