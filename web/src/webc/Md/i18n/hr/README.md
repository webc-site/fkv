# Sigurno raščlanite i renderirajte Markdown tekst u HTML.

- **Sigurnosna sanacija**: Obranite se od XSS napada koristeći izvorni `setHTML` API ili `DOMParser` čistače
- **Izdvajanje sadržaja**: Automatski čita tekstualni sadržaj zadanog utora tijekom inicijalizacije i renderira ga
- **Ugrađeno postavljanje**: unaprijed postavljeni popis, tablica, blok koda, citati i stilovi okvira upozorenja

## Koristite demo

### statičko prikazivanje

```html
<c-md>
# titula
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

## Parametri sučelja

Proslijedite Markdown tekst izravno unutar oznake (kao zadani utor) ili ažurirajte dinamički putem `value`.

## Opis stila

Klasa stila `.Md` automatski se dodaje kada se komponenta inicijalizira. Podržava sljedeći izgled proširenog okvira upozorenja Markdown (pokreće sintaksa reference):

- `[!NOTE]`：Brze informacije
- `[!TIP]`: Prijedlozi/Savjeti
- `[!IMPORTANT]`: Važna pozornost
- `[!WARNING]`: Upozorenje o riziku
- `[!CAUTION]`: Upozorenje o iznimno visokom riziku

## sigurnosni mehanizam

Prilikom iscrtavanja, prvo pozovite izvorni `setHTML` API za sigurno umetanje HTML-a. Ako ga preglednik ne podržava, automatski će se vratiti na rekurzivni čistač temeljen na `DOMParser` i implementirati sljedeća sigurnosna pravila:
- Uklonite oznake koje nisu sigurne (npr. `script`, `iframe` itd.)
- Obriši `on*` atribute događaja
- Obranite se od XSS napada filtriranjem atributa počevši od opasnih protokola kao što su `javascript:`, `vbscript:` i `data:` (osim `data:image/`).