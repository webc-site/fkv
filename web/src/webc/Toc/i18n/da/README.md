# Directory Navigation Component

- **Responsiv skuffe**: Vis sidebjælke i bred skærm, fold automatisk sammen på smal skærm og samarbejde med `c-side` skuffe og titeludløserknap
- **Flersproget driver**: Understøtter objektordbog eller flersprogede funktioner, reagerer automatisk på sprogskift
- **Automatisk undermappe**: Parse automatisk `h1`~`h6` titlerne i målbeholderen for at generere en sekundær mappe og overvåge Hash-visningsportens rullefremhævning.
- **Asynkron indlæsning**: Klik på et mappeelement for automatisk at hente Markdown-tekst asynkront og injicere den i indholdsområdet og vise `Ing` indlæsningsanimation

## Brug demoen

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
  "use": "brug komponent"
});
</script>
```

## Interface parametre

### Egenskaber og metoder

- `toc.prefix`: Hent eller indstil routingpræfiksstrengen (såsom `"doc"`, den efterfølgende skråstreg er valgfri), `README`-nøglen vil automatisk blive knyttet til en tom sti (dvs. selve præfikset)
- `toc.li`: Hent eller indstil mappegenereringsfunktionen for flere sprog `(lang) => ({ key: title })`
- `toc.mdUrl`: Hent eller indstil funktionen Markdown URL-generering `(key, lang) => url`
- `toc.target`: Hent eller indstil den tilknyttede målgengivelsesknude (såsom `c-md`). Hvis det ikke er angivet, vil det automatisk finde `c-md` / `main` / `article`
- `toc.load(key)`: Udløs manuelt dokumentindlæsning og mappeudvidelse svarende til nøglen
- `toc.sync()`: Genpars manuelt titlen i den aktuelle målknude, synkroniser den sekundære mappe og fremhæv status