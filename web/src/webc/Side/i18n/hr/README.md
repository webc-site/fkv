# Ploče ladice koje klize sa strane

- Automatski stvarajte unutarnje strukture pomicanja
- Pritisnite vanjsko područje za automatsko sažimanje
- Pokret prsta lijevo i desno za zatvaranje
- Bočno klizno pozicioniranje u lijevom i desnom smjeru
- Osigurajte otvorene i zatvorene događaje

## Koristite demo

```html
<button class="open-btn">Otvoriti</button>

<c-side class="right">
  <div>Sadržaj ladice</div>
</c-side>

<script type="module">
import { open } from "webc.site/Side.js";

const btn = document.querySelector(".open-btn"),
  side = document.querySelector("c-side");

btn.onclick = () => open(side);
</script>
```

## sučelje

### metoda

- `open(el)`: Funkcija izvoza, otvaranje ladice
- `close(el)`: Funkcija izvoza, zatvaranje ladice
- `el.open()`: metoda instance, otvorena ladica
- `el.close()`: metoda instance, zatvori ladicu

### događaj

- `open`: Pali se kad je uključen
- `close`: Pali kada je zatvoren

## stilska klasa

- `.right`: desno klizanje prema van
- `.top`: Klizno pozicioniranje
- `.bottom`: izvucite iz položaja
- (Ako nije postavljeno, zadano je klizanje ulijevo)
- `.open`: otvoren status
- `.H`: skriveno stanje
- `.T`: prijelazno stanje animacije