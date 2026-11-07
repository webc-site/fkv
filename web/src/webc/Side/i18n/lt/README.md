# Iš šono išstumiamos stalčių plokštės

- Automatiškai sukurkite vidines slinkimo struktūras
- Spustelėkite išorinę sritį, kad automatiškai sutrauktumėte
- Perbraukite kairėn ir dešinėn, kad uždarytumėte
- Šoninis slydimo padėtis į kairę ir į dešinę
- Pateikite atvirus ir uždarus renginius

## Naudokite demonstracinę versiją

```html
<button class="open-btn">Atidaryti</button>

<c-side class="right">
  <div>Stalčių turinys</div>
</c-side>

<script type="module">
import { open } from "webc.site/Side.js";

const btn = document.querySelector(".open-btn"),
  side = document.querySelector("c-side");

btn.onclick = () => open(side);
</script>
```

## sąsaja

### metodas

- `open(el)`: eksportavimo funkcija, atidarykite stalčių
- `close(el)`: eksportavimo funkcija, uždarykite stalčių
- `el.open()`: egzemplioriaus metodas, atidarykite stalčių
- `el.close()`: egzemplioriaus metodas, uždarykite stalčių

### įvykis

- `open`: įsijungia įjungus
- `close`: užsidega, kai uždaroma

## stiliaus klasė

- `.right`: dešinėn išstumiama padėtis
- `.top`: išstumkite padėtį
- `.bottom`: išstumti iš padėties
- (Jei nenustatyta, numatytasis nustatymas yra išstumti į kairę)
- `.open`: atidaryta būsena
- `.H`: paslėpta būsena
- `.T`: animacijos perėjimo būsena