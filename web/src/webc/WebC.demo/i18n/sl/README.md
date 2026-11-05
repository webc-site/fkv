# Upodobite in prikažite predstavitve komponent, dokumentacijo in navodila za namestitev

- Izvlecite in razčlenite naslov, uvod in vsebino dokumenta README
- Peskovnik za izvajanje interaktivnih predstavitev komponent in izolacijo slogov
- Pretvorite kodne bloke v dokumentih v interaktivne predstavitve
- Prilagodljiva postavitev, širok zaslonski prikaz stolpcev, ozek zaslon preklapljanja med zavihki

## Uporabite demo

```html
<c-webc-demo></c-webc-demo>
```

```js
import WebCDemo from "./_.js";

const el = document.querySelector("c-webc-demo");

//Inicializiraj in pridobi operacijski vmesnik
const [setMod, setMd, unMount] = WebCDemo(el);
```

## Parametri vmesnika

### `WebCDemo(el)`

Inicializirajte komponento.

- **parameter**
  - `el`:`HTMLElement`, vsebnik komponent.
- **Vrnjena vrednost**:`[setMod, setMd, unMount]`
  - `setMod(mod)`: Nastavite predstavitveni modul.
  - `setMd(name, md, lang)`: Nastavite ime komponente, vsebino dokumenta README in trenutni jezik.
  - `unMount()`: Odstranite komponento in počistite poslušalca dogodkov.

### Atribut `mod` demo modula

- `CSS`:`string`, pot do lista slogov komponente.
- `HTM`:`string`, predloga HTML komponente.
- `default`:`function(root)`, funkcija inicializacije komponente, prejme senčni korenski vsebnik.

## stilski razred

### `.M`

vsebinsko področje.

### `.L`

Leva plošča vsebuje navodila in dokumentacijo.

### `.R`

Desna plošča, ki vsebuje demo.

### `c-nav`

Navigacijska vrstica ozkega zaslona.