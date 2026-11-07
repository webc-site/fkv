# Mga panel ng drawer na dumudulas mula sa gilid

- Awtomatikong lumikha ng mga panloob na istruktura ng pag-scroll
- I-click ang panlabas na bahagi upang awtomatikong i-collapse
- Mag-swipe pakaliwa at pakanan na galaw para isara
- Pagpoposisyon ng side sliding sa kaliwa at kanang direksyon
- Magbigay ng bukas at malapit na mga kaganapan

## Gamitin ang demo

```html
<button class="open-btn">Bukas</button>

<c-side class="right">
  <div>Mga nilalaman ng drawer</div>
</c-side>

<script type="module">
import { open } from "webc.site/Side.js";

const btn = document.querySelector(".open-btn"),
  side = document.querySelector("c-side");

btn.onclick = () => open(side);
</script>
```

## interface

### paraan

- `open(el)`: Export function, bukas na drawer
- `close(el)`: I-export ang function, isara ang drawer
- `el.open()`: instance method, bukas na drawer
- `el.close()`: instance method, isara ang drawer

### kaganapan

- `open`: Gumagana kapag naka-on
- `close`: Siga kapag sarado

## klase ng istilo

- `.right`: right slide out positioning
- `.top`: Slide out na pagpoposisyon
- `.bottom`: mag-slide palabas sa pagpoposisyon
- (Kung hindi nakatakda, ang default ay ang pag-slide palabas pakaliwa)
- `.open`: bukas na katayuan
- `.H`: nakatagong estado
- `.T`: estado ng paglipat ng animation