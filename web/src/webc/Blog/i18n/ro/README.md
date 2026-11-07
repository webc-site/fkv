# Componenta de postare pe blog

- **Gruparea automată a anilor**: clasificați automat anii în funcție de data articolului și aranjați-i în ordine inversă în funcție de an
- **Format de dată simplificat**: eliminați automat prefixul anului la afișare și afișați numai luna și ziua
- **Rezumat și text de markdown**: previzualizarea rezumatului listei încorporate și redarea textului complet al articolului unic
- **Titlu tridimensional degradat**: integrați `H1Txt` pentru a afișa titlul și data lansării

## Utilizați demonstrația

```html
<c-blog></c-blog>

<script type="module">
import "webc.site/Blog.js";

const blog = document.querySelector("c-blog");
blog.prefix = "/blog/";
blog.mdUrl = (key, lang) => "/doc/" + lang + "/blog/" + key + ".md";
blog._ = {
  „2026/public”: [„titlu”, „rezumat”, „2026-07-27”]
};

// Comutați pentru a încărca corpul articolului specificat (treceți un șir gol pentru a reveni la listă)
blog.load("2026/publish");
</script>
```

## Parametrii interfeței

### Proprietăți și metode

- `blog.prefix`: obțineți sau setați prefixul linkului articolului (cum ar fi `"/blog/"`)
- `blog.mdUrl`: obțineți sau setați funcția de generare a adresei articolului Markdown `(key, lang) => url`
- `blog._`: obțineți sau setați dicționarul de metadate a listei de articole `{ [key]: [title, summary, date] }`
- `blog.load(key)`: comutați pentru a încărca conținutul articolului din cheia specificată, treceți un șir gol `""` pentru a reveni la lista de bloguri