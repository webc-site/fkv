# Combinação de caixa de seleção suspensa e botão

- Divida a caixa de seleção e o botão horizontalmente
- Salve automaticamente o estado selecionado em localStorage
- Interceptar cliques de botão e enviar eventos personalizados

## Use a demonstração

```html
<c-select-btn key="my-select-key" value="option1">
  <c-select>
    <option value="option1">Opção 1</option>
    <option value="option2">Opção 2</option>
  </c-select>
  <button>correr</button>
</c-select-btn>

<script type="module">
  import "webc.site/SelectBtn.js";
  const el = document.querySelector("c-select-btn");
  el.addEventListener("submit", (e) => {
    console.log(e.value);
  });
</script>
```

## propriedade

- `key`: nome da chave usada para persistência localStorage
- `value`: valor atualmente selecionado

## evento

- `submit`: enviado quando o botão é clicado, `e.value` carrega o valor atualmente selecionado