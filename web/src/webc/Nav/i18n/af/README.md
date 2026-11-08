# Boonste navigasiebalk wat outomaties invou of uitbrei wanneer die bladsy blaai

- **Versteek outomaties**: Versteek outomaties wanneer u op die bladsy blaai.
- **Outo-uitbreiding**: Word outomaties vertoon wanneer die bladsy opgerol word of die rolbalk verdwyn.
- **Prestasie-optimalisering**: Gebruik `requestAnimationFrame` om blaai te verswak.
- **Vrystelling van hulpbronne**: Gebeurtenisluisteraars en -waarnemers word outomaties verwyder wanneer die komponent vernietig word.

## Gebruik die demo

```html
<main>
  <c-vs>
    <nav class="Nav">
      <c-nav>
        <a class="logo" href="#">LOGO</a>
      </c-nav>
      <c-nav>
        <a class="item" href="#">Skakel</a>
      </c-nav>
    </nav>
    <div class="content">Swiep af om die navigasie te versteek, swiep op om dit te wys</div>
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

## Instruksies vir gebruik

Moet gebruik word as 'n kind of afstammeling element van `<c-vs>`.

## styl status

Die komponent skakel die volgende klasnaam op `nav.Nav`:

- `D`: Wys status (gly af of bo).
- `I`: Oorgangstoestand word versteek.
- `H`: Versteekte status.

## styl veranderlikes

- `--top`: Outomaties opgedateer deur die komponent na 'n negatiewe waarde van hoogte, gebruik vir posisionering en oorgang.