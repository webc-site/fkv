# Komponenta navigacije po imeniku

- **Odziven predal**: prikaži stransko vrstico na širokem zaslonu, samodejno strni na ozkem zaslonu in sodeluj s predalom `c-side` in gumbom za sprožitev naslova
- **Večjezični gonilnik**: Podpira objektni slovar ali večjezične funkcije, samodejno se odzove na preklop jezika
- **Samodejni podimenik**: Samodejno razčleni naslove `h1`~`h6` v ciljnem vsebniku, da ustvari sekundarni imenik in nadzira osvetlitev drsečega vidnega polja Hash.
- **Asinhrono nalaganje**: Kliknite na element imenika, da samodejno pridobite asinhrono besedilo Markdown in ga vstavite v področje vsebine, pri čemer se prikaže `Ing` animacija nalaganja

## Uporabite demo

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
  "use": "uporabi komponento"
});
</script>
```

## Parametri vmesnika

### Lastnosti in metode

- `toc.prefix`: pridobite ali nastavite niz predpone usmerjanja (kot je `"doc"`, končna poševnica ni obvezna), ključ `README` bo samodejno preslikan v prazno pot (tj. sama predpona)
- `toc.li`: Pridobite ali nastavite funkcijo za generiranje večjezičnega imenika `(lang) => ({ key: title })`
- `toc.mdUrl`: Pridobite ali nastavite funkcijo za ustvarjanje URL-ja Markdown `(key, lang) => url`
- `toc.target`: Pridobite ali nastavite povezano ciljno vozlišče upodabljanja (kot je `c-md`). Če ni navedeno, samodejno najde `c-md` / `main` / `article`
- `toc.load(key)`: Ročno sproži nalaganje dokumenta in razširitev imenika, ki ustreza ključu
- `toc.sync()`: Ročno ponovno razčlenite naslov v trenutnem ciljnem vozlišču, sinhronizirajte sekundarni imenik in označite stanje