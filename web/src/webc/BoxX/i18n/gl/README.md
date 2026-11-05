# Crea e mostra unha capa emerxente cun botón de pechar

- Chamada funcional para crear e mostrar a capa emerxente
- Botón de peche integrado, fai clic para pechar e eliminar o elemento do DOM
- Preme Esc para desenfocar a caixa de entrada ou pechar a capa emerxente

## Use a demostración

```html
<button>Fai clic para aparecer</button>

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


## Parámetros da interface

A función de exportación predeterminada devolve `HTMLDialogElement` e engadíronse as clases de estilo `Box` e `X` ao elemento de diálogo.

## clase de estilo

### `.Box.X`

Aplicado ao elemento `dialog`, especifica o estilo de diálogo.

### `a.X`

Aplicado ao elemento `a`, especifica o estilo do botón pechar.