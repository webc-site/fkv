# Paneli drôr sy'n llithro allan o'r ochr

- Creu strwythurau sgrolio mewnol yn awtomatig
- Cliciwch ar yr ardal allanol i gwympo'n awtomatig
- Sychwch yr ystum chwith a dde i gau
- Lleoliad llithro ochr i'r cyfeiriad chwith a dde
- Darparu digwyddiadau agored a chau

## Defnyddiwch y demo

```html
<button class="open-btn">Agored</button>

<c-side class="right">
  <div>Cynnwys drôr</div>
</c-side>

<script type="module">
import { open } from "webc.site/Side.js";

const btn = document.querySelector(".open-btn"),
  side = document.querySelector("c-side");

btn.onclick = () => open(side);
</script>
```

## rhyngwyneb

### dull

- `open(el)`: Swyddogaeth allforio, drôr agored
- `close(el)`: Swyddogaeth allforio, drôr cau
- `el.open()`: dull enghraifft, drôr agored
- `el.close()`: dull enghraifft, caewch y drôr

### digwyddiad

- `open`: Tanau pan gaiff ei droi ymlaen
- `close`: Tanau pan fydd ar gau

## dosbarth arddull

- `.right`: sleid dde lleoli allan
- `.top`: Llithro allan lleoli
- `.bottom`: llithro allan o leoliad
- (Os na chaiff ei osod, y rhagosodiad yw llithro allan i'r chwith)
- `.open`: statws agored
- `.H`: cyflwr cudd
- `.T`: cyflwr trawsnewid animeiddiad