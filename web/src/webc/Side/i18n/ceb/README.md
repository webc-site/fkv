# Mga panel sa drawer nga nag-slide gikan sa kilid

- Awtomatikong paghimo sa internal nga mga istruktura sa pag-scroll
- Pag-klik sa gawas nga lugar aron awtomatiko nga mahugno
- Pag-swipe sa wala ug tuo nga lihok aron pagsira
- Side sliding positioning sa wala ug tuo nga direksyon
- Paghatag og bukas ug suod nga mga panghitabo

## Gamita ang demo

```html
<button class="open-btn">Bukas</button>

<c-side class="right">
  <div>Mga sulod sa drawer</div>
</c-side>

<script type="module">
import { open } from "webc.site/Side.js";

const btn = document.querySelector(".open-btn"),
  side = document.querySelector("c-side");

btn.onclick = () => open(side);
</script>
```

## interface

### pamaagi

- `open(el)`: Export function, bukas nga drawer
- `close(el)`: Export function, close drawer
- `el.open()`: instance method, open drawer
- `el.close()`: instance method, isira ang drawer

### panghitabo

- `open`: Nagdilaab kung gi-on
- `close`: Masunog kung sirado

## estilo nga klase

- `.right`: right slide out positioning
- `.top`: I-slide pagawas ang positioning
- `.bottom`: pag-slide pagawas sa positioning
- (Kung dili itakda, ang default mao ang pag-slide sa wala)
- `.open`: bukas nga kahimtang
- `.H`: gitago nga kahimtang
- `.T`: kahimtang sa pagbalhin sa animation