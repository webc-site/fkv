# Kataloogide navigeerimise komponent

- **Reagatiivne sahtel**: kuvage külgriba laiekraanil, ahendage kitsal ekraanil automaatselt ja toimige sahtli ja pealkirja päästiku nupuga `c-side`
- **Mitme keele draiver**: toetab objektisõnastiku või mitmekeelseid funktsioone, reageerib automaatselt keelevahetusele
- **Automaatne alamkataloog**: parsige automaatselt sihtkonteineris olevad `h1`~`h6` pealkirjad, et luua teisese kataloog ja jälgida räsivaateava kerimise esiletõstmist.
- **Asünkroonne laadimine**: klõpsake kataloogiüksusel, et hankida automaatselt asünkroonselt Markdowni tekst ja sisestada see sisualale, kuvades `Ing` laadimisanimatsiooni

## Kasutage demo

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
  "use": "kasuta komponenti"
});
</script>
```

## Liidese parameetrid

### Omadused ja meetodid

- `toc.prefix`: hankige või määrake marsruutimise eesliite string (nt `"doc"`, lõpus olev kaldkriips on valikuline), võti `README` vastendatakse automaatselt tühjale teele (st prefiksile endale)
- `toc.li`: hankige või määrake kataloogi mitmekeelse genereerimise funktsioon `(lang) => ({ key: title })`
- `toc.mdUrl`: hankige või määrake Markdowni URL-i genereerimise funktsioon `(key, lang) => url`
- `toc.target`: hankige või määrake seotud sihtrenderdussõlm (nt `c-md`). Kui pole määratud, leiab see automaatselt `c-md` / `main` / `article`
- `toc.load(key)`: käivitage käsitsi dokumendi laadimine ja kataloogi laiendamine vastavalt võtmele
- `toc.sync()`: parsige pealkirja käsitsi ümber praeguses sihtsõlmes, sünkroonige teisese kataloogi ja tõstke olek esile