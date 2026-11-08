# Gjengi og vis komponentdemoer, dokumentasjon og installasjonsinstruksjoner

- Trekk ut og analyser tittelen, introduksjonen og innholdet til README-dokumentet
- Sandbox for å kjøre interaktive demoer av komponenter og isolere stiler
- Konverter kodeblokker i dokumenter til interaktive presentasjoner
- Adaptiv layout, bred skjerm kolonnevisning, smal skjermfanebyttevisning

## Bruk demoen

```html
<c-webc-demo></c-webc-demo>
```

```js
import WebCDemo from "./_.js";

const el = document.querySelector("c-webc-demo");

//Initialiser og hent operasjonsgrensesnittet
const [setMod, setMd, unMount] = WebCDemo(el);
```

## Grensesnittparametere

### `WebCDemo(el)`

Initialiser komponenten.

- **parameter**
  - `el`:`HTMLElement`, komponentbeholder.
- **Returverdi**:`[setMod, setMd, unMount]`
  - `setMod(mod)`: Sett opp demomodulen.
  - `setMd(name, md, lang)`: Angi komponentnavn, README-dokumentinnhold og gjeldende språk.
  - `unMount()`: Last ned komponenten og ryd opp i hendelseslytteren.

### Demomodul `mod` attributt

- `CSS`:`string`, stilarkbanen til komponenten.
- `HTM`:`string`, komponentens HTML-mal.
- `default`:`function(root)`, komponentinitialiseringsfunksjon, mottar skyggerotbeholder.

## stilklasse

### `.M`

innholdsområde.

### `.L`

Det venstre panelet inneholder instruksjoner og dokumentasjon.

### `.R`

Høyre panel, som inneholder demoen.

### `c-nav`

Navigasjonslinje for smal skjerm.