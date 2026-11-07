# Blogginnleggskomponent

- **Automatisk årsgruppering**: Klassifiser automatisk år i henhold til artikkeldato, og ordne dem i omvendt rekkefølge etter år
- **Forenklet datoformat**: Fjern automatisk årsprefikset ved visning, og vis kun måneden og dagen
- **Markdown-sammendrag og tekst**: Innebygd forhåndsvisning av listesammendrag og fullstendig tekstgjengivelse av enkeltartikkel
- **Gradient tredimensjonal tittel**: Integrer `H1Txt` for å vise overskriften og utgivelsesdatoen

## Bruk demoen

```html
<c-blog></c-blog>

<script type="module">
import "webc.site/Blog.js";

const blog = document.querySelector("c-blog");
blog.prefix = "/blog/";
blog.mdUrl = (key, lang) => "/doc/" + lang + "/blog/" + key + ".md";
blog._ = {
  "2026/publish": ["tittel", "abstrakt", "2026-07-27"]
};

// Bytt for å laste den angitte artikkelteksten (passer en tom streng for å bytte tilbake til listen)
blog.load("2026/publish");
</script>
```

## Grensesnittparametere

### Egenskaper og metoder

- `blog.prefix`: Få eller angi artikkelkoblingsprefikset (for eksempel `"/blog/"`)
- `blog.mdUrl`: Få eller still inn Markdown-artikkeladressegenereringsfunksjonen `(key, lang) => url`
- `blog._`: Få eller angi artikkellistens metadataordbok `{ [key]: [title, summary, date] }`
- `blog.load(key)`: Bytt for å laste artikkelinnholdet til den angitte nøkkelen, send en tom streng `""` for å bytte tilbake til blogglisten