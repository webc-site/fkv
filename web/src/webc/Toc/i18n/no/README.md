# Katalognavigasjonskomponent

- **Responsiv skuff**: Vis sidefelt i bred skjerm, fold automatisk sammen på smal skjerm og samarbeid med `c-side` skuff og tittelutløserknapp
- **Flerspråklig driver**: Støtter objektordbok eller flerspråklige funksjoner, reagerer automatisk på språkbytte
- **Automatisk underkatalog**: Parse automatisk `h1`~`h6` titlene i målbeholderen for å generere en sekundær katalog og overvåke Hash-visningsportens rullehøydepunkt.
- **Asynkron lasting**: Klikk på et katalogelement for å automatisk hente Markdown-tekst asynkront og injisere den i innholdsområdet, og vise `Ing` lasteanimasjon

## Bruk demoen

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
  "use": "bruk komponent"
});
</script>
```

## Grensesnittparametere

### Egenskaper og metoder

- `toc.prefix`: Hent eller angi rutingprefiksstrengen (som `"doc"`, skråstreken er valgfri), `README`-nøkkelen blir automatisk tilordnet en tom bane (dvs. selve prefikset)
- `toc.li`: Hent eller angi kataloggenereringsfunksjonen for flere språk `(lang) => ({ key: title })`
- `toc.mdUrl`: Få eller still inn funksjonen Markdown URL generering `(key, lang) => url`
- `toc.target`: Hent eller angi den tilknyttede målgjengivelsesnoden (som `c-md`). Hvis det ikke er spesifisert, vil det automatisk finne `c-md` / `main` / `article`
- `toc.load(key)`: Utløs manuelt dokumentinnlasting og katalogutvidelse tilsvarende nøkkel
- `toc.sync()`: Parse tittelen på nytt manuelt i den gjeldende målnoden, synkroniser den sekundære katalogen og marker status