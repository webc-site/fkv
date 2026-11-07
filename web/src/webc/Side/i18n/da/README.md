# Skuffepaneler, der glider ud fra siden

- Opret automatisk interne rullestrukturer
- Klik på det ydre område for automatisk at skjule
- Stryg til venstre og højre for at lukke
- Sideskydende positionering i venstre og højre retning
- Tilbyder åbne og lukkede arrangementer

## Brug demoen

```html
<button class="open-btn">Åben</button>

<c-side class="right">
  <div>Skuffe indhold</div>
</c-side>

<script type="module">
import { open } from "webc.site/Side.js";

const btn = document.querySelector(".open-btn"),
  side = document.querySelector("c-side");

btn.onclick = () => open(side);
</script>
```

## interface

### metode

- `open(el)`: Eksportfunktion, åben skuffe
- `close(el)`: Eksportfunktion, luk skuffe
- `el.open()`: instansmetode, åben skuffe
- `el.close()`: instansmetode, luk skuffen

### tilfælde

- `open`: Udløses, når den er tændt
- `close`: Udløses, når den er lukket

## stil klasse

- `.right`: højre skyde ud positionering
- `.top`: Skub positionering ud
- `.bottom`: glid ud af position
- (Hvis ikke indstillet, er standardindstillingen at glide ud til venstre)
- `.open`: åben status
- `.H`: skjult tilstand
- `.T`: animationsovergangstilstand