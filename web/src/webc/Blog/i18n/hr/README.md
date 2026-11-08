# Komponenta posta na blogu

- **Automatsko grupiranje godina**: Automatski klasificirajte godine prema datumu članka i rasporedite ih obrnutim redoslijedom prema godini
- **Pojednostavljeni format datuma**: Automatski uklonite prefiks godine prilikom prikaza i prikažite samo mjesec i dan
- **Markdown sažetak i tekst**: ugrađeni pregled sažetka popisa i renderiranje kompletnog teksta jednog članka
- **Gradijentni trodimenzionalni naslov**: Integrirajte `H1Txt` za prikaz naslova i datuma objave

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

// Prebacivanje za učitavanje navedenog tijela članka (proslijedite prazan niz za povratak na popis)
blog.load("2026/publish");
</script>
```

## Parametri sučelja

### Svojstva i metode

- `blog.prefix`: Dobijte ili postavite prefiks veze članka (kao što je `"/blog/"`)
- `blog.mdUrl`: Preuzmite ili postavite Markdown funkciju generiranja adrese članka `(key, lang) => url`
- `blog._`: Dobijte ili postavite rječnik metapodataka popisa članaka `{ [key]: [title, summary, date] }`
- `blog.load(key)`: Prebacite se za učitavanje sadržaja članka navedenog ključa, proslijedite prazan niz `""` da biste se vratili na popis blogova