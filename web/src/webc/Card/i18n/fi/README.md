# Kapselikortti, jossa on oikea nuoli suspensiossa

- Vie hiiri oikealle osoittavaa nuolta
- Aktiivinen sininen korostus
- Zoomaa palautetta napsauttamalla

## Käytä demoa

```html
<main style="display:flex;gap:16px">
  <a class="Card A" href="#">
    <h2>Otsikko 1</h2>
    <i>Kuvaus 1</i>
  </a>
  <a class="Card" href="#">
    <h2>Otsikko 2</h2>
    <i>Kuvaus 2</i>
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

## tyyliluokka

- `.Card`: Peruskortti
- `.Card.A`: Aktivoi korostus
- `> h2`: Otsikko
- `> i`: kuvaus