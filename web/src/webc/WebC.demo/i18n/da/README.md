# Gengiv og vis komponentdemoer, dokumentation og installationsinstruktioner

- Uddrag og parse titlen, introduktionen og indholdet af README-dokumentet
- Sandbox til at køre interaktive demoer af komponenter og isolere stilarter
- Konverter kodeblokke i dokumenter til interaktive præsentationer
- Adaptivt layout, widescreen-søjlevisning, smal skærmfaneskiftevisning

## Brug demoen

```html
<c-webc-demo></c-webc-demo>
```

```js
import WebCDemo from "./_.js";

const el = document.querySelector("c-webc-demo");

//Initialiser og få operationsgrænsefladen
const [setMod, setMd, unMount] = WebCDemo(el);
```

## Interface parametre

### `WebCDemo(el)`

Initialiser komponenten.

- **parameter**
  - `el`:`HTMLElement`, komponentbeholder.
- **Returværdi**:`[setMod, setMd, unMount]`
  - `setMod(mod)`: Konfigurer demomodulet.
  - `setMd(name, md, lang)`: Indstil komponentnavnet, README-dokumentindholdet og det aktuelle sprog.
  - `unMount()`: Aflæs komponenten og ryd op i begivenhedslytteren.

### Demomodulets `mod` attribut

- `CSS`:`string`, stilarkstien til komponenten.
- `HTM`:`string`, komponentens HTML-skabelon.
- `default`:`function(root)`, komponentinitieringsfunktion, modtager skyggerodbeholder.

## stil klasse

### `.M`

indholdsområde.

### `.L`

Det venstre panel indeholder instruktioner og dokumentation.

### `.R`

Højre panel, der indeholder demoen.

### `c-nav`

Navigationslinje med smal skærm.