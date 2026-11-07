# კაფსულის ბარათი მარჯვენა ისრით ნაჩვენებია შეჩერებით

- გადაიტანეთ მარჯვენა ისარი
- აქტიური ლურჯი ხაზგასმა
- დააწკაპუნეთ გამოხმაურების გასადიდებლად

## გამოიყენეთ დემო ვერსია

```html
<main style="display:flex;gap:16px">
  <a class="Card A" href="#">
    <h2>სათაური 1</h2>
    <i>აღწერა 1</i>
  </a>
  <a class="Card" href="#">
    <h2>სათაური 2</h2>
    <i>აღწერა 2</i>
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

## სტილის კლასი

- `.Card`: ძირითადი ბარათი
- `.Card.A`: მონიშვნის გააქტიურება
- `> h2`: სათაური
- `> i`: აღწერა