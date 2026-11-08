# Blogiviestikomponentti

- **Automaattinen vuosiryhmittely**: Luokittele vuodet automaattisesti artikkelipäivän mukaan ja järjestä ne käänteiseen järjestykseen vuoden mukaan
- **Yksinkertaistettu päivämäärämuoto**: Poista automaattisesti vuoden etuliite näytössä ja näytä vain kuukausi ja päivä
- **Merkintäyhteenveto ja teksti**: Sisäänrakennettu luettelon yhteenvedon esikatselu ja yhden artikkelin koko tekstin renderöinti
- **Gradient-kolmiulotteinen otsikko**: Integroi `H1Txt` näyttääksesi otsikon ja julkaisupäivän

## Käytä demoa

```html
<c-blog></c-blog>

<script type="module">
import "webc.site/Blog.js";

const blog = document.querySelector("c-blog");
blog.prefix = "/blog/";
blog.mdUrl = (key, lang) => "/doc/" + lang + "/blog/" + key + ".md";
blog._ = {
  "2026/julkaisu": ["nimi", "tiivistelmä", "2026-07-27"]
};

// Vaihda, jos haluat ladata määritetyn artikkelin rungon (välitä tyhjä merkkijono vaihtaaksesi takaisin luetteloon)
blog.load("2026/publish");
</script>
```

## Käyttöliittymän parametrit

### Ominaisuudet ja menetelmät

- `blog.prefix`: Hanki tai aseta artikkelilinkin etuliite (kuten `"/blog/"`)
- `blog.mdUrl`: Hanki tai aseta Markdown-artikkelin osoitteen luontitoiminto `(key, lang) => url`
- `blog._`: Hae tai aseta artikkeliluettelon metatietosanakirja `{ [key]: [title, summary, date] }`
- `blog.load(key)`: Vaihda, jos haluat ladata määritetyn avaimen artikkelin sisällön, välitä tyhjä merkkijono `""` vaihtaaksesi takaisin blogiluetteloon