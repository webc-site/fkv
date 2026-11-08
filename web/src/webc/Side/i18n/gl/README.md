# Paneis de caixóns que se deslizan cara a fóra do lado

- Crea automaticamente estruturas de desprazamento interno
- Fai clic na zona exterior para contraer automaticamente
- Pasa o dedo cara á esquerda e á dereita para pechar
- Posicionamento de deslizamento lateral en direccións esquerda e dereita
- Proporcionar eventos abertos e pechados

## Use a demostración

```html
<button class="open-btn">Aberto</button>

<c-side class="right">
  <div>Contido do caixón</div>
</c-side>

<script type="module">
import { open } from "webc.site/Side.js";

const btn = document.querySelector(".open-btn"),
  side = document.querySelector("c-side");

btn.onclick = () => open(side);
</script>
```

## interface

### método

- `open(el)`: función de exportación, caixón aberto
- `close(el)`: función de exportación, pecha o caixón
- `el.open()`: método de instancia, caixón aberto
- `el.close()`: método de instancia, pecha o caixón

### evento

- `open`: dispara cando se acende
- `close`: dispara cando está pechado

## clase de estilo

- `.right`: posición deslizante cara á dereita
- `.top`: desliza cara a fóra
- `.bottom`: desliza fóra do posicionamento
- (Se non está definido, o predeterminado é desprazar cara á esquerda)
- `.open`: estado aberto
- `.H`: estado oculto
- `.T`: estado de transición da animación