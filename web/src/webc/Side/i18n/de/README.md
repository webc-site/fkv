# Seitlich herausziehbare Schubladenpaneele

- Erstellen Sie automatisch interne Scrollstrukturen
- Klicken Sie auf den äußeren Bereich, um ihn automatisch zu verkleinern
- Zum Schließen wischen Sie mit der Geste nach links und rechts
- Seitlich verschiebbare Positionierung nach links und rechts
- Bieten Sie offene und geschlossene Veranstaltungen an

## Nutzen Sie die Demo

```html
<button class="open-btn">Offen</button>

<c-side class="right">
  <div>Schubladeninhalt</div>
</c-side>

<script type="module">
import { open } from "webc.site/Side.js";

const btn = document.querySelector(".open-btn"),
  side = document.querySelector("c-side");

btn.onclick = () => open(side);
</script>
```

## Schnittstelle

### Verfahren

- `open(el)`: Exportfunktion, Schublade öffnen
- `close(el)`: Exportfunktion, Schublade schließen
- `el.open()`: Instanzmethode, Schublade öffnen
- `el.close()`: Instanzmethode, Schublade schließen

### Ereignis

- `open`: Wird beim Einschalten ausgelöst
- `close`: Wird beim Schließen ausgelöst

## Stilklasse

- `.right`: rechte Slide-Out-Positionierung
- `.top`: Positionierung nach außen verschieben
- `.bottom`: Rutsche aus der Positionierung
- (Wenn nicht festgelegt, wird standardmäßig nach links herausgeschoben)
- `.open`: offener Status
- `.H`: ausgeblendeter Status
- `.T`: Übergangszustand der Animation