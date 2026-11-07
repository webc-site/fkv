# Jäsennä ja renderöi Markdown-teksti turvallisesti HTML-muotoon.

- **Turvallisuuspuhdistus**: Suojaudu XSS-hyökkäyksiä vastaan ​​käyttämällä alkuperäistä `setHTML` API- tai `DOMParser` puhdistusaineita
- **Sisällön purkaminen**: Lue automaattisesti oletuspaikan tekstisisältö alustuksen aikana ja hahmonna se
- **Sisäänrakennettu ladonta**: esiasetettu luettelo, taulukko, koodilohko, lainaus ja varoitusruututyylit

## Käytä demoa

### staattinen renderöinti

```html
<c-md>
# otsikko
Tämä on osa Markdownin sisältöä
</c-md>
```

### dynaaminen tehtävä

```html
<c-md id="md"></c-md>

<script type="module">
import "webc.site/Md.js";
document.querySelector("#md").value = "#Uusi otsikko\nUusi sisältö";
</script>
```

## Käyttöliittymän parametrit

Lisää Markdown-teksti suoraan tunnisteeseen (oletuspaikkana) tai päivitä dynaamisesti `value`:n kautta.

## Tyylin kuvaus

Tyyliluokka `.Md` lisätään automaattisesti, kun komponentti alustetaan. Tukee seuraavaa Markdown-laajennetun varoitusruudun asettelua (käynnistetään viitesyntaksin avulla):

- `[!NOTE]`: Nopeat tiedot
- `[!TIP]`: Ehdotuksia/vinkkejä
- `[!IMPORTANT]`: Tärkeä huomio
- `[!WARNING]`: Riskivaroitus
- `[!CAUTION]`: Erittäin suuren riskin varoitus

## turvamekanismi

Kun hahmonnat, kutsu ensin natiivi `setHTML` API, jotta voit lisätä HTML-koodin turvallisesti. Jos selain ei tue sitä, se päivittää automaattisesti rekursiiviseen puhdistajaan `DOMParser`:n perusteella ja ottaa käyttöön seuraavan suojauskäytännön:
- Poista ei-turvalliset tunnisteet (esim. `script`, `iframe` jne.)
- Tyhjennä `on*` tapahtumaattribuutit
- Puolusta XSS-hyökkäyksiä vastaan ​​suodattamalla attribuutteja, jotka alkavat vaarallisista protokollista, kuten `javascript:`, `vbscript:` ja `data:` (paitsi `data:image/`).