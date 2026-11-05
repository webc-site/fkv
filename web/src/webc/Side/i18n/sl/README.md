# Stransko izvlečne plošče predalov

- Samodejno ustvari notranje drsne strukture
- Kliknite zunanje območje, da se samodejno strne
- Za zapiranje povlecite levo in desno
- Bočno drsno pozicioniranje v levo in desno smer
- Zagotovite odprte in zaprte dogodke

## Uporabite demo

```html
<button class="open-btn">Odpri</button>

<c-side class="right">
  <div>Vsebina predala</div>
</c-side>

<script type="module">
import { open } from "webc.site/Side.js";

const btn = document.querySelector(".open-btn"),
  side = document.querySelector("c-side");

btn.onclick = () => open(side);
</script>
```

## vmesnik

### metoda

- `open(el)`: funkcija izvoza, odpiranje predala
- `close(el)`: funkcija izvoza, zapiranje predala
- `el.open()`: metoda primerka, odpri predal
- `el.close()`: metoda primerka, zaprite predal

### dogodek

- `open`: sproži se ob vklopu
- `close`: Proži, ko je zaprt

## stilski razred

- `.right`: desni drsnik navzven
- `.top`: Položaj drsnika
- `.bottom`: zdrs iz položaja
- (Če ni nastavljeno, je privzeto drsenje v levo)
- `.open`: odprto stanje
- `.H`: skrito stanje
- `.T`: prehodno stanje animacije