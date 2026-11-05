# Renderējiet un parādiet komponentu demonstrācijas, dokumentāciju un instalēšanas instrukcijas

- Izņemiet un parsējiet README dokumenta nosaukumu, ievadu un saturu
- Sandbox, lai palaistu interaktīvas komponentu demonstrācijas un izolētu stilus
- Pārvērtiet koda blokus dokumentos interaktīvās prezentācijās
- Adaptīvs izkārtojums, plata ekrāna kolonnu displejs, šaura ekrāna ciļņu pārslēgšanas displejs

## Izmantojiet demonstrāciju

```html
<c-webc-demo></c-webc-demo>
```

```js
import WebCDemo from "./_.js";

const el = document.querySelector("c-webc-demo");

//Inicializējiet un iegūstiet darbības saskarni
const [setMod, setMd, unMount] = WebCDemo(el);
```

## Interfeisa parametri

### `WebCDemo(el)`

Inicializējiet komponentu.

- **parametrs**
  - `el`:`HTMLElement`, komponentu konteiners.
- **Atgriešanas vērtība**:`[setMod, setMd, unMount]`
  - `setMod(mod)`: iestatiet demonstrācijas moduli.
  - `setMd(name, md, lang)`: iestatiet komponenta nosaukumu, README dokumenta saturu un pašreizējo valodu.
  - `unMount()`: izlādējiet komponentu un notīriet notikumu uztvērēju.

### Demonstrācijas moduļa `mod` atribūts

- `CSS`:`string`, komponenta stila lapas ceļš.
- `HTM`:`string`, komponenta HTML veidne.
- `default`:`function(root)`, komponentu inicializācijas funkcija, saņem ēnas saknes konteineru.

## stila klase

### `.M`

satura apgabals.

### `.L`

Kreisajā panelī ir instrukcijas un dokumentācija.

### `.R`

Labais panelis, kurā ir demonstrācija.

### `c-nav`

Šaurā ekrāna navigācijas josla.