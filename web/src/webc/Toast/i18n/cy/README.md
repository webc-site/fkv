# Ffenestr naid brydlon ar ymyl y dudalen

- **Galwad swyddogaeth**: Yn darparu rhyngwynebau arferol a chyfeiliornus
- ** Pentwr awtomatig **: Mae awgrymiadau lluosog yn cyfrifo'r gofod a'r pentwr yn fertigol yn awtomatig
- **Cau i lawr wedi'i amserlennu a llaw**: Yn cefnogi cau â llaw wedi'i amserlennu

## Defnyddiwch y demo

```js
import { toast, toastErr } from "webc.site/js/Toast.js";

toast((el) => {
  el.textContent = "Cynnwys prydlon";
});

toastErr((el) => {
  el.textContent = "Gwall cynnwys";
});

toast(
  (el) => {
    el.textContent = "Cau'n awtomatig ar ôl 99 eiliad";
  },
  99
);
```

## Paramedrau rhyngwyneb

### toast(render, timeout)

Creu ac arddangos awgrymiadau.

- `render`: `(el) => void`, yn gwneud galwad yn ôl, `el` yw'r elfen blwch prydlon.
- `timeout`: `Number`, goramser eiliadau. Diofyn 9. Gosodwch i 0 i beidio â chau i lawr yn awtomatig.
- Gwerth dychwelyd: elfen blwch prydlon. Yn cefnogi `el.close()` i ffwrdd.

### toastErr(render, timeout)

Creu ac arddangos negeseuon gwall. Mae'r paramedrau yr un fath â `toast`, gyda `.ERR` dosbarth arddull.

## dosbarth arddull

- `.Toast`: Dosbarth sylfaen blwch prydlon.
- `.ERR`: Statws gwall.
- `.animated` / `.fadeInR` / `.fadeOutR`: trawsnewidiad animeiddiedig.
- `.x`: Cau'r botwm.