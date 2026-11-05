# Ylänavigointipalkki, joka kutistuu tai laajenee automaattisesti, kun sivua vieritetään

- **Automaattinen piilotus**: Piilota automaattisesti vierittäessäsi sivua alaspäin.
- **Automaattinen laajennus**: Näkyy automaattisesti, kun sivua vieritetään ylöspäin tai vierityspalkki katoaa.
- **Suorituskyvyn optimointi**: Käytä `requestAnimationFrame`-painiketta vierityksen säätämiseen.
- **Julkaise resurssit**: Tapahtumaseuraajat ja tarkkailijat poistetaan automaattisesti, kun komponentti tuhoutuu.

## Käytä demoa

```html
<main>
  <c-vs>
    <nav class="Nav">
      <c-nav>
        <a class="logo" href="#">LOGO</a>
      </c-nav>
      <c-nav>
        <a class="item" href="#">Linkki</a>
      </c-nav>
    </nav>
    <div class="content">Piilota navigointi pyyhkäisemällä alas ja näytä se pyyhkäisemällä ylös</div>
  </c-vs>
</main>
<style>
  main {
    position: relative;
    height: 300px;
    border: 1px solid #000;
  }
  nav.Nav > c-nav {
    background: #fffc;
    backdrop-filter: blur(4px);
    border-bottom: 1px solid #ccc;
  }
  .content {
    height: 200dvh;
    background: linear-gradient(to bottom, #fff, #ccc);
    padding: 80px 20px 20px;
    color: #666;
    text-align: center;
  }
</style>
```

## Käyttöohjeet

Sitä on käytettävä elementin `<c-vs>` ala- tai jälkeläisenä elementtinä.

## tyylin tila

Komponentti kytkee seuraavan luokan nimen päälle `nav.Nav`:

- `D`: Näytä tila (liuku alas tai ylhäältä).
- `I`: Siirtymätila piilotetaan.
- `H`: Piilotettu tila.

## tyylimuuttujat

- `--top`: Komponentti päivittää automaattisesti negatiiviseen korkeusarvoon, käytetään paikannukseen ja siirtymiseen.