# Skuffepaneler som glir ut fra siden

- Lag automatisk interne rullestrukturer
- Klikk på det ytre området for automatisk å skjule
- Sveip til venstre og høyre for å lukke
- Sideskyveposisjon i venstre og høyre retning
- Tilby åpne og lukkede arrangementer

## Bruk demoen

```html
<button class="open-btn">Åpne</button>

<c-side class="right">
  <div>Skuffens innhold</div>
</c-side>

<script type="module">
import { open } from "webc.site/Side.js";

const btn = document.querySelector(".open-btn"),
  side = document.querySelector("c-side");

btn.onclick = () => open(side);
</script>
```

## grensesnitt

### metode

- `open(el)`: Eksportfunksjon, åpen skuff
- `close(el)`: Eksportfunksjon, lukk skuffen
- `el.open()`: forekomstmetode, åpen skuff
- `el.close()`: forekomstmetode, lukk skuffen

### hendelse

- `open`: Utløses når den er slått på
- `close`: Avfyres når den er lukket

## stilklasse

- `.right`: høyre skyv ut posisjonering
- `.top`: Skyv ut plassering
- `.bottom`: skyv ut av posisjonering
- (Hvis ikke angitt, er standard å skyve ut til venstre)
- `.open`: åpen status
- `.H`: skjult tilstand
- `.T`: animasjonsovergangstilstand