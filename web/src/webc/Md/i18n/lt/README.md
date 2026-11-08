# Saugiai išanalizuoti ir pateikti Markdown tekstą į HTML.

- **Saugumo valymas**: apsisaugokite nuo XSS atakų naudodami savąją `setHTML` API arba `DOMParser` valiklius
- **Turinio ištraukimas**: inicijavimo metu automatiškai perskaitykite numatytojo lizdo tekstinį turinį ir atvaizduokite jį
- **Įtaisytas rinkimas**: iš anksto nustatytas sąrašas, lentelė, kodų blokas, citatų ir įspėjimo laukelių stiliai

## Naudokite demonstracinę versiją

### statinis atvaizdavimas

```html
<c-md>
# titulą
Tai yra Markdown turinio dalis
</c-md>
```

### dinamiška užduotis

```html
<c-md id="md"></c-md>

<script type="module">
import "webc.site/Md.js";
document.querySelector("#md").value = "#Naujas pavadinimas\nNaujas turinys";
</script>
```

## Sąsajos parametrai

Įveskite žymėjimo tekstą tiesiai žymoje (kaip numatytąją sritį) arba dinamiškai atnaujinkite naudodami `value`.

## Stiliaus aprašymas

Stiliaus klasė `.Md` pridedama automatiškai, kai komponentas inicijuojamas. Palaikomas šis Markdown išplėstinio įspėjimo laukelio išdėstymas (suaktyvinamas pagal nuorodos sintaksę):

- `[!NOTE]`: Greita informacija
- `[!TIP]`: pasiūlymai / patarimai
- `[!IMPORTANT]`: Svarbus dėmesys
- `[!WARNING]`: įspėjimas apie pavojų
- `[!CAUTION]`: įspėjimas apie itin didelę riziką

## saugumo mechanizmas

Pateikdami pirmiausia iškvieskite savąją `setHTML` API, kad saugiai įterptumėte HTML. Jei naršyklė jos nepalaiko, ji automatiškai pereis į rekursinį valytuvą, pagrįstą `DOMParser`, ir įdiegs šią saugos politiką:
- Pašalinkite nesaugias žymas (pvz., `script`, `iframe` ir kt.)
- Išvalyti `on*` įvykio atributus
- Apsaugokite nuo XSS atakų filtruodami atributus, pradedant pavojingais protokolais, tokiais kaip `javascript:`, `vbscript:` ir `data:` (išskyrus `data:image/`).