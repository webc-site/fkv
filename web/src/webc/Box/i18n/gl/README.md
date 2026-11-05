# Crear e mostrar unha capa emerxente

- Chamada funcional para crear e mostrar a capa emerxente
- Elimina automaticamente elementos do DOM cando estea pechado
- Evitar o comportamento de cancelación da tecla Esc

## Use a demostración

```html
<button>Fai clic para aparecer</button>

<script type="module">
import Box from "webc.site/Box.js";

document.querySelector("button").onclick = () => {
  const dialog = Box(),
    content = document.createElement("div"),
    title = document.createElement("h3"),
    description = document.createElement("p"),
    button = document.createElement("button");

  content.className = "Lg";
  title.textContent = "Título";
  description.textContent = "Texto";
  button.className = "Btn Main";
  button.textContent = "Aceptar";
  button.onclick = () => dialog.close();
  content.append(title, description, button);
  dialog.append(content);
};
</script>
```

## Parámetros da interface

### Funcións exportadas predeterminadas

`Box()`

- **Valor de retorno**: `HTMLDialogElement`, engadiuse o elemento de diálogo da clase de estilo `Box`.

## clase de estilo

### `.Box`

Aplícase ao elemento `dialog` para establecer o posicionamento, o centrado e o desenfoque da máscara de fondo.

### `.Lg`

Unha clase de estilo aplicada ao contido interno, establecendo a cor de fondo, as esquinas redondeadas, a sombra e o deseño vertical, e definindo o deseño dos `h3` e `p` internos.