# Direktorioa nabigatzeko osagaia

- **Erantzun duen tiradera**: bistaratu alboko barra pantaila zabalean, tolestu automatikoki pantaila estuan eta elkarlanean aritu `c-side` tiraderarekin eta izenburuaren abiarazte-botoiarekin
- **Hizkuntza anitzeko kontrolatzailea**: objektu hiztegia edo hizkuntza anitzeko funtzioak onartzen ditu, automatikoki erantzuten dio hizkuntza aldaketei
- **Azpidirektorio automatikoa**: automatikoki analizatu `h1`~`h6` izenburuak xede-edukiontzian, bigarren direktorioa sortzeko eta Hash ikuspegiaren korritze nabarmena kontrolatzeko.
- **Karga asinkronoa**: egin klik direktorioko elementu batean Markdown testua modu asinkronoan automatikoki lortzeko eta edukiaren eremuan sartzeko, `Ing` kargatzeko animazioa bistaratuz

## Erabili demoa

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
  "use": "erabili osagaia"
});
</script>
```

## Interfazearen parametroak

### Propietateak eta metodoak

- `toc.prefix`: Lortu edo ezarri bideratze-aurrizki-katea (adibidez, `"doc"`, amaierako barra hautazkoa da), `README` tekla automatikoki bide huts batera mapatuko da (hau da, aurrizkia bera)
- `toc.li`: Lortu edo ezarri direktorioa hizkuntza anitzeko funtzioa sortzeko `(lang) => ({ key: title })`
- `toc.mdUrl`: Lortu edo ezarri Markdown URLak sortzeko funtzioa `(key, lang) => url`
- `toc.target`: Lortu edo ezarri lotutako xede-errendatze-nodoa (adibidez, `c-md`). Zehazten ez bada, automatikoki aurkituko du `c-md` / `main` / `article`
- `toc.load(key)`: eskuz abiarazi dokumentuaren karga eta gakoari dagokion direktorio-hedapena
- `toc.sync()`: eskuz berriro aztertu izenburua uneko xede-nodoan, sinkronizatu bigarren mailako direktorioa eta nabarmendu egoera