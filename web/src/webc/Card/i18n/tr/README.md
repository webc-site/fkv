# Askıda sağ ok gösterilen kapsül kartı

- Sağ oku gezdirin
- Aktif mavi vurgu
- Geri bildirimi yakınlaştırmak için tıklayın

## Demoyu kullanın

```html
<main style="display:flex;gap:16px">
  <a class="Card A" href="#">
    <h2>Başlık 1</h2>
    <i>Açıklama 1</i>
  </a>
  <a class="Card" href="#">
    <h2>Başlık 2</h2>
    <i>Açıklama 2</i>
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

## stil sınıfı

- `.Card`: Temel kart
- `.Card.A`: Vurgulamayı etkinleştir
- `> h2`: Başlık
- `> i`: açıklama