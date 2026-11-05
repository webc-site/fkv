# निलंबनामध्ये उजव्या बाणासह कॅप्सूल कार्ड प्रदर्शित केले आहे

- उजवा बाण फिरवा
- सक्रिय निळा हायलाइट
- फीडबॅक झूम करण्यासाठी क्लिक करा

## डेमो वापरा

```html
<main style="display:flex;gap:16px">
  <a class="Card A" href="#">
    <h2>शीर्षक १</h2>
    <i>वर्णन १</i>
  </a>
  <a class="Card" href="#">
    <h2>शीर्षक 2</h2>
    <i>वर्णन २</i>
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

## शैली वर्ग

- `.Card`: मूलभूत कार्ड
- `.Card.A`: हायलाइटिंग सक्रिय करा
- `> h2`: शीर्षक
- `> i`: वर्णन