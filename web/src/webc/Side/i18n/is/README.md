# Skúffuplötur sem renna út frá hliðinni

- Búðu til sjálfkrafa innri skrunbyggingar
- Smelltu á ytra svæðið til að hrynja sjálfkrafa saman
- Strjúktu til vinstri og hægri til að loka
- Hliðarrennistaða í vinstri og hægri átt
- Bjóða upp á opna og lokaða viðburði

## Notaðu kynninguna

```html
<button class="open-btn">Opið</button>

<c-side class="right">
  <div>Innihald skúffu</div>
</c-side>

<script type="module">
import { open } from "webc.site/Side.js";

const btn = document.querySelector(".open-btn"),
  side = document.querySelector("c-side");

btn.onclick = () => open(side);
</script>
```

## viðmót

### aðferð

- `open(el)`: Útflutningsaðgerð, opna skúffu
- `close(el)`: Útflutningsaðgerð, lokaðu skúffu
- `el.open()`: tilviksaðferð, opna skúffu
- `el.close()`: tilviksaðferð, lokaðu skúffunni

### atburður

- `open`: Kveikir þegar kveikt er á því
- `close`: Kviknar þegar lokað er

## stílaflokkur

- `.right`: hægri renna út staðsetningu
- `.top`: Renndu staðsetningu út
- `.bottom`: renna út úr staðsetningu
- (Ef það er ekki stillt er sjálfgefið að renna út til vinstri)
- `.open`: opinn staða
- `.H`: falið ástand
- `.T`: umbreytingarástand hreyfimynda