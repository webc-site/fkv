# Painéis de gaveta que deslizam lateralmente

- Crie automaticamente estruturas de rolagem interna
- Clique na área externa para recolher automaticamente
- Deslize para a esquerda e para a direita para fechar
- Posicionamento deslizante lateral nas direções esquerda e direita
- Fornece eventos de abertura e fechamento

## Use a demonstração

```html
<button class="open-btn">Abrir</button>

<c-side class="right">
  <div>Conteúdo da gaveta</div>
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

- `open(el)`: Função de exportação, gaveta aberta
- `close(el)`: Função exportar, fechar gaveta
- `el.open()`: método de instância, gaveta aberta
- `el.close()`: método de instância, feche a gaveta

### evento

- `open`: Dispara quando ligado
- `close`: Dispara quando fechado

## aula de estilo

- `.right`: posicionamento deslizante à direita
- `.top`: posicionamento deslizante
- `.bottom`: deslizar para fora do posicionamento
- (Se não estiver definido, o padrão é deslizar para a esquerda)
- `.open`: status aberto
- `.H`: estado oculto
- `.T`: estado de transição da animação