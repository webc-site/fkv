# Kapsulekaart met regspyltjie vertoon in suspensie

- Beweeg die regterpyltjie
- Aktiewe blou hoogtepunt
- Klik om terugvoer te zoem

## Gebruik die demo

```html
<main style="display:flex;gap:16px">
  <a class="Card A" href="#">
    <h2>Titel 1</h2>
    <i>Beskrywing 1</i>
  </a>
  <a class="Card" href="#">
    <h2>Titel 2</h2>
    <i>Beskrywing 2</i>
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

## styl klas

- `.Card`: Basiese kaart
- `.Card.A`: Aktiveer uitlig
- `> h2`: Titel
- `> i`: beskrywing