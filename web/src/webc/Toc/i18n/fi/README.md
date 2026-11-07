# Hakemiston navigointikomponentti

- **Responsiivinen vetolaatikko**: Näytä sivupalkki laajassa näytössä, tiivistyy automaattisesti kapeassa näytössä ja toimii yhteistyössä `c-side` vetolaatikon ja otsikon laukaisupainikkeen kanssa
- **Monikielinen ohjain**: Tukee objektisanakirja- tai monikielitoimintoja, reagoi automaattisesti kielen vaihtoon
- **Automaattinen alihakemisto**: Jäsennä `h1`~`h6` otsikot automaattisesti kohdesäiliössä luodaksesi toissijaisen hakemiston ja valvoaksesi Hash-näkymäportin vierittävää korostusta.
- **Asynkroninen lataus**: Napsauta hakemistokohdetta saadaksesi Markdown-tekstin automaattisesti asynkronisesti ja lisäämään sen sisältöalueelle näyttäen `Ing` latausanimaatiota

## Käytä demoa

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
  "use": "käytä komponenttia"
});
</script>
```

## Käyttöliittymän parametrit

### Ominaisuudet ja menetelmät

- `toc.prefix`: Hanki tai aseta reititysetuliitemerkkijono (kuten `"doc"`, perässä oleva kauttaviiva on valinnainen), avain `README` kartoitetaan automaattisesti tyhjään polkuun (eli itse etuliite).
- `toc.li`: Hanki tai aseta hakemiston monikielinen luontitoiminto `(lang) => ({ key: title })`
- `toc.mdUrl`: Hanki tai aseta Markdown-URL-osoitteen luontitoiminto `(key, lang) => url`
- `toc.target`: Hanki tai aseta siihen liittyvä kohderenderöintisolmu (kuten `c-md`). Jos sitä ei ole määritetty, se löytää automaattisesti `c-md` / `main` / `article`
- `toc.load(key)`: Käynnistä manuaalisesti asiakirjan lataus ja avainta vastaava hakemiston laajennus
- `toc.sync()`: jäsennä otsikko manuaalisesti uudelleen nykyisessä kohdesolmussa, synkronoi toissijainen hakemisto ja korosta tila