# Blogindlægskomponent

- **Automatisk årsgruppering**: Klassificer automatisk år i henhold til artikeldato, og arrangere dem i omvendt rækkefølge efter år
- **Forenklet datoformat**: Fjern automatisk årspræfikset ved visning, og vis kun måneden og dagen
- **Markdown-resumé og tekst**: Indbygget listeopsummeringsforhåndsvisning og komplet tekstgengivelse af en enkelt artikel
- **Gradient tredimensionel titel**: Integrer `H1Txt` for at vise overskriften og udgivelsesdatoen

## Brug demoen

```html
<c-blog></c-blog>

<script type="module">
import "webc.site/Blog.js";

const blog = document.querySelector("c-blog");
blog.prefix = "/blog/";
blog.mdUrl = (key, lang) => "/doc/" + lang + "/blog/" + key + ".md";
blog._ = {
  "2026/publicer": ["titel", "abstrakt", "2026-07-27"]
};

// Skift for at indlæse den angivne artikeltekst (giv en tom streng for at skifte tilbage til listen)
blog.load("2026/publish");
</script>
```

## Interface parametre

### Egenskaber og metoder

- `blog.prefix`: Hent eller indstil artiklens linkpræfiks (såsom `"/blog/"`)
- `blog.mdUrl`: Hent eller indstil Markdown-artikeladressegenereringsfunktionen `(key, lang) => url`
- `blog._`: Hent eller indstil artikellistens metadataordbog `{ [key]: [title, summary, date] }`
- `blog.load(key)`: Skift for at indlæse artikelindholdet for den angivne nøgle, send en tom streng `""` for at skifte tilbage til bloglisten