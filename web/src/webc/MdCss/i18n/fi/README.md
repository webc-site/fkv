# Lukukokemukseen optimoidut typografiset tyylit

- **Perusladonta**: mukauta otsikoita, kappaleita, lihavointia, kursivointia, yliviivausta, tekstin sisäistä koodia, kuvia ja jakoviivoja.
- **Linkin animaatio**: Näytä alleviivausanimaatio, kun viet hiiren osoitinta.
- **Projektiluettelo**: Järjestämättömät luettelot käyttävät luettelomerkkejä, järjestetyissä luetteloissa pyöristettyjä taustasarjanumeroita ja ne tukevat monitasoista sisäkkäisyyttä.
- **Tehtävälista**: Tarjoaa valintaruudun tyylin, tukee tehtäviä ja suoritettua tilaa.
- **Varoitusvinkit**: Tukee viittä erityyppistä varoitusruutua: Huomautus, Vihje, Tärkeä, Varoitus ja Huomio.
- **Taulukot ja koodi**: Tarjoaa taulukon rivien korostuksen ja koodilohkot varjoilla ja liukuvärillä.

## Käytä demoa

```html
<div class="Md">
  <h1>otsikko</h1>
  <p>Kappaleen teksti ja <a href="#">linkki</a></p>
  <hr>
  <h2>Toisen tason otsikko</h2>
  <ul><li><input type="checkbox" checked="" disabled=""> Tehtäväkohde</li></ul>
  <blockquote class="q note">
    <b class="qt"><i class="qi"></i>`<p>Huomaa sisältö</p>
  </blockquote>
</div>
```

## tyyliluokka

Säilön `.Md` alla oleviin HTML-elementteihin sovelletaan tyylejä.

### Varoituslaatikon tyyli

Lisää seuraava luokan nimi `blockquote` -elementtiin ja `.q` luokan nimi vaihtaaksesi hälytysruutua:

- `note`: Huomautuksia
- `tip`: Vihje
- `important`: Tärkeää
- `warning`: varoitus
- `caution`: Huomautus