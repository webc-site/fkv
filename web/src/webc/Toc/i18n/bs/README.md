# Komponenta za navigaciju direktorija

- **Reakciona ladica**: Prikaži bočnu traku na širokom ekranu, automatski sažimajte na uskom ekranu i surađujte sa `c-side` fiokom i dugmetom za pokretanje naslova
- **Višejezični upravljački program**: Podržava rječnik objekata ili višejezične funkcije, automatski odgovara na promjenu jezika
- **Automatski poddirektorij**: Automatski raščlanite `h1`~`h6` naslove u ciljnom kontejneru da biste generirali sekundarni direktorij i nadgledali pomicanje Hash polja prikaza.
- **Asinkrono učitavanje**: Kliknite na stavku direktorija da automatski asinkrono dobijete Markdown tekst i ubacite ga u područje sadržaja, prikazujući `Ing` animaciju učitavanja

## Koristite demo

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
  "koristi": "koristi komponentu"
});
</script>
```

## Parametri interfejsa

### Svojstva i metode

- `toc.prefix`: Nabavite ili postavite niz prefiksa za usmjeravanje (kao što je `"doc"`, zadnja kosa crta nije obavezna), `README` ključ će se automatski mapirati na praznu putanju (tj. sam prefiks)
- `toc.li`: Nabavite ili postavite funkciju višejezičnog generiranja direktorija `(lang) => ({ key: title })`
- `toc.mdUrl`: Preuzmite ili postavite funkciju Markdown generiranja URL-a `(key, lang) => url`
- `toc.target`: Nabavite ili postavite povezani ciljni čvor za prikazivanje (kao što je `c-md`). Ako nije navedeno, automatski će pronaći `c-md` / `main` / `article`
- `toc.load(key)`: Ručno pokrenite učitavanje dokumenta i proširenje direktorija koji odgovara ključu
- `toc.sync()`: Ručno ponovo analizirajte naslov u trenutnom ciljnom čvoru, sinkronizirajte sekundarni direktorij i označite status