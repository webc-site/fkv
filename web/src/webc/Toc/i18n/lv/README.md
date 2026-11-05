# Direktoriju navigācijas komponents

- **Atsaucīga atvilktne**: displeja sānjosla platajā ekrānā, automātiski sakļauta šaurā ekrānā un sadarbojas ar `c-side` atvilktnes un virsraksta palaišanas pogu
- **Daudzvalodu draiveris**: atbalsta objektu vārdnīcas vai vairāku valodu funkcijas, automātiski reaģē uz valodas maiņu
- **Automātisks apakšdirektorijs**: automātiski parsējiet `h1`~`h6` virsrakstus mērķa konteinerā, lai ģenerētu sekundāro direktoriju un pārraudzītu Hash skata loga ritināšanas izcēlumu.
- **Asinhronā ielāde**: noklikšķiniet uz direktorija vienuma, lai automātiski asinhroni iegūtu Markdown tekstu un ievadītu to satura apgabalā, parādot `Ing` ielādes animāciju

## Izmantojiet demonstrāciju

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
  "lietot": "izmantot komponentu"
});
</script>
```

## Interfeisa parametri

### Īpašības un metodes

- `toc.prefix`: iegūstiet vai iestatiet maršrutēšanas prefiksa virkni (piemēram, `"doc"`, beigu slīpsvītra nav obligāta), atslēga `README` tiks automātiski kartēta uz tukšu ceļu (t.i., pašu prefiksu)
- `toc.li`: iegūstiet vai iestatiet direktoriju vairāku valodu ģenerēšanas funkciju `(lang) => ({ key: title })`
- `toc.mdUrl`: iegūstiet vai iestatiet Markdown URL ģenerēšanas funkciju `(key, lang) => url`
- `toc.target`: iegūstiet vai iestatiet saistīto mērķa renderēšanas mezglu (piemēram, `c-md`). Ja tas nav norādīts, tas automātiski atradīs `c-md` / `main` / `article`
- `toc.load(key)`: manuāli aktivizējiet dokumenta ielādi un direktorija paplašināšanu atbilstoši atslēgai
- `toc.sync()`: manuāli atkārtoti parsējiet virsrakstu pašreizējā mērķa mezglā, sinhronizējiet sekundāro direktoriju un izcelšanas statusu