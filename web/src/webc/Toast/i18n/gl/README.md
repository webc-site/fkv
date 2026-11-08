# Solicita unha ventá emerxente no bordo da páxina

- **Chamada de función**: ofrece interfaces normais e de aviso de erro
- **Apilación automática**: varias suxestións calculan automaticamente o espazamento e apílanse verticalmente
- **Apagamento programado e manual**: admite o apagado programado e manual

## Use a demostración

```js
import { toast, toastErr } from "webc.site/js/Toast.js";

toast((el) => {
  el.textContent = "Contido de solicitude";
});

toastErr((el) => {
  el.textContent = "Contido do erro";
});

toast(
  (el) => {
    el.textContent = "Pechar automaticamente despois de 99 segundos";
  },
  99
);
```

## Parámetros da interface

### toast(render, timeout)

Crear e mostrar solicitudes.

- `render`: `(el) => void`, renderizando a devolución de chamada, `el` é o elemento da caixa de solicitudes.
- `timeout`: `Number`, tempo de espera segundos. Predeterminado 9. Establécese en 0 para que non se apague automaticamente.
- Valor de retorno: elemento de caixa de aviso. Admite `el.close()` desactivado.

### toastErr(render, timeout)

Crear e mostrar mensaxes de erro. Os parámetros son os mesmos que `toast`, coa clase de estilo `.ERR`.

## clase de estilo

- `.Toast`: clase base da caixa de solicitudes.
- `.ERR`: estado de erro.
- `.animated` / `.fadeInR` / `.fadeOutR`: transición animada.
- `.x`: botón Pechar.