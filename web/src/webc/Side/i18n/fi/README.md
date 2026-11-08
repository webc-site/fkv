# Sivulta ulos liukuvat laatikostopaneelit

- Luo automaattisesti sisäisiä vieritysrakenteita
- Tiivistä automaattisesti napsauttamalla ulkoaluetta
- Sulje pyyhkäisemällä vasemmalle ja oikealle
- Sivulle liukuva asento vasemmalle ja oikealle
- Järjestä avoimia ja suljettuja tapahtumia

## Käytä demoa

```html
<button class="open-btn">Avata</button>

<c-side class="right">
  <div>Laatikon sisältö</div>
</c-side>

<script type="module">
import { open } from "webc.site/Side.js";

const btn = document.querySelector(".open-btn"),
  side = document.querySelector("c-side");

btn.onclick = () => open(side);
</script>
```

## käyttöliittymä

### menetelmä

- `open(el)`: Vientitoiminto, avaa laatikko
- `close(el)`: Vientitoiminto, sulje laatikko
- `el.open()`: ilmentymämenetelmä, avaa laatikko
- `el.close()`: instanssimenetelmä, sulje laatikko

### tapahtuma

- `open`: Syttyy, kun se on päällä
- `close`: Syttyy, kun se suljetaan

## tyyliluokka

- `.right`: oikea liu'uttaminen ulos sijoittelusta
- `.top`: Liu'uta paikannus
- `.bottom`: liu'uta pois paikaltaan
- (Jos ei ole asetettu, oletusarvo on liukuminen ulos vasemmalle)
- `.open`: avoin tila
- `.H`: piilotettu tila
- `.T`: animaation siirtymätila