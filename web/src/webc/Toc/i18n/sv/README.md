# Katalognavigeringskomponent

- **Responsiv låda**: Visa sidofältet i bred skärm, komprimera automatiskt på smal skärm och samarbeta med `c-side` låda och titelutlösningsknapp
- **Flerspråkig drivrutin**: Stöder objektordbok eller flerspråkiga funktioner, svarar automatiskt på språkväxling
- **Automatisk underkatalog**: Parsera automatiskt `h1`~`h6`-titlarna i målbehållaren för att generera en sekundär katalog och övervaka rullningsmarkeringen för Hash-vyporten.
- **Asynkron laddning**: Klicka på ett katalogobjekt för att automatiskt hämta Markdown-text asynkront och injicera den i innehållsområdet och visa `Ing` laddande animering

## Använd demon

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
  "use": "använd komponent"
});
</script>
```

## Gränssnittsparametrar

### Egenskaper och metoder

- `toc.prefix`: Hämta eller ställ in routingprefixsträngen (som `"doc"`, det avslutande snedstrecket är valfritt), `README`-nyckeln mappas automatiskt till en tom sökväg (d.v.s. själva prefixet)
- `toc.li`: Hämta eller ställ in kataloggenereringsfunktionen för flera språk `(lang) => ({ key: title })`
- `toc.mdUrl`: Hämta eller ställ in funktionen Markdown URL-generering `(key, lang) => url`
- `toc.target`: Hämta eller ställ in den associerade målrenderingsnoden (som `c-md`). Om det inte anges hittar det automatiskt `c-md` / `main` / `article`
- `toc.load(key)`: Aktivera manuellt dokumentladdning och katalogexpansion motsvarande nyckel
- `toc.sync()`: Omtolka titeln manuellt i den aktuella målnoden, synkronisera den sekundära katalogen och markera status