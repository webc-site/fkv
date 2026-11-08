# Component voor mapnavigatie

- **Responsieve lade**: geef de zijbalk weer in breedbeeld, klapt automatisch in op smal scherm en werkt samen met `c-side` lade en titeltriggerknop
- **Meertalige driver**: Ondersteunt objectwoordenboek of meertalige functies, reageert automatisch op taalwisseling
- **Automatische submap**: Parseer automatisch de `h1`~`h6` titels in de doelcontainer om een ​​secundaire map te genereren en de scrollmarkering van de Hash-viewport te controleren.
- **Asynchroon laden**: klik op een directory-item om Markdown-tekst automatisch asynchroon te verkrijgen en deze in het inhoudsgebied te injecteren, waarbij `Ing` laadanimatie wordt weergegeven

## Gebruik de demo

```html
<c-toc></c-toc>
<c-md></c-md>

<script type="module">
import "webc.site/Toc.js";

const toc = document.querySelector("c-toc");
toc.target = document.querySelector("c-md");
toc.prefix = "doc";
toc.mdUrl = (key, lang) => "/doc/" + lang + "/" + key + ".md";
toc.li = (lang) => ({
  "README": "WebC.site",
  "use": "gebruik component"
});
</script>
```

## Interfaceparameters

### Eigenschappen en methoden

- `toc.prefix`: haal de string voor het routeringsvoorvoegsel op of stel deze in (zoals `"doc"`, de afsluitende slash is optioneel), de `README` sleutel wordt automatisch toegewezen aan een leeg pad (d.w.z. het voorvoegsel zelf)
- `toc.li`: Haal of stel de meertalige generatiefunctie voor directory's `(lang) => ({ key: title })` in
- `toc.mdUrl`: Haal of stel de functie voor het genereren van Markdown-URL's `(key, lang) => url` in
- `toc.target`: haal het bijbehorende doelrenderingknooppunt op of stel het in (zoals `c-md`). Indien niet gespecificeerd, wordt automatisch `c-md` / `main` / `article` gevonden
- `toc.load(key)`: Activeer handmatig het laden van documenten en mapuitbreiding die overeenkomt met de sleutel
- `toc.sync()`: herparseer handmatig de titel in het huidige doelknooppunt, synchroniseer de secundaire map en markeer de status