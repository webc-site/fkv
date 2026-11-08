# Verzeechnes Navigatioun Komponent

- **Responsiv Tirang**: Säitebar am breede Bildschierm weisen, zerklappt automatesch am schmuele Bildschierm a kooperéiert mam `c-side` Tirang an Titel Ausléiser Knäppchen
- ** Multi-Sprooch Chauffer **: Ënnerstëtzt Objekt Wierderbuch oder Multi-Sprooch Funktiounen, reagéiert automatesch op Sproochewiessel
- **Automatesch Ënnerverzeechnes**: Parséiert automatesch `h1`~`h6` Titelen am Zilbehälter fir e sekundäre Verzeechnes ze generéieren an den Hash Viewport Scrolling Highlight ze iwwerwaachen.
- **Asynchron Luede**: Klickt op e Verzeichniselement fir automatesch Markdown-Text asynchron ze kréien an en an den Inhaltsberäich ze sprëtzen, an `Ing` Animatioun ze weisen

## Benotzt d'Demo

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
  "use": "Komponent benotzen"
});
</script>
```

## Interface Parameteren

### Eegeschaften a Methoden

- `toc.prefix`: Kritt oder setzt de Routing-Präfix-String (wéi `"doc"`, den Ofschnëttschnëtt ass fakultativ), de `README` Schlëssel gëtt automatesch op en eidele Wee gemappt (dh de Präfix selwer)
- `toc.li`: Gitt oder setzt d'Verzeechnes Multi-Sprooch Generatioun Funktioun `(lang) => ({ key: title })`
- `toc.mdUrl`: Kritt oder set d'Markdown URL Generatioun Funktioun `(key, lang) => url`
- `toc.target`: Gitt oder setzt den assoziéierten Zil-Rendering Node (wéi `c-md`). Wann net spezifizéiert, fënnt et automatesch `c-md` / `main` / `article`
- `toc.load(key)`: Manuell Ausléiser vum Dokument Luede an Verzeechnesexpansioun entspriechend Schlëssel
- `toc.sync()`: Manuell nei parséieren den Titel am aktuellen Zilknuet, synchroniséiert de sekundäre Verzeichnis an Highlight Status