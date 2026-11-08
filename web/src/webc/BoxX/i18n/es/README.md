# Crear y mostrar una capa emergente con un botón de cerrar

- Llamada funcional para crear y mostrar una capa emergente
- Botón de cierre incorporado, haga clic para cerrar y eliminar el elemento del DOM
- Presione Esc para desenfocar el cuadro de entrada o cerrar la capa emergente

## Usa la demostración

```html
<button>Haga clic para aparecer</button>

<script type="module">
import BoxX from "webc.site/BoxX.js";

document.querySelector("button").onclick = () => {
  const dialog = BoxX(),
    main = document.createElement("main"),
    h3 = document.createElement("h3");
  main.classList.add("Lg");
  h3.textContent = "texto";
  main.appendChild(h3);
  dialog.appendChild(main);
};
</script>
```


## Parámetros de interfaz

La función de exportación predeterminada devuelve `HTMLDialogElement` y las clases de estilo `Box` y `X` se han agregado al elemento de diálogo.

## clase de estilo

### `.Box.X`

Aplicado al elemento `dialog`, especifica el estilo del diálogo.

### `a.X`

Aplicado al elemento `a`, especifica el estilo del botón de cierre.