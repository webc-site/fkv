# Lådpaneler som glider ut från sidan

- Skapa automatiskt interna rullningsstrukturer
- Klicka på det yttre området för att automatiskt komprimera
- Svep åt vänster och höger för att stänga
- Sidoglidande positionering i vänster och höger riktning
- Tillhandahålla öppna och stängda evenemang

## Använd demon

```html
<button class="open-btn">Öppna</button>

<c-side class="right">
  <div>Lådans innehåll</div>
</c-side>

<script type="module">
import { open } from "webc.site/Side.js";

const btn = document.querySelector(".open-btn"),
  side = document.querySelector("c-side");

btn.onclick = () => open(side);
</script>
```

## gränssnitt

### metod

- `open(el)`: Exportfunktion, öppna lådan
- `close(el)`: Exportfunktion, stäng lådan
- `el.open()`: instansmetod, öppna lådan
- `el.close()`: instansmetod, stäng lådan

### händelse

- `open`: Avfyras när den slås på
- `close`: Avfyras när den är stängd

## stilklass

- `.right`: höger utskjutbar positionering
- `.top`: Skjut ut positionering
- `.bottom`: glid ut ur positionen
- (Om inte inställt, är standard att glida ut till vänster)
- `.open`: öppen status
- `.H`: dolt tillstånd
- `.T`: animationsövergångstillstånd