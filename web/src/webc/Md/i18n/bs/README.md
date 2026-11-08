# Sigurno analizirajte i renderirajte Markdown tekst u HTML.

- **Sigurnosna sanacija**: branite se od XSS napada koristeći izvorne `setHTML` API ili `DOMParser` čistače
- **Izdvajanje sadržaja**: Automatski pročitajte tekstualni sadržaj zadanog slota tokom inicijalizacije i renderirajte ga
- **Ugrađeni tipovi**: unaprijed postavljena lista, tabela, stilovi kodnog bloka, citata i okvira upozorenja

## Koristite demo

### static rendering

```html
<c-md>
# naslov
Ovo je dio Markdown sadržaja
</c-md>
```

### dinamički zadatak

```html
<c-md id="md"></c-md>

<script type="module">
import "webc.site/Md.js";
document.querySelector("#md").value = "#Novi naslov\nNovi sadržaj";
</script>
```

## Parametri interfejsa

Proslijedite Markdown tekst direktno unutar oznake (kao zadani slot) ili ažurirajte dinamički putem `value`.

## Opis stila

Klasa stila `.Md` se automatski dodaje kada se komponenta inicijalizira. Podržava sljedeći izgled proširenog okvira upozorenja Markdown (pokrenut referentnom sintaksom):

- `[!NOTE]`：Primljene informacije
- `[!TIP]`: Prijedlozi/Savjeti
- `[!IMPORTANT]`: Važna pažnja
- `[!WARNING]`: Upozorenje o riziku
- `[!CAUTION]`: Upozorenje o izuzetno visokom riziku

## sigurnosni mehanizam

Prilikom renderiranja prvo pozovite izvorni `setHTML` API da bezbedno ubacite HTML. Ako ga pretraživač ne podržava, automatski će se vratiti na rekurzivni čistač baziran na `DOMParser` i implementirati sljedeću sigurnosnu politiku:
- Uklonite nesigurne oznake (npr. `script`, `iframe`, itd.)
- Obrišite `on*` atribute događaja
- Odbranite se od XSS napada filtriranjem atributa počevši od opasnih protokola kao što su `javascript:`, `vbscript:` i `data:` (osim `data:image/`).