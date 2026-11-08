# Navigointivalikko mukautettu eri näyttökokoihin

- Työpöytä: Linkit näytetään vaakasuorissa ruuduissa
- Mobiiliversio: Tiivistä valikkopainikkeeseen, liu'uta sivupalkin vetolaatikko ulos napsauttamalla
- Reitin aktivointi: vertaa automaattisesti nykyistä polkua ja hajautusarvoa ja lisää aktivointitilaluokan nimi vastaavaan linkkiin `A`
- Sulje automaattisesti: Kun mobiilipääte napsauttaa linkkiä tai sivupalkin ulkoista taustaa, sivupalkki sulkeutuu automaattisesti.

## käyttää

```html
<c-nav-menu>
  <a href="/">etusivulle</a>
  <a href="/doc">asiakirja</a>
  <a href="/about">noin</a>
</c-nav-menu>
```

## havainnollistaa

Jos alielementissä ei ole `c-side`:tä, kun komponentti alustetaan, mobiilivalikkopainike ja `c-side` sivupalkki luodaan automaattisesti ja kaikki `a` -tunnisteet siirretään sivupalkkiin.

## tyyliluokka

- **A**: Aktivoitu linkki lisää automaattisesti luokan nimen `A`