# Snel pop-upvenster aan de rand van de pagina

- **Functieoproep**: Biedt normale en foutpromptinterfaces
- **Automatisch stapelen**: Meerdere tips berekenen automatisch de afstand en stapelen verticaal
- **Geplande en handmatige uitschakeling**: Ondersteunt geplande en handmatige uitschakeling

## Gebruik de demo

```js
import { toast, toastErr } from "webc.site/js/Toast.js";

toast((el) => {
  el.textContent = "Promptinhoud";
});

toastErr((el) => {
  el.textContent = "Foutinhoud";
});

toast(
  (el) => {
    el.textContent = "Automatisch sluiten na 99 seconden";
  },
  99
);
```

## Interfaceparameters

### toast(render, timeout)

Aanwijzingen maken en weergeven.

- `render`: `(el) => void`, waardoor callback wordt weergegeven, `el` is het promptbox-element.
- `timeout`: `Number`, time-out seconden. Standaard 9. Stel in op 0 om niet automatisch uit te schakelen.
- Retourwaarde: promptbox-element. Ondersteunt `el.close()` uitgeschakeld.

### toastErr(render, timeout)

Foutmeldingen maken en weergeven. De parameters zijn hetzelfde als `toast`, met `.ERR` stijlklasse.

## stijl klasse

- `.Toast`: Basisklasse promptbox.
- `.ERR`: Foutstatus.
- `.animated` / `.fadeInR` / `.fadeOutR`: geanimeerde overgang.
- `.x`: knop Sluiten.