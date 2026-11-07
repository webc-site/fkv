# Ventana emergente emergente en el borde de la página

- **Llamada de función**: proporciona interfaces de aviso normal y de error
- **Apilamiento automático**: varias puntas calculan automáticamente el espacio y apilan verticalmente
- **Apagado programado y manual**: Admite apagado programado y manual

## Usa la demostración

```js
import { toast, toastErr } from "webc.site/js/Toast.js";

toast((el) => {
  el.textContent = "Contenido del mensaje";
});

toastErr((el) => {
  el.textContent = "Contenido del error";
});

toast(
  (el) => {
    el.textContent = "Cerrar automáticamente después de 99 segundos";
  },
  99
);
```

## Parámetros de interfaz

### toast(render, timeout)

Crear y mostrar mensajes.

- `render`: `(el) => void`, devolución de llamada, `el` es el elemento del cuadro de solicitud.
- `timeout`: `Number`, segundos de tiempo de espera. Predeterminado 9. Establezca en 0 para no apagarse automáticamente.
- Valor de retorno: elemento del cuadro de solicitud. Admite `el.close()` desactivado.

### toastErr(render, timeout)

Crear y mostrar mensajes de error. Los parámetros son los mismos que `toast`, con la clase de estilo `.ERR`.

## clase de estilo

- `.Toast`: Clase base del cuadro de aviso.
- `.ERR`: Estado de error.
- `.animated` / `.fadeInR` / `.fadeOutR`: transición animada.
- `.x`: Botón Cerrar.