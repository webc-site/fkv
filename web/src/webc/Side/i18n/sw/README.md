# Paneli za droo zinazoteleza kutoka upande

- Unda kiotomatiki miundo ya ndani ya kusogeza
- Bofya eneo la nje ili ukunje kiotomatiki
- Telezesha kidole kushoto na kulia ili kufunga
- Uwekaji wa kuteleza wa upande katika mwelekeo wa kushoto na kulia
- Kutoa matukio ya wazi na ya karibu

## Tumia onyesho

```html
<button class="open-btn">Fungua</button>

<c-side class="right">
  <div>Yaliyomo kwenye droo</div>
</c-side>

<script type="module">
import { open } from "webc.site/Side.js";

const btn = document.querySelector(".open-btn"),
  side = document.querySelector("c-side");

btn.onclick = () => open(side);
</script>
```

## kiolesura

### mbinu

- `open(el)`: Chaguo za kukokotoa, fungua droo
- `close(el)`: Chaguo za kukokotoa, funga droo
- `el.open()`: mbinu ya mfano, droo iliyofunguliwa
- `el.close()`: mbinu ya mfano, funga droo

### tukio

- `open`: Moto unapowashwa
- `close`: Moto unapofungwa

## darasa la mtindo

- `.right`: mkao wa kulia wa telezesha kidole nje
- `.top`: Telezesha nje nafasi
- `.bottom`: telezesha kutoka kwenye nafasi
- (Ikiwa haijawekwa, chaguo-msingi ni kutelezesha nje kushoto)
- `.open`: fungua hali
- `.H`: hali iliyofichwa
- `.T`: hali ya mpito ya uhuishaji