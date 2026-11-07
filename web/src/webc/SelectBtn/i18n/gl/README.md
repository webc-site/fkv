# Combinación de caixa de selección despregable e botón

- Empalme a caixa de selección e o botón horizontalmente
- Garda automaticamente o estado seleccionado en localStorage
- Interceptar os clics dos botóns e enviar eventos personalizados

## Use a demostración

```html
<c-select-btn key="my-select-key" value="option1">
  <c-select>
    <option value="option1">Opción 1</option>
    <option value="option2">Opción 2</option>
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

## propiedade

- `key`: nome da chave usado para a persistencia de LocalStorage
- `value`: valor seleccionado actualmente

## evento

- `submit`: enviado cando se fai clic no botón, `e.value` leva o valor seleccionado actualmente