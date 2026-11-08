# Carte capsule avec flèche droite affichée en suspension

- Passez la souris sur la flèche droite
- Surbrillance bleue active
- Cliquez pour agrandir les commentaires

## Utilisez la démo

```html
<main style="display:flex;gap:16px">
  <a class="Card A" href="#">
    <h2>Titre 1</h2>
    <i>Descriptif 1</i>
  </a>
  <a class="Card" href="#">
    <h2>Titre 2</h2>
    <i>Descriptif 2</i>
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

## cours de style

- `.Card` : Carte de base
- `.Card.A` : Activer la mise en surbrillance
- `> h2` : titre
- `> i` : description