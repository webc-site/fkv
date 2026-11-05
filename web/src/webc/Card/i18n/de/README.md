# Kapselkarte mit Pfeil nach rechts in der Schwebe

- Bewegen Sie den Mauszeiger über den rechten Pfeil
- Aktives blaues Highlight
- Klicken Sie hier, um das Feedback zu vergrößern

## Nutzen Sie die Demo

```html
<main style="display:flex;gap:16px">
  <a class="Card A" href="#">
    <h2>Titel 1</h2>
    <i>Beschreibung 1</i>
  </a>
  <a class="Card" href="#">
    <h2>Titel 2</h2>
    <i>Beschreibung 2</i>
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

## Stilklasse

- `.Card`: Basiskarte
- `.Card.A`: Hervorhebung aktivieren
- `> h2`: Titel
- `> i`: Beschreibung