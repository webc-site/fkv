# Kapsulės kortelė su rodykle dešinėn rodoma suspensijoje

- Užveskite rodyklę dešinėn
- Aktyvus mėlynas akcentas
- Spustelėkite, jei norite padidinti atsiliepimą

## Naudokite demonstracinę versiją

```html
<main style="display:flex;gap:16px">
  <a class="Card A" href="#">
    <h2>1 pavadinimas</h2>
    <i>1 aprašymas</i>
  </a>
  <a class="Card" href="#">
    <h2>2 antraštė</h2>
    <i>2 aprašymas</i>
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

## stiliaus klasė

- `.Card`: pagrindinė kortelė
- `.Card.A`: suaktyvinkite paryškinimą
- `> h2`: pavadinimas
- `> i`: aprašymas