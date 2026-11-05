# Katalogų naršymo komponentas

- **Reaguojantis stalčius**: Rodyti šoninę juostą plačiame ekrane, automatiškai sutraukti siaurame ekrane ir bendradarbiauti su `c-side` stalčiaus ir pavadinimo paleidimo mygtuku
- **Kelių kalbų tvarkyklė**: palaiko objektų žodyno arba kelių kalbų funkcijas, automatiškai reaguoja į kalbos perjungimą
- **Automatinis pakatalogis**: automatiškai analizuokite `h1`~`h6` pavadinimus tiksliniame konteineryje, kad sukurtumėte antrinį katalogą ir stebėtumėte maišos peržiūros srities slinkimo paryškinimą.
- **Asinchroninis įkėlimas**: spustelėkite katalogo elementą, kad automatiškai asinchroniškai gautumėte žymėjimo tekstą ir įterptumėte jį į turinio sritį, rodant `Ing` įkėlimo animaciją

## Naudokite demonstracinę versiją

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
  "naudoti": "naudoti komponentą"
});
</script>
```

## Sąsajos parametrai

### Savybės ir metodai

- `toc.prefix`: gaukite arba nustatykite maršruto priešdėlio eilutę (pvz., `"doc"`, pasvirasis brūkšnys yra neprivalomas), raktas `README` bus automatiškai susietas su tuščiu keliu (t. y. pačiu priešdėliu)
- `toc.li`: gaukite arba nustatykite katalogo kelių kalbų generavimo funkciją `(lang) => ({ key: title })`
- `toc.mdUrl`: gaukite arba nustatykite žymėjimo URL generavimo funkciją `(key, lang) => url`
- `toc.target`: gaukite arba nustatykite susietą tikslinį atvaizdavimo mazgą (pvz., `c-md`). Jei nenurodyta, ji automatiškai suras `c-md` / `main` / `article`
- `toc.load(key)`: rankiniu būdu suaktyvinkite dokumento įkėlimą ir katalogo išplėtimą pagal raktą
- `toc.sync()`: rankiniu būdu iš naujo išanalizuoti pavadinimą dabartiniame tiksliniame mazge, sinchronizuoti antrinį katalogą ir paryškinti būseną