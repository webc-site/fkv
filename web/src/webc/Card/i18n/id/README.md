# Kartu kapsul dengan panah kanan ditampilkan dalam suspensi

- Arahkan panah ke kanan
- Sorotan biru aktif
- Klik untuk memperbesar masukan

## Gunakan demonya

```html
<main style="display:flex;gap:16px">
  <a class="Card A" href="#">
    <h2>Judul 1</h2>
    <i>Deskripsi 1</i>
  </a>
  <a class="Card" href="#">
    <h2>Judul 2</h2>
    <i>Deskripsi 2</i>
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

- `.Card`: Kartu dasar
- `.Card.A`: Mengaktifkan penyorotan
- `> h2`: Judul
- `> i`: deskripsi