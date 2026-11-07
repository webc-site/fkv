# Rendern und Anzeigen von Komponentendemos, Dokumentation und Installationsanweisungen

- Extrahieren und analysieren Sie den Titel, die Einleitung und den Inhalt des README-Dokuments
- Sandbox zum Ausführen interaktiver Demos von Komponenten und zum Isolieren von Stilen
- Konvertieren Sie Codeblöcke in Dokumenten in interaktive Präsentationen
- Adaptives Layout, Spaltenanzeige auf breitem Bildschirm, Anzeige mit Tab-Umschaltung auf schmalem Bildschirm

## Nutzen Sie die Demo

```html
<c-webc-demo></c-webc-demo>
```

```js
import WebCDemo from "./_.js";

const el = document.querySelector("c-webc-demo");

// Initialisieren und erhalten Sie die Operationsschnittstelle
const [setMod, setMd, unMount] = WebCDemo(el);
```

## Schnittstellenparameter

### `WebCDemo(el)`

Initialisieren Sie die Komponente.

- **Parameter**
  - `el`:`HTMLElement`, Komponentencontainer.
- **Rückgabewert**:`[setMod, setMd, unMount]`
  - `setMod(mod)`: Richten Sie das Demomodul ein.
  - `setMd(name, md, lang)`: Legen Sie den Komponentennamen, den Inhalt des README-Dokuments und die aktuelle Sprache fest.
  - `unMount()`: Entladen Sie die Komponente und bereinigen Sie den Ereignis-Listener.

### Demomodul-Attribut `mod`

- `CSS`:`string`, der Stylesheet-Pfad der Komponente.
- `HTM`:`string`, die HTML-Vorlage der Komponente.
- `default`:`function(root)`, Komponenteninitialisierungsfunktion, empfängt den Schatten-Root-Container.

## Stilklasse

### `.M`

Inhaltsbereich.

### `.L`

Das linke Feld enthält Anweisungen und Dokumentation.

### `.R`

Rechtes Feld mit der Demo.

### `c-nav`

Schmale Navigationsleiste auf dem Bildschirm.