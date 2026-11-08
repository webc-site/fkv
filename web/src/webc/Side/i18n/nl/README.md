# Ladepanelen die vanaf de zijkant uitschuiven

- Creëer automatisch interne scrollstructuren
- Klik op het buitenste gebied om automatisch samen te vouwen
- Veeg naar links en rechts om te sluiten
- Zijwaarts glijdende positionering in linker- en rechterrichting
- Zorg voor open en close-evenementen

## Gebruik de demo

```html
<button class="open-btn">Open</button>

<c-side class="right">
  <div>Inhoud lade</div>
</c-side>

<script type="module">
import { open } from "webc.site/Side.js";

const btn = document.querySelector(".open-btn"),
  side = document.querySelector("c-side");

btn.onclick = () => open(side);
</script>
```

## interface

### methode

- `open(el)`: Exportfunctie, lade openen
- `close(el)`: Exportfunctie, lade sluiten
- `el.open()`: instantiemethode, open lade
- `el.close()`: instantiemethode, sluit de lade

### evenement

- `open`: Wordt geactiveerd wanneer ingeschakeld
- `close`: gaat af wanneer gesloten

## stijl klasse

- `.right`: positionering voor uitschuiven naar rechts
- `.top`: Uitschuifpositie
- `.bottom`: uit positie glijden
- (Als dit niet is ingesteld, is de standaardinstelling om naar links uit te schuiven)
- `.open`: open status
- `.H`: verborgen status
- `.T`: animatie-overgangsstatus