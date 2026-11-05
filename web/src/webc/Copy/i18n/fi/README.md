# Kopioi teksti leikepöydälle

Napsauta painiketta kopioidaksesi määritetyn elementin tekstin.

- Tukee rel:n käyttöä kopioitavan elementin valitsimen määrittämiseen
- Jos sitä ei ole määritetty, edellinen sisaruselementti kopioidaan oletusarvoisesti.
- Sisäkkäinen Pophover-kehotelaatikko
- Kansainvälinen tuki

## Käytä demoa

```html
<!-- Kopioi tietyt elementit -->
<span id="target">Tässä on kopioitava teksti</span>
<c-copy rel="#target"></c-copy>

<!-- Edelliset elementit kopioidaan oletusarvoisesti -->
<span>edellisen elementin teksti</span>
<c-copy></c-copy>
```

## parametri

| omaisuutta | havainnollistaa |
| --- | --- |
| rel | Kopioi kohdeelementin valitsin. Valinnainen |

## statustyyli

- `c-pophover.Done`: kehotelaatikon tyyli onnistuneen kopioinnin jälkeen