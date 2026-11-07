# Suspensioonis kuvatud paremnoolega kapslikaart

- Hõljutage kursorit paremale noolele
- Aktiivne sinine esiletõst
- Klõpsake tagasiside suumimiseks

## Kasutage demo

```html
<main style="display:flex;gap:16px">
  <a class="Card A" href="#">
    <h2>1. pealkiri</h2>
    <i>Kirjeldus 1</i>
  </a>
  <a class="Card" href="#">
    <h2>2. pealkiri</h2>
    <i>Kirjeldus 2</i>
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

## stiiliklass

- `.Card`: põhikaart
- `.Card.A`: esiletõstmise aktiveerimine
- `> h2`: pealkiri
- `> i`: kirjeldus