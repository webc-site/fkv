# Ponnahdusikkuna sivun reunassa

- **Toimintokutsu**: Tarjoaa normaalit ja virhekehoteliitännät
- **Automaattinen pinoaminen**: Useat kärjet laskevat automaattisesti välin ja pinoavat pystysuunnassa
- **Ajoitettu ja manuaalinen sammutus**: Tukee ajoitettua ja manuaalista sammutusta

## Käytä demoa

```js
import { toast, toastErr } from "webc.site/js/Toast.js";

toast((el) => {
  el.textContent = "Kehottaa sisältöä";
});

toastErr((el) => {
  el.textContent = "Virheellinen sisältö";
});

toast(
  (el) => {
    el.textContent = "Sulje automaattisesti 99 sekunnin kuluttua";
  },
  99
);
```

## Käyttöliittymän parametrit

### toast(render, timeout)

Luo ja näytä kehotteita.

- `render`: `(el) => void`, renderöi takaisinsoitto, `el` on kehoteruutuelementti.
- `timeout`: `Number`, aikakatkaisu sekunteja. Oletusasetus 9. Aseta arvoksi 0, jotta se ei sammu automaattisesti.
- Palautusarvo: kehoteruutuelementti. Tukee `el.close()` pois päältä.

### toastErr(render, timeout)

Luo ja näytä virheilmoituksia. Parametrit ovat samat kuin `toast`, tyyliluokalla `.ERR`.

## tyyliluokka

- `.Toast`: Kehotelaatikon perusluokka.
- `.ERR`: Virhetila.
- `.animated` / `.fadeInR` / `.fadeOutR`: animoitu siirtymä.
- `.x`: Sulje-painike.