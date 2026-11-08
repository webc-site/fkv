# Küljelt välja libisevad sahtlipaneelid

- Looge automaatselt sisemised kerimisstruktuurid
- Automaatseks ahendamiseks klõpsake välimist ala
- Sulgemiseks pühkige vasakule ja paremale liigutus
- Küljel libistatav positsioneerimine vasakule ja paremale
- Pakkuge avatud ja suletud üritusi

## Kasutage demo

```html
<button class="open-btn">Avatud</button>

<c-side class="right">
  <div>Sahtli sisu</div>
</c-side>

<script type="module">
import { open } from "webc.site/Side.js";

const btn = document.querySelector(".open-btn"),
  side = document.querySelector("c-side");

btn.onclick = () => open(side);
</script>
```

## liides

### meetod

- `open(el)`: ekspordifunktsioon, sahtel avamine
- `close(el)`: ekspordifunktsioon, sahtel sulgemine
- `el.open()`: eksemplari meetod, avatud sahtel
- `el.close()`: eksemplari meetod, sulgege sahtel

### sündmus

- `open`: süttib sisselülitamisel
- `close`: süttib sulgemisel

## stiiliklass

- `.right`: parempoolse väljalibisemise positsioneerimine
- `.top`: libistage positsioneerimine välja
- `.bottom`: libistage asukohast välja
- (Kui pole määratud, libistatakse vaikimisi vasakule välja)
- `.open`: avatud olek
- `.H`: peidetud olek
- `.T`: animatsiooni ülemineku olek