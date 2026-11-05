# Combinación de cuadro de selección desplegable y botón

- Empalme el cuadro de selección y el botón horizontalmente.
- Guarde automáticamente el estado seleccionado en localStorage
- Interceptar clics en botones y enviar eventos personalizados

## Usa la demostración

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

## propiedad

- `key`: nombre de clave utilizado para la persistencia del almacenamiento local
- `value`: valor seleccionado actualmente

## evento

- `submit`: se envía cuando se hace clic en el botón, `e.value` lleva el valor seleccionado actualmente