# I-prompt ang pop-up window sa gilid ng page

- **Function call**: Nagbibigay ng normal at error prompt interface
- **Auto stack**: Awtomatikong kalkulahin ng maraming tip ang spacing at stack nang patayo
- **Naka-iskedyul at manu-manong pag-shutdown**: Sinusuportahan ang naka-iskedyul at manu-manong pag-shutdown

## Gamitin ang demo

```js
import { toast, toastErr } from "webc.site/js/Toast.js";

toast((el) => {
  el.textContent = "Prompt na nilalaman";
});

toastErr((el) => {
  el.textContent = "Nilalaman ng error";
});

toast(
  (el) => {
    el.textContent = "Awtomatikong isara pagkatapos ng 99 segundo";
  },
  99
);
```

## Mga parameter ng interface

### toast(render, timeout)

Gumawa at magpakita ng mga prompt.

- `render`: `(el) => void`, nagre-render ng callback, `el` ay ang elemento ng prompt na kahon.
- `timeout`: `Number`, mga segundo ng timeout. Default 9. Itakda sa 0 upang hindi awtomatikong isara.
- Ibinalik na halaga: elemento ng prompt na kahon. Sinusuportahan ang `el.close()` off.

### toastErr(render, timeout)

Lumikha at magpakita ng mga mensahe ng error. Ang mga parameter ay pareho sa `toast`, na may `.ERR` style class.

## klase ng istilo

- `.Toast`: Prompt box base class.
- `.ERR`: Status ng error.
- `.animated` / `.fadeInR` / `.fadeOutR`: animated na paglipat.
- `.x`: Button na isara.