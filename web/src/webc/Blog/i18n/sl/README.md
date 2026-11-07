# Komponenta objave v blogu

- **Samodejno razvrščanje let v skupine**: Samodejno razvrstite leta glede na datum članka in jih razporedite v obratnem vrstnem redu glede na leto
- **Poenostavljena oblika datuma**: samodejno odstrani predpono leta pri prikazu in prikaže samo mesec in dan
- **Povzetek in besedilo oznake**: vgrajen predogled povzetka seznama in upodabljanje celotnega besedila posameznega članka
- **Gradientni tridimenzionalni naslov**: Integrirajte `H1Txt` za prikaz naslova in datuma objave

## Uporabite demo

```html
<c-blog></c-blog>

<script type="module">
import "webc.site/Blog.js";

const blog = document.querySelector("c-blog");
blog.prefix = "/blog/";
blog.mdUrl = (key, lang) => "/doc/" + lang + "/blog/" + key + ".md";
blog._ = {
  "2026/objava": ["naslov", "povzetek", "27.7.2026"]
};

// Preklopite za nalaganje podanega telesa članka (posredujte prazen niz za preklop nazaj na seznam)
blog.load("2026/publish");
</script>
```

## Parametri vmesnika

### Lastnosti in metode

- `blog.prefix`: Pridobite ali nastavite predpono povezave do članka (kot je `"/blog/"`)
- `blog.mdUrl`: Pridobite ali nastavite funkcijo generiranja naslova članka Markdown `(key, lang) => url`
- `blog._`: Pridobite ali nastavite slovar metapodatkov seznama člankov `{ [key]: [title, summary, date] }`
- `blog.load(key)`: Preklopite za nalaganje vsebine članka navedenega ključa, posredujte prazen niz `""` za preklop nazaj na seznam blogov