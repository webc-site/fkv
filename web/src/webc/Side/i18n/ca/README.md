# Panells del calaix que llisquen des del costat

- Creeu automàticament estructures de desplaçament intern
- Feu clic a la zona exterior per replegar automàticament
- Fes lliscar el dit cap a l'esquerra i dreta per tancar-lo
- Posicionament de lliscament lateral en direccions esquerra i dreta
- Proporcioneu esdeveniments oberts i tancats

## Utilitzeu la demostració

```html
<button class="open-btn">Obert</button>

<c-side class="right">
  <div>Contingut del calaix</div>
</c-side>

<script type="module">
import { open } from "webc.site/Side.js";

const btn = document.querySelector(".open-btn"),
  side = document.querySelector("c-side");

btn.onclick = () => open(side);
</script>
```

## interfície

### mètode

- `open(el)`: funció d'exportació, calaix obert
- `close(el)`: funció d'exportació, tanca el calaix
- `el.open()`: mètode d'instància, calaix obert
- `el.close()`: mètode d'instància, tanqueu el calaix

### esdeveniment

- `open`: dispara quan està activat
- `close`: dispara quan està tancat

## classe d'estil

- `.right`: posicionament cap a fora lliscant a la dreta
- `.top`: Posicionament lliscant cap a fora
- `.bottom`: llisca fora del posicionament
- (Si no s'estableix, el valor predeterminat és lliscar cap a l'esquerra)
- `.open`: estat obert
- `.H`: estat ocult
- `.T`: estat de transició de l'animació