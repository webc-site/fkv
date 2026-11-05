# Pannelli tal-kxaxen li jiżżerżqu 'l barra mill-ġenb

- Oħloq awtomatikament strutturi interni ta 'scrolling
- Ikklikkja ż-żona ta 'barra biex tikkollassa awtomatikament
- Swipe ġest xellug u lemin biex tagħlaq
- Pożizzjonament li jiżżerżaq tal-ġenb fid-direzzjonijiet tax-xellug u tal-lemin
- Ipprovdi avvenimenti miftuħa u qrib

## Uża d-demo

```html
<button class="open-btn">Miftuħa</button>

<c-side class="right">
  <div>Kontenut tal-kexxun</div>
</c-side>

<script type="module">
import { open } from "webc.site/Side.js";

const btn = document.querySelector(".open-btn"),
  side = document.querySelector("c-side");

btn.onclick = () => open(side);
</script>
```

## interface

### metodu

- `open(el)`: Funzjoni ta 'esportazzjoni, kexxun miftuħ
- `close(el)`: Funzjoni ta 'esportazzjoni, kexxun mill-qrib
- `el.open()`: metodu ta' istanza, kexxun miftuħ
- `el.close()`: metodu ta 'istanza, agħlaq il-kexxun

### avveniment

- `open`: Nirien meta jinxtegħlu
- `close`: Nirien meta magħluqa

## klassi tal-istil

- `.right`: pożizzjonament tal-islajd tal-lemin
- `.top`: Żerżaq 'il barra pożizzjonament
- `.bottom`: slide barra mill-pożizzjonament
- (Jekk mhux issettjat, id-default huwa li tiżżerżaq 'il barra lejn ix-xellug)
- `.open`: status miftuħ
- `.H`: stat moħbi
- `.T`: stat ta' tranżizzjoni ta' animazzjoni