# Pag-aghat sa pop-up nga bintana sa kilid sa panid

- **Function call**: Naghatag ug normal ug error prompt interface
- **Auto stack**: Awtomatikong kalkulado sa daghang mga tip ang gilay-on ug patindog nga stack
- **Gi-iskedyul ug manwal nga pagsira**: Nagsuporta sa naka-iskedyul ug manwal nga pagsira

## Gamita ang demo

```js
import { toast, toastErr } from "webc.site/js/Toast.js";

toast((el) => {
  el.textContent = "Dali nga sulod";
});

toastErr((el) => {
  el.textContent = "Kasaypanan nga sulod";
});

toast(
  (el) => {
    el.textContent = "Awtomatiko nga pagsira pagkahuman sa 99 segundos";
  },
  99
);
```

## Mga parametro sa interface

### toast(render, timeout)

Paghimo ug ipakita ang mga prompt.

- `render`: `(el) => void`, pag-render og callback, `el` mao ang prompt nga elemento sa kahon.
- `timeout`: `Number`, timeout segundos. Default 9. Itakda sa 0 aron dili awtomatik nga mapalong.
- Ibalik nga bili: elemento sa prompt nga kahon. Gisuportahan ang `el.close()` off.

### toastErr(render, timeout)

Paghimo ug pagpakita sa mga mensahe sa sayup. Ang mga parametro parehas sa `toast`, nga adunay `.ERR` style class.

## estilo nga klase

- `.Toast`: Pag-aghat sa base nga klase sa kahon.
- `.ERR`: Error status.
- `.animated` / `.fadeInR` / `.fadeOutR`: animated nga transisyon.
- `.x`: Close button.