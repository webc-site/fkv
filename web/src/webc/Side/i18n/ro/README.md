# Panouri de sertare care alunecă din lateral

- Creați automat structuri interne de defilare
- Faceți clic pe zona exterioară pentru a restrânge automat
- Glisați la stânga și la dreapta gest pentru a închide
- Pozitionare alunecare laterala in directii stanga si dreapta
- Furnizați evenimente deschise și închise

## Utilizați demonstrația

```html
<button class="open-btn">Deschide</button>

<c-side class="right">
  <div>Conținutul sertarului</div>
</c-side>

<script type="module">
import { open } from "webc.site/Side.js";

const btn = document.querySelector(".open-btn"),
  side = document.querySelector("c-side");

btn.onclick = () => open(side);
</script>
```

## interfata

### metodă

- `open(el)`: funcție de export, sertar deschis
- `close(el)`: funcție de export, închideți sertarul
- `el.open()`: metoda de instanță, sertar deschis
- `el.close()`: metoda de instanță, închideți sertarul

### eveniment

- `open`: se declanșează când este pornit
- `close`: se declanșează când este închis

## clasa de stil

- `.right`: poziționare cu glisare spre dreapta
- `.top`: poziționarea glisați
- `.bottom`: glisați din poziționare
- (Dacă nu este setat, implicit este să alunece spre stânga)
- `.open`: stare deschisă
- `.H`: stare ascunsă
- `.T`: starea de tranziție a animației