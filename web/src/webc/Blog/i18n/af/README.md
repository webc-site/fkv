# Blogplasing-komponent

- **Outomatiese jaargroepering**: Klassifiseer jare outomaties volgens artikeldatum, en rangskik hulle in omgekeerde volgorde volgens jaar
- **Vereenvoudigde datumformaat**: Verwyder outomaties die jaarvoorvoegsel wanneer dit vertoon word, en vertoon slegs die maand en dag
- **Markdown-opsomming en teks**: Ingeboude lysopsommingvoorskou en volledige teksweergawe van enkele artikels
- **Gradient driedimensionele titel**: Integreer `H1Txt` om die opskrif en vrystellingsdatum te vertoon

## Gebruik die demo

```html
<c-blog></c-blog>

<script type="module">
import "webc.site/Blog.js";

const blog = document.querySelector("c-blog");
blog.prefix = "/blog/";
blog.mdUrl = (key, lang) => "/doc/" + lang + "/blog/" + key + ".md";
blog._ = {
  "2026/publiseer": ["titel", "abstract", "2026-07-27"]
};

// Skakel oor om die gespesifiseerde artikelliggaam te laai (stuur 'n leë string deur om terug te skakel na die lys)
blog.load("2026/publish");
</script>
```

## Interface parameters

### Eienskappe en metodes

- `blog.prefix`: Kry of stel die artikelskakelvoorvoegsel (soos `"/blog/"`)
- `blog.mdUrl`: Kry of stel die Markdown-artikeladres genereerfunksie `(key, lang) => url`
- `blog._`: Kry of stel die artikellys-metadatawoordeboek `{ [key]: [title, summary, date] }` in
- `blog.load(key)`: Skakel oor om die artikelinhoud van die gespesifiseerde sleutel te laai, gee 'n leë string `""` deur om terug te skakel na die bloglys