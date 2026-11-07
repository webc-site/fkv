# Eskatu pop-up leihoa orriaren ertzean

- **Funtzio-deia**: interfaze normalak eta akatsak eskaintzen ditu
- **Pila automatikoa**: hainbat aholku automatikoki tartea kalkulatzen dute eta bertikalean pilatzen dira
- **Programatutako eta eskuz itzaltzea**: programatutako eta eskuzko itzaltzea onartzen du

## Erabili demoa

```js
import { toast, toastErr } from "webc.site/js/Toast.js";

toast((el) => {
  el.textContent = "Edukia eskatu";
});

toastErr((el) => {
  el.textContent = "Errorea edukia";
});

toast(
  (el) => {
    el.textContent = "Automatikoki itxi 99 segundoren buruan";
  },
  99
);
```

## Interfazearen parametroak

### toast(render, timeout)

Sortu eta bistaratu galderak.

- `render`: `(el) => void`, dei-itzulera errendatzen, `el` gonbidapen-kutxaren elementua da.
- `timeout`: `Number`, denbora-muga segundo. Lehenetsia 9. Ezarri 0 moduan automatikoki itzaltzeko.
- Itzultzeko balioa: gonbita-koadroaren elementua. `el.close()` desaktibatuta onartzen du.

### toastErr(render, timeout)

Sortu eta bistaratu errore-mezuak. Parametroak `toast` bezalakoak dira, `.ERR` estiloko klasearekin.

## estilo klasea

- `.Toast`: Galdera-kutxako oinarrizko klasea.
- `.ERR`: Errorearen egoera.
- `.animated` / `.fadeInR` / `.fadeOutR`: trantsizio animatua.
- `.x`: Itxi botoia.