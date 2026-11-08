# Renderujte a zobrazujte ukážky komponentov, dokumentáciu a pokyny na inštaláciu

- Extrahujte a analyzujte názov, úvod a obsah dokumentu README
- Sandbox na spustenie interaktívnych ukážok komponentov a izoláciu štýlov
- Preveďte bloky kódu v dokumentoch na interaktívne prezentácie
- Adaptívne rozloženie, širokouhlý stĺpcový displej, úzka obrazovka prepínania medzi kartami

## Použite demo

```html
<c-webc-demo></c-webc-demo>
```

```js
import WebCDemo from "./_.js";

const el = document.querySelector("c-webc-demo");

//Inicializácia a získanie operačného rozhrania
const [setMod, setMd, unMount] = WebCDemo(el);
```

## Parametre rozhrania

### `WebCDemo(el)`

Inicializujte komponent.

- **parameter**
  - `el`:`HTMLElement`, kontajner komponentov.
- **Vrátená hodnota**:`[setMod, setMd, unMount]`
  - `setMod(mod)`: Nastavte demo modul.
  - `setMd(name, md, lang)`: Nastavte názov komponentu, obsah dokumentu README a aktuálny jazyk.
  - `unMount()`: Uvoľnite komponent a vyčistite prijímač udalostí.

### Demo modul `mod` atribút

- `CSS`:`string`, cesta šablóny štýlov komponentu.
- `HTM`:`string`, šablóna HTML komponentu.
- `default`:`function(root)`, funkcia inicializácie komponentu, prijíma tieňový koreňový kontajner.

## štýlová trieda

### `.M`

oblasť obsahu.

### `.L`

Ľavý panel obsahuje pokyny a dokumentáciu.

### `.R`

Pravý panel s ukážkou.

### `c-nav`

Úzka navigačná lišta obrazovky.