# Kad kapsul dengan anak panah kanan dipaparkan dalam penggantungan

- Tuding anak panah kanan
- Sorotan biru aktif
- Klik untuk zum maklum balas

## Gunakan demo

```html
<main style="display:flex;gap:16px">
  <a class="Card A" href="#">
    <h2>Tajuk 1</h2>
    <i>Penerangan 1</i>
  </a>
  <a class="Card" href="#">
    <h2>Tajuk 2</h2>
    <i>Penerangan 2</i>
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

## kelas gaya

- `.Card`: Kad asas
- `.Card.A`: Aktifkan penyerlahan
- `> h2`: Tajuk
- `> i`: perihalan