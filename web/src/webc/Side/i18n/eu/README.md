# Albotik ateratzen diren tiradera-panelak

- Sortu automatikoki barruko korritze-egiturak
- Egin klik kanpoko eremuan automatikoki tolesteko
- Irristatu ezkerrera eta eskuinera keinua ixteko
- Alboko irristaketa kokatzea ezkerreko eta eskuineko norabideetan
- Eman ireki eta itxi ekitaldiak

## Erabili demoa

```html
<button class="open-btn">Ireki</button>

<c-side class="right">
  <div>Tiraderaren edukia</div>
</c-side>

<script type="module">
import { open } from "webc.site/Side.js";

const btn = document.querySelector(".open-btn"),
  side = document.querySelector("c-side");

btn.onclick = () => open(side);
</script>
```

## interfazea

### metodoa

- `open(el)`: esportazio funtzioa, tiradera ireki
- `close(el)`: Esportatu funtzioa, itxi tiradera
- `el.open()`: instantzia metodoa, tiradera irekia
- `el.close()`: instantzia metodoa, itxi tiradera

### gertaera

- `open`: piztuta dagoenean pizten da
- `close`: Itxita dagoenean piztu egiten da

## estilo klasea

- `.right`: eskuineko lerratzea
- `.top`: Irristatu kokapena
- `.bottom`: irristatu kokapenetik
- (Ezartzen ez bada, lehenetsia ezkerrera lerratzea da)
- `.open`: irekita dagoen egoera
- `.H`: ezkutuko egoera
- `.T`: animazioaren trantsizio-egoera