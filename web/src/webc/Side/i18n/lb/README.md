# Tirangplacke déi vun der Säit eraus rutschen

- Automatesch intern Scrollstrukturen erstellen
- Klickt op de baussenzege Beräich fir automatesch ze zerbriechen
- Swipe lénks a riets Geste fir zou ze maachen
- Side Rutsch Positionéierung a lénks a riets Richtungen
- Gitt oppen an zouen Eventer

## Benotzt d'Demo

```html
<button class="open-btn">Oppen</button>

<c-side class="right">
  <div>Tirang Inhalt</div>
</c-side>

<script type="module">
import { open } from "webc.site/Side.js";

const btn = document.querySelector(".open-btn"),
  side = document.querySelector("c-side");

btn.onclick = () => open(side);
</script>
```

## Interface

### Method

- `open(el)`: Exportfunktioun, Tirang opmaachen
- `close(el)`: Exportfunktioun, Tirang zoumaachen
- `el.open()`: Instanzmethod, Open Tirang
- `el.close()`: Instanzmethod, den Tirang zoumaachen

### Event

- `open`: Feiert wann ageschalt
- `close`: Feier wann zou

## Stil Klass

- `.right`: riets Rutsch eraus Positioun
- `.top`: Ausrutschen Positionéierung
- `.bottom`: aus der Positionéierung rutschen
- (Wann net agestallt ass, ass de Standard fir lénks eraus ze rutschen)
- `.open`: oppe Status
- `.H`: hidden state
- `.T`: Animatioun Iwwergangszoustand