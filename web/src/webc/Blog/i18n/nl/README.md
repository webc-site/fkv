# Blogpostcomponent

- **Automatische jaargroepering**: Classificeer jaren automatisch op basis van artikeldatum en rangschik ze in omgekeerde volgorde op jaar
- **Vereenvoudigd datumformaat**: verwijder automatisch het jaarvoorvoegsel bij weergave en geef alleen de maand en dag weer
- **Markdown-samenvatting en tekst**: ingebouwd overzichtsvoorbeeld van de lijst en volledige tekstweergave van één artikel
- **Driedimensionale titel met verloop**: Integreer `H1Txt` om de kop en releasedatum weer te geven

## Gebruik de demo

```html
<c-blog></c-blog>

<script type="module">
import "webc.site/Blog.js";

const blog = document.querySelector("c-blog");
blog.prefix = "/blog/";
blog.mdUrl = (key, lang) => "/doc/" + lang + "/blog/" + key + ".md";
blog._ = {
  "2026/publiceren": ["titel", "abstract", "27-07-2026"]
};

// Schakel over om de opgegeven artikeltekst te laden (geef een lege string door om terug te schakelen naar de lijst)
blog.load("2026/publish");
</script>
```

## Interfaceparameters

### Eigenschappen en methoden

- `blog.prefix`: haal het voorvoegsel van de artikellink op of stel het in (zoals `"/blog/"`)
- `blog.mdUrl`: Haal of stel de functie voor het genereren van Markdown-artikeladressen `(key, lang) => url` in
- `blog._`: Haal of stel het metadatawoordenboek van de artikellijst `{ [key]: [title, summary, date] }` in
- `blog.load(key)`: Schakel over om de artikelinhoud van de opgegeven sleutel te laden, geef een lege tekenreeks `""` door om terug te schakelen naar de bloglijst