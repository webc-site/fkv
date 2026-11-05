# Rendert a weist Komponentdemos, Dokumentatioun an Installatiounsinstruktiounen

- Extrait an parse den Titel, Aféierung an Inhalt vum README Dokument
- Sandbox fir interaktiv Demos vu Komponenten ze lafen an Stiler ze isoléieren
- Konvertéiert Codeblocken an Dokumenter an interaktive Presentatiounen
- Adaptive Layout, breet Écran Kolonn Display, schmuel Écran Tab schalt Display

## Benotzt d'Demo

```html
<c-webc-demo></c-webc-demo>
```

```js
import WebCDemo from "./_.js";

const el = document.querySelector("c-webc-demo");

// Initialiséiere a kritt d'Operatiounsinterface
const [setMod, setMd, unMount] = WebCDemo(el);
```

## Interface Parameteren

### `WebCDemo(el)`

Initialiséiere vun der Komponent.

- **parameter**
  - `el`:`HTMLElement`, Komponentbehälter.
- **Retourwäert**:`[setMod, setMd, unMount]`
  - `setMod(mod)`: Setzt den Demomodul op.
  - `setMd(name, md, lang)`: Setzt de Komponentnumm, README Dokumentinhalt an déi aktuell Sprooch.
  - `unMount()`: Luet de Komponent un a botzt den Eventnolauschterer.

### Demo Modul `mod` Attribut

- `CSS`:`string`, de Stilblattwee vun der Komponent.
- `HTM`:`string`, HTML-Schabloun vum Komponent.
- `default`:`function(root)`, Komponentinitialiséierungsfunktioun, kritt Schattenrootbehälter.

## Stil Klass

### `.M`

Inhalt Beräich.

### `.L`

Déi lénks Panel enthält Instruktiounen an Dokumentatioun.

### `.R`

Riets Panel, mat der Demo.

### `c-nav`

Schmuel Écran Navigatioun Bar.