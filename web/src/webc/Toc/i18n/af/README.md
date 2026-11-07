# Gidsnavigasiekomponent

- **Responsiewe laai**: Vertoon sybalk in wye skerm, vou outomaties in nou skerm in en werk saam met `c-side` laai en titel-snellerknoppie
- **Meertalige drywer**: Ondersteun objekwoordeboek of meertalige funksies, reageer outomaties op taalwisseling
- **Outomatiese subgids**: Ontleed outomaties die `h1`~`h6` titels in die teikenhouer om 'n sekondêre gids te genereer en die Hash-kykpoort-rolhoogtepunt te monitor.
- **Asinchroniese laai**: Klik op 'n gidsitem om Markdown-teks outomaties asinchroon te verkry en dit in die inhoudsarea in te spuit, wat `Ing` laai-animasie vertoon

## Gebruik die demo

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
  "use": "gebruik komponent"
});
</script>
```

## Interface parameters

### Eienskappe en metodes

- `toc.prefix`: Kry of stel die roete-voorvoegselstring (soos `"doc"`, die agterste skuinsstreep is opsioneel), die `README` sleutel sal outomaties na 'n leë pad gekarteer word (d.w.s. die voorvoegsel self)
- `toc.li`: Kry of stel die gids-multitaalgenereringfunksie `(lang) => ({ key: title })`
- `toc.mdUrl`: Kry of stel die Markdown URL-generasiefunksie `(key, lang) => url`
- `toc.target`: Kry of stel die geassosieerde teikenweergawe nodus (soos `c-md`). Indien nie gespesifiseer nie, sal dit outomaties `c-md` / `main` / `article` vind
- `toc.load(key)`: Sneller handmatig dokumentlaai en gidsuitbreiding wat ooreenstem met sleutel
- `toc.sync()`: Herontleed die titel handmatig in die huidige teikennodus, sinchroniseer die sekondêre gids en merk status