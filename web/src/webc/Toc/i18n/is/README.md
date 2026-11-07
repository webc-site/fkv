# Listaleiðsöguhluti

- **Svörun skúffu**: Birta hliðarstiku á breiðum skjá, fella sjálfkrafa saman á þröngum skjá og vinna með `c-side` skúffu og titlavirkjunarhnappi
- **Mörg tungumála ökumaður**: Styður hlutaorðabók eða fjöltungumálaaðgerðir, bregst sjálfkrafa við tungumálaskiptum
- **Sjálfvirk undirskrá**: Þjálfa sjálfkrafa `h1`~`h6` titlana í markgámnum til að búa til aukamöppu og fylgjast með skrunhápunkti Hash útsýnisins.
- **Ósamstillt hleðsla**: Smelltu á hlut í möppu til að fá sjálfkrafa Markdown texta ósamstillt og sprauta honum inn á innihaldssvæðið, sýna `Ing` hleðslu hreyfimynd

## Notaðu kynninguna

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
  "use": "nota hluti"
});
</script>
```

## Viðmótsbreytur

### Eiginleikar og aðferðir

- `toc.prefix`: Fáðu eða stilltu leiðarforskeytistrenginn (eins og `"doc"`, aftari skástrikið er valfrjálst), `README` lykillinn verður sjálfkrafa varpaður á tóma slóð (þ.e. forskeytið sjálft)
- `toc.li`: Fáðu eða stilltu aðgerðina til að búa til fjölda tungumála fyrir möppu `(lang) => ({ key: title })`
- `toc.mdUrl`: Fáðu eða stilltu Markdown URL myndunaraðgerðina `(key, lang) => url`
- `toc.target`: Fáðu eða stilltu tilheyrandi markbirtingarhnút (eins og `c-md`). Ef það er ekki tilgreint mun það sjálfkrafa finna `c-md` / `main` / `article`
- `toc.load(key)`: Kveiktu handvirkt á hleðslu skjala og skráarstækkun sem samsvarar lyklinum
- `toc.sync()`: Endurgreina titilinn handvirkt í núverandi markhnút, samstilla aukaskrána og auðkenna stöðu