# No sāniem izbīdāmi atvilktņu paneļi

- Automātiski izveidot iekšējās ritināšanas struktūras
- Noklikšķiniet uz ārējā apgabala, lai automātiski sakļautu
- Lai aizvērtu, velciet pa kreisi un pa labi
- Sānu bīdāma pozicionēšana kreisajā un labajā virzienā
- Nodrošiniet atklātus un slēgtus pasākumus

## Izmantojiet demonstrāciju

```html
<button class="open-btn">Atvērt</button>

<c-side class="right">
  <div>Atvilktņu saturs</div>
</c-side>

<script type="module">
import { open } from "webc.site/Side.js";

const btn = document.querySelector(".open-btn"),
  side = document.querySelector("c-side");

btn.onclick = () => open(side);
</script>
```

## saskarne

### metodi

- `open(el)`: eksportēšanas funkcija, atvilktnes atvēršana
- `close(el)`: eksportēšanas funkcija, aizveriet atvilktni
- `el.open()`: instances metode, atvērt atvilktni
- `el.close()`: instances metode, aizveriet atvilktni

### notikumu

- `open`: tiek aktivizēts, kad tas ir ieslēgts
- `close`: aizdegas, kad tas ir aizvērts

## stila klase

- `.right`: labās puses izbīdīšanas pozicionēšana
- `.top`: izbīdiet pozicionēšanu
- `.bottom`: izbīdīt no pozicionēšanas
- (Ja nav iestatīts, noklusējuma iestatījums ir izslīdēt pa kreisi)
- `.open`: atvērts statuss
- `.H`: slēptais stāvoklis
- `.T`: animācijas pārejas stāvoklis