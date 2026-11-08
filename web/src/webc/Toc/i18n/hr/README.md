# Komponenta navigacije imenika

- **Responzivna ladica**: Prikažite bočnu traku na širokom zaslonu, automatski sažmite na uskom zaslonu i surađujte s `c-side` ladicom i gumbom za pokretanje naslova
- **Višejezični upravljački program**: Podržava rječnik objekta ili višejezične funkcije, automatski reagira na promjenu jezika
- **Automatski poddirektorij**: Automatski raščlanite `h1`~`h6` naslove u ciljnom spremniku kako biste generirali sekundarni direktorij i nadzirali pomicanje istaknutog polja prikaza Hash.
- **Asinkrono učitavanje**: Kliknite na stavku direktorija za automatsko asinkrono dobivanje Markdown teksta i umetanje u područje sadržaja, prikazujući `Ing` animaciju učitavanja

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
  "use": "koristi komponentu"
});
</script>
```

## Parametri sučelja

### Svojstva i metode

- `toc.prefix`: Uzmite ili postavite niz prefiksa usmjeravanja (kao što je `"doc"`, kosa crta na kraju nije obavezna), ključ `README` automatski će se mapirati na praznu stazu (tj. sam prefiks)
- `toc.li`: Nabavite ili postavite funkciju generiranja više jezika `(lang) => ({ key: title })`
- `toc.mdUrl`: Dobijte ili postavite funkciju generiranja URL-a Markdown `(key, lang) => url`
- `toc.target`: Dobijte ili postavite pridruženi ciljni čvor prikazivanja (kao što je `c-md`). Ako nije navedeno, automatski će pronaći `c-md` / `main` / `article`
- `toc.load(key)`: Ručno pokrenite učitavanje dokumenta i proširenje direktorija koji odgovara ključu
- `toc.sync()`: Ručno ponovno raščlanite naslov u trenutnom ciljnom čvoru, sinkronizirajte sekundarni direktorij i označite status