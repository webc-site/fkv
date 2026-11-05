# Komponenti i navigimit të drejtorisë

- **Sirtari i përgjegjshëm**: Shfaq shiritin anësor në ekran të gjerë, paloset automatikisht në ekran të ngushtë dhe bashkëpuno me sirtarin `c-side` dhe butonin e aktivizimit të titullit
- **Shofer shumëgjuhësh**: Mbështet fjalorin e objekteve ose funksionet shumëgjuhëshe, i përgjigjet automatikisht ndërrimit të gjuhës
- **Nëndrejtori automatike**: Analizoni automatikisht titujt `h1`~`h6` në kontejnerin e synuar për të gjeneruar një drejtori dytësore dhe për të monitoruar theksimin në lëvizje të portit të shikimit Hash.
- **Ngarkimi asinkron**: Klikoni në një artikull direktorie për të marrë automatikisht tekstin Markdown në mënyrë asinkrone dhe për ta injektuar atë në zonën e përmbajtjes, duke shfaqur `Ing` animacionin e ngarkimit

## Përdorni demonstrimin

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
  "use": "përdor komponentin"
});
</script>
```

## Parametrat e ndërfaqes

### Vetitë dhe metodat

- `toc.prefix`: Merrni ose vendosni vargun e prefiksit të rrugëzimit (si p.sh. `"doc"`, vija e pjerrët është opsionale), tasti `README` do të vendoset automatikisht në një shteg të zbrazët (d.m.th. vetë prefiksin)
- `toc.li`: Merr ose vendos funksionin e gjenerimit shumëgjuhësh të drejtorisë `(lang) => ({ key: title })`
- `toc.mdUrl`: Merrni ose vendosni funksionin e krijimit të URL-së Markdown `(key, lang) => url`
- `toc.target`: Merr ose cakton nyjen përkatëse të interpretimit të objektivit (si p.sh. `c-md`). Nëse nuk specifikohet, do të gjejë automatikisht `c-md` / `main` / `article`
- `toc.load(key)`: Aktivizoni me dorë ngarkimin e dokumentit dhe zgjerimin e drejtorisë që korrespondon me çelësin
- `toc.sync()`: Ripare manualisht titullin në nyjen aktuale të synuar, sinkronizoni drejtorinë dytësore dhe nënvizoni statusin