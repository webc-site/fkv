# Sol·licita una finestra emergent a la vora de la pàgina

- **Trucada de funció**: proporciona interfícies d'indicació d'errors normals
- **Apilació automàtica**: diversos consells calculen automàticament l'espaiat i s'apilen verticalment
- **Apagat programat i manual**: Admet tancament programat i manual

## Utilitzeu la demostració

```js
import { toast, toastErr } from "webc.site/js/Toast.js";

toast((el) => {
  el.textContent = "Contingut sol·licitat";
});

toastErr((el) => {
  el.textContent = "Contingut d'error";
});

toast(
  (el) => {
    el.textContent = "Tancar automàticament després de 99 segons";
  },
  99
);
```

## Paràmetres de la interfície

### toast(render, timeout)

Crear i mostrar instruccions.

- `render`: `(el) => void`, que representa una devolució de trucada, `el` és l'element del quadre de sol·licitud.
- `timeout`: `Number`, temps d'espera segons. Per defecte 9. Establiu a 0 perquè no s'apaga automàticament.
- Valor de retorn: element del quadre de sol·licitud. Admet `el.close()` desactivat.

### toastErr(render, timeout)

Crear i mostrar missatges d'error. Els paràmetres són els mateixos que `toast`, amb la classe d'estil `.ERR`.

## classe d'estil

- `.Toast`: classe base del quadre de sol·licitud.
- `.ERR`: estat d'error.
- `.animated` / `.fadeInR` / `.fadeOutR`: transició animada.
- `.x`: botó Tanca.