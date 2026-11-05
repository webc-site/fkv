# Komponent navigácie v adresári

- **Responzívny vysúvací panel**: Zobrazte bočný panel na širokouhlej obrazovke, automaticky sa zbalí na úzkej obrazovke a spolupracuje s `c-side` tlačidlom na spustenie zásuvky a nadpisu
- **Viacjazyčný ovládač**: Podporuje objektový slovník alebo viacjazyčné funkcie, automaticky reaguje na zmenu jazyka
- **Automatický podadresár**: Automaticky analyzuje `h1`~`h6` nadpisy v cieľovom kontajneri, aby sa vygeneroval sekundárny adresár a sledovalo sa zvýraznenie posúvania výrezu hash.
- **Asynchrónne načítavanie**: Kliknutím na položku adresára automaticky získate asynchrónne text Markdown a vložíte ho do oblasti obsahu, pričom sa zobrazí animácia načítania `Ing`

## Použite demo

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
  "use": "použitie komponentu"
});
</script>
```

## Parametre rozhrania

### Vlastnosti a metódy

- `toc.prefix`: Získajte alebo nastavte reťazec predpony smerovania (napríklad `"doc"`, koncová lomka je voliteľná), kľúč `README` sa automaticky namapuje na prázdnu cestu (t. j. na samotnú predponu)
- `toc.li`: Získajte alebo nastavte funkciu viacjazyčného generovania adresára `(lang) => ({ key: title })`
- `toc.mdUrl`: Získajte alebo nastavte funkciu generovania adresy URL Markdown `(key, lang) => url`
- `toc.target`: Získajte alebo nastavte priradený cieľový vykresľovací uzol (napríklad `c-md`). Ak nie je zadaný, automaticky nájde `c-md` / `main` / `article`
- `toc.load(key)`: Manuálne spustiť načítanie dokumentu a rozšírenie adresára zodpovedajúce kľúču
- `toc.sync()`: Manuálne znova analyzujte názov v aktuálnom cieľovom uzle, synchronizujte sekundárny adresár a zvýraznite stav