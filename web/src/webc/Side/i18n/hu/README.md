# Oldalról kicsúsztatható fiókpanelek

- Belső görgetős struktúrák automatikus létrehozása
- Kattintson a külső területre az automatikus összecsukáshoz
- Csúsztassa balra és jobbra a kézmozdulatot a bezáráshoz
- Oldalsó csúszó pozicionálás bal és jobb irányban
- Nyílt és zárt események biztosítása

## Használd a demót

```html
<button class="open-btn">Nyitott</button>

<c-side class="right">
  <div>Fiók tartalma</div>
</c-side>

<script type="module">
import { open } from "webc.site/Side.js";

const btn = document.querySelector(".open-btn"),
  side = document.querySelector("c-side");

btn.onclick = () => open(side);
</script>
```

## felület

### módszer

- `open(el)`: Exportálási funkció, fiók megnyitása
- `close(el)`: Exportálási funkció, fiók bezárása
- `el.open()`: példánymódszer, fiók megnyitása
- `el.close()`: példánymódszer, zárja be a fiókot

### esemény

- `open`: Bekapcsolt állapotban világít
- `close`: Zárt állapotban tüzel

## stílus osztály

- `.right`: jobbra csúsztatható pozicionálás
- `.top`: Kicsúsztatható pozicionálás
- `.bottom`: csúsztassa ki a pozícióból
- (Ha nincs beállítva, az alapértelmezett a balra csúsztatva)
- `.open`: nyitott állapot
- `.H`: rejtett állapot
- `.T`: animáció átmeneti állapota