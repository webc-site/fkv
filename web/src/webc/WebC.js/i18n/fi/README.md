# Dokumentaatio ja lähdekoodi komponenttien tai työkalutoimintojen renderöintiin

- Kaksisarakkeinen responsiivinen asettelu, jossa asiakirjat näkyvät vasemmalla ja lähdekoodi näytetään oikealla
- Dokumentaatio ja lähdekoodi tukevat Muokkaa- ja Koodi-pikapainikkeita siirtyäksesi lähdeosoitteeseen.
- Otsikko ja johdanto näkyvät yläreunassa, mikä tukee i18n automaattista käännöstä.

## Käytä demoa

```html
<c-webc-js src="/src/js/dom.js" href="/doc/{code}/js/dom.md"></c-webc-js>
```

## Kiinteistöparametrit

- `src`: lähdekoodin URL-osoite.
- `href`: Asiakirjan merkinnän URL-osoite, tukee `{code}` -paikkamerkin käyttöä mukautumaan nykyiseen kieleen.