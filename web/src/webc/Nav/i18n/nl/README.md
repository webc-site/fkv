# Bovenste navigatiebalk die automatisch in- of uitklapt wanneer de pagina scrolt

- **Automatisch verbergen**: automatisch verbergen wanneer u naar beneden scrollt op de pagina.
- **Auto-uitvouwen**: automatisch weergegeven wanneer de pagina omhoog wordt gescrolld of de schuifbalk verdwijnt.
- **Prestatieoptimalisatie**: gebruik `requestAnimationFrame` om het scrollen te vertragen.
- **Releasebronnen**: gebeurtenislisteners en waarnemers worden automatisch verwijderd wanneer het onderdeel wordt vernietigd.

## Gebruik de demo

```html
<main>
  <c-vs>
    <nav class="Nav">
      <c-nav>
        <a class="logo" href="#">LOGO</a>
      </c-nav>
      <c-nav>
        <a class="item" href="#">Link</a>
      </c-nav>
    </nav>
    <div class="content">Veeg naar beneden om de navigatie te verbergen, veeg naar boven om deze weer te geven</div>
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

## Instructies voor gebruik

Moet worden gebruikt als onderliggend of afstammend element van `<c-vs>`.

## stijl status

De component schakelt de volgende klassenaam in `nav.Nav`:

- `D`: Status weergeven (schuif naar beneden of naar boven).
- `I`: Overgangsstatus is verborgen.
- `H`: Verborgen status.

## stijlvariabelen

- `--top`: automatisch bijgewerkt door de component naar een negatieve hoogtewaarde, gebruikt voor positionering en overgang.