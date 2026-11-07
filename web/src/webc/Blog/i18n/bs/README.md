# Komponenta objave na blogu

- **Automatsko grupisanje godina**: Automatski klasificirajte godine prema datumu članka i rasporedite ih obrnutim redoslijedom po godini
- **Pojednostavljeni format datuma**: Automatski uklonite prefiks godine prilikom prikaza i prikažite samo mjesec i dan
- **Sažetak i tekst za označavanje**: Ugrađeni pregled sažetka liste i kompletno prikazivanje teksta jednog članka
- **Gradijentni trodimenzionalni naslov**: Integrirajte `H1Txt` za prikaz naslova i datuma objavljivanja

## Koristite demo

```html
<c-blog></c-blog>

<script type="module">
import "webc.site/Blog.js";

const blog = document.querySelector("c-blog");
blog.prefix = "/blog/";
blog.mdUrl = (key, lang) => "/doc/" + lang + "/blog/" + key + ".md";
blog._ = {
  "2026/objavi": ["naslov", "sažetak", "2026-07-27"]
};

// Prebacite se za učitavanje navedenog tijela članka (proslijedite prazan niz za povratak na listu)
blog.load("2026/publish");
</script>
```

## Parametri interfejsa

### Svojstva i metode

- `blog.prefix`: Nabavite ili postavite prefiks veze na članak (kao što je `"/blog/"`)
- `blog.mdUrl`: Preuzmite ili postavite funkciju generiranja adrese članka Markdown `(key, lang) => url`
- `blog._`: Preuzmite ili postavite rječnik metapodataka liste članaka `{ [key]: [title, summary, date] }`
- `blog.load(key)`: Prebacite se za učitavanje sadržaja članka navedenog ključa, proslijedite prazan niz `""` za povratak na listu blogova