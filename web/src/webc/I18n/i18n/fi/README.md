# Painikkeet ja ponnahdusikkunat käyttöliittymän kielten vaihtamiseen

- Napsauta painiketta avataksesi kielen valinnan kelluvan kerroksen.
- Korosta automaattisesti valittu kieli.

## Käytä demoa

```html
<c-i18n></c-i18n>

<script type="module">
import "webc.site/I18n.js";
// Käytä seuraavaa JS-koodia kielimuutosten seuraamiseen
import { onLang } from "webc.site/js/i18n.js";

onLang((code, id) => {
  alert(code + ' : ' + id);
});
</script>
```

## Käyttöliittymän parametrit

### Muokattu tunniste `<c-i18n>`

Mukautetut HTML-tunnisteet, jotka on otettu käyttöön JS:ssä ja joita käytetään HTML:ssä.

## Statushallinta

Käytä `src/js/i18n.js` hallinnoidaksesi yleistä kielen tilaa:

- **`LANG_LI`**: Kieliluettelotaulukko muodossa `[[nimi, tunnus], ...]`.
- **`lang()`**: Hanki nykyinen kielitunnus.
- **`langCode()`**: Hanki nykyinen kielikoodaus.
- **`langSet(id)`**: Aseta kielitunnus ja ilmoita tilaajille.
- **`onLang(func)`**: Tilaa kielimuutokset. Jos kieli on asetettu, takaisinsoitto käynnistyy välittömästi. Palauta tilauksen peruutustoiminto.

## tyyliluokka

### `.BtnC.lang`

kuvake-painiketta.

### `.I18n.Lg`

Ponnahdusikkunan pääasettelua käytetään kielivaihtoehtojen käärimiseen.