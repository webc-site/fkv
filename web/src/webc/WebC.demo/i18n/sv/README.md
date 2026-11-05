# Återge och visa komponentdemonstrationer, dokumentation och installationsinstruktioner

- Extrahera och analysera titeln, inledningen och innehållet i README-dokumentet
- Sandlåda för att köra interaktiva demos av komponenter och isolera stilar
- Konvertera kodblock i dokument till interaktiva presentationer
- Adaptiv layout, bred skärm kolumnvisning, smal skärm flikbyte

## Använd demon

```html
<c-webc-demo></c-webc-demo>
```

```js
import WebCDemo from "./_.js";

const el = document.querySelector("c-webc-demo");

//Initiera och erhålla operationsgränssnittet
const [setMod, setMd, unMount] = WebCDemo(el);
```

## Gränssnittsparametrar

### `WebCDemo(el)`

Initiera komponenten.

- **parameter**
  - `el`:`HTMLElement`, komponentbehållare.
- **Returvärde**:`[setMod, setMd, unMount]`
  - `setMod(mod)`: Konfigurera demomodulen.
  - `setMd(name, md, lang)`: Ställ in komponentnamn, README-dokumentinnehåll och aktuellt språk.
  - `unMount()`: Ladda ur komponenten och rensa upp händelseavlyssnaren.

### Demomodul `mod` attribut

- `CSS`:`string`, stilmallsvägen för komponenten.
- `HTM`:`string`, komponentens HTML-mall.
- `default`:`function(root)`, komponentinitieringsfunktion, tar emot skuggrotbehållare.

## stilklass

### `.M`

innehållsområde.

### `.L`

Den vänstra panelen innehåller instruktioner och dokumentation.

### `.R`

Höger panel, som innehåller demon.

### `c-nav`

Smal skärm navigeringsfält.