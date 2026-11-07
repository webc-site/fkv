# Crear y mostrar capa emergente

- Llamada funcional para crear y mostrar una capa emergente
- Eliminar automáticamente elementos del DOM cuando está cerrado
- Evitar el comportamiento de cancelación de la tecla Esc

## Usa la demostración

```html
<button>Haga clic para aparecer</button>

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
  descripción.textContent = "Texto";
  button.className = "Btn Main";
  button.textContent = "Aceptar";
  button.onclick = () => dialog.close();
  content.append(title, description, button);
  dialog.append(content);
};
</script>
```

## Parámetros de interfaz

### Funciones exportadas predeterminadas

`Box()`

- **Valor de retorno**: `HTMLDialogElement`, se ha agregado el elemento de diálogo de la clase de estilo `Box`.

## clase de estilo

### `.Box`

Se aplica al elemento `dialog` para establecer el posicionamiento, el centrado y el desenfoque de la máscara de fondo.

### `.Lg`

Una clase de estilo aplicada al contenido interno, que establece el color de fondo, las esquinas redondeadas, la sombra y el diseño vertical, y define el diseño de los `h3` y `p` internos.