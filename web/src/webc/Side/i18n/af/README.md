# Laaiepanele wat van die kant af uitskuif

- Skep outomaties interne blaaistrukture
- Klik op die buitenste area om outomaties in te vou
- Swiep links en regs gebare om toe te maak
- Kantskuifposisionering in links en regs rigtings
- Verskaf oop en toe geleenthede

## Gebruik die demo

```html
<button class="open-btn">Maak oop</button>

<c-side class="right">
  <div>Laaie inhoud</div>
</c-side>

<script type="module">
import { open } from "webc.site/Side.js";

const btn = document.querySelector(".open-btn"),
  side = document.querySelector("c-side");

btn.onclick = () => open(side);
</script>
```

## koppelvlak

### metode

- `open(el)`: Uitvoerfunksie, maak laai oop
- `close(el)`: Uitvoerfunksie, maak laai toe
- `el.open()`: instansiemetode, maak laai oop
- `el.close()`: instansie metode, maak die laai toe

### gebeurtenis

- `open`: Vuur wanneer aangeskakel
- `close`: Vuur wanneer gesluit

## styl klas

- `.right`: regs skuif uit posisionering
- `.top`: Skuif posisionering uit
- `.bottom`: gly uit posisionering
- (Indien nie gestel nie, is die verstek om links uit te gly)
- `.open`: oop status
- `.H`: verborge toestand
- `.T`: animasie-oorgangstoestand