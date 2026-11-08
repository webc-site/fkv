# ಅಮಾನತಿನಲ್ಲಿ ಪ್ರದರ್ಶಿಸಲಾದ ಬಲ ಬಾಣದ ಕ್ಯಾಪ್ಸುಲ್ ಕಾರ್ಡ್

- ಬಲ ಬಾಣದ ಮೇಲೆ ಸುಳಿದಾಡಿ
- ಸಕ್ರಿಯ ನೀಲಿ ಹೈಲೈಟ್
- ಪ್ರತಿಕ್ರಿಯೆಯನ್ನು ಜೂಮ್ ಮಾಡಲು ಕ್ಲಿಕ್ ಮಾಡಿ

## ಡೆಮೊ ಬಳಸಿ

```html
<main style="display:flex;gap:16px">
  <a class="Card A" href="#">
    <h2>ಶೀರ್ಷಿಕೆ 1</h2>
    <i>ವಿವರಣೆ 1</i>
  </a>
  <a class="Card" href="#">
    <h2>ಶೀರ್ಷಿಕೆ 2</h2>
    <i>ವಿವರಣೆ 2</i>
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

## ಶೈಲಿ ವರ್ಗ

- `.Card`: ಮೂಲ ಕಾರ್ಡ್
- `.Card.A`: ಹೈಲೈಟ್ ಮಾಡುವುದನ್ನು ಸಕ್ರಿಯಗೊಳಿಸಿ
- `> h2`: ಶೀರ್ಷಿಕೆ
- `> i`: ವಿವರಣೆ