# Piirrä ja näytä komponenttien esittelyt, dokumentaatio ja asennusohjeet

- Pura ja jäsennä README-dokumentin otsikko, johdanto ja sisältö
- Hiekkalaatikko komponenttien interaktiivisten demojen suorittamiseen ja tyylien eristämiseen
- Muunna asiakirjojen koodilohkot interaktiivisiksi esityksiksi
- Mukautuva asettelu, laajakuvasarakkeen näyttö, kapea näyttö välilehtien vaihtonäyttö

## Käytä demoa

```html
<c-webc-demo></c-webc-demo>
```

```js
import WebCDemo from "./_.js";

const el = document.querySelector("c-webc-demo");

//Alusta ja hanki käyttöliittymä
const [setMod, setMd, unMount] = WebCDemo(el);
```

## Käyttöliittymän parametrit

### `WebCDemo(el)`

Alusta komponentti.

- **parametri**
  - `el`:`HTMLElement`, komponenttisäiliö.
- **Palautusarvo**:`[setMod, setMd, unMount]`
  - `setMod(mod)`: Asenna esittelymoduuli.
  - `setMd(name, md, lang)`: Aseta komponentin nimi, README-asiakirjan sisältö ja nykyinen kieli.
  - `unMount()`: Pura komponentti ja puhdista tapahtumaseura.

### Demomoduulin `mod` attribuutti

- `CSS`:`string`, komponentin tyylisivupolku.
- `HTM`:`string`, komponentin HTML-malli.
- `default`:`function(root)`, komponentin alustustoiminto, vastaanottaa varjojuurikontin.

## tyyliluokka

### `.M`

sisältöalue.

### `.L`

Vasemmassa paneelissa on ohjeet ja dokumentaatio.

### `.R`

Oikea paneeli, joka sisältää demon.

### `c-nav`

Kapea näytön navigointipalkki.