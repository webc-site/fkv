# Komponent demolarini, hujjatlarni va o'rnatish ko'rsatmalarini ko'rsatish va ko'rsatish

- README hujjatining sarlavhasi, kirish qismi va mazmunini ajratib oling va tahlil qiling
- Komponentlarning interaktiv demolarini ishga tushirish va uslublarni ajratish uchun Sandbox
- Hujjatlardagi kod bloklarini interaktiv taqdimotlarga aylantiring
- Moslashuvchan tartib, keng ekranli ustunli displey, tor ekranli yorliqni almashtirish displeyi

## Ko‘rsatma ishlatish

```html
<c-webc-demo></c-webc-demo>
```

```js
import WebCDemo from "./_.js";

const el = document.querySelector("c-webc-demo");

//Operatsiya interfeysini ishga tushiring va oling
const [setMod, setMd, unMount] = WebCDemo(el);
```

## Interfeys parametrlari

### `WebCDemo(el)`

Komponentni ishga tushiring.

- **parametr**
  - `el`:`HTMLElement`, komponentlar konteyneri.
- **Qaytish qiymati**:`[setMod, setMd, unMount]`
  - `setMod(mod)`: demo modulni sozlang.
  - `setMd(name, md, lang)`: Komponent nomini, README hujjat mazmunini va joriy tilni o'rnating.
  - `unMount()`: Komponentni yuklab oling va voqea tinglovchisini tozalang.

### Demo moduli `mod` atributi

- `CSS`:`string`, komponentning uslublar jadvali yo'li.
- `HTM`:`string`, komponentning HTML shabloni.
- `default`:`function(root)`, komponentni ishga tushirish funksiyasi, soya ildiz konteynerini qabul qiladi.

## uslublar sinfi

### `.M`

tarkib maydoni.

### `.L`

Chap panelda ko'rsatmalar va hujjatlar mavjud.

### `.R`

Demoni o'z ichiga olgan o'ng panel.

### `c-nav`

Tor ekranli navigatsiya paneli.