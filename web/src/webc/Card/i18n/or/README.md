# ନିଲମ୍ବନରେ ପ୍ରଦର୍ଶିତ ଡାହାଣ ତୀର ସହିତ କ୍ୟାପସୁଲ କାର୍ଡ |

- ଡାହାଣ ତୀରକୁ ବୁଲାନ୍ତୁ |
- ସକ୍ରିୟ ନୀଳ ହାଇଲାଇଟ୍ |
- ମତାମତ ଜୁମ୍ କରିବାକୁ କ୍ଲିକ୍ କରନ୍ତୁ |

## ଡେମୋ ବ୍ୟବହାର କରନ୍ତୁ |

```html
<main style="display:flex;gap:16px">
  <a class="Card A" href="#">
    <h2>ଆଖ୍ୟା 1</h2>
    <i>ବର୍ଣ୍ଣନା 1</i>
  </a>
  <a class="Card" href="#">
    <h2>ଆଖ୍ୟା 2</h2>
    <i>ବର୍ଣ୍ଣନା 2</i>
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

## ଶ style ଳୀ ଶ୍ରେଣୀ

- `.Card`: ମ Basic ଳିକ କାର୍ଡ |
- `.Card.A`: ହାଇଲାଇଟ୍ ସକ୍ରିୟ କରନ୍ତୁ |
- `> h2`: ଆଖ୍ୟା |
- `> i`: ବର୍ଣ୍ଣନା |