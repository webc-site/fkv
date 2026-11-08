# Dirisha ibukizi la haraka kwenye ukingo wa ukurasa

- **Kazi ya simu**: Hutoa violesura vya kawaida na vya haraka vya hitilafu
- **Mrundikano otomatiki**: Vidokezo vingi hukokotoa nafasi kiotomatiki na mrundikano wima
- ** Uzimaji ulioratibiwa na wa mikono **: Inaauni kuzima kwa ratiba na kwa mikono

## Tumia onyesho

```js
import { toast, toastErr } from "webc.site/js/Toast.js";

toast((el) => {
  el.textContent = "Maudhui ya haraka";
});

toastErr((el) => {
  el.textContent = "Maudhui ya hitilafu";
});

toast(
  (el) => {
    el.textContent = "Funga kiotomatiki baada ya sekunde 99";
  },
  99
);
```

## Vigezo vya interface

### toast(render, timeout)

Unda na uonyeshe vidokezo.

- `render`: `(el) => void`, kutoa callback, `el` ndicho kipengele cha kisanduku cha kuuliza.
- `timeout`: `Number`, muda umeisha sekunde. Chaguo-msingi 9. Weka hadi 0 ili isizime kiotomatiki.
- Thamani ya kurejesha: kipengele cha kisanduku cha papo hapo. Inaauni `el.close()` kuzima.

### toastErr(render, timeout)

Unda na uonyeshe ujumbe wa makosa. Vigezo ni sawa na `toast`, na `.ERR` darasa la mtindo.

## darasa la mtindo

- `.Toast`: Darasa la msingi la kisanduku.
- `.ERR`: Hali ya hitilafu.
- `.animated` / `.fadeInR` / `.fadeOutR`: mpito uliohuishwa.
- `.x`: Kitufe cha kufunga.