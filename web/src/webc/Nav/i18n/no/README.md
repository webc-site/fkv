# Øverste navigasjonslinje som automatisk kollapser eller utvides når siden ruller

- **Automatisk skjul**: Skjul automatisk når du ruller nedover siden.
- **Auto-utvid**: Vises automatisk når siden rulles opp eller rullefeltet forsvinner.
- **Ytelsesoptimalisering**: Bruk `requestAnimationFrame` for å begrense rullingen.
- **Slipp ressurser**: Hendelseslyttere og observatører fjernes automatisk når komponenten blir ødelagt.

## Bruk demoen

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
    <div class="content">Sveip ned for å skjule navigasjonen, sveip opp for å vise den</div>
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

## Bruksanvisning

Må brukes som et underordnet eller etterkommerelement av `<c-vs>`.

## stilstatus

Komponenten bytter følgende klassenavn på `nav.Nav`:

- `D`: Vis status (skyv ned eller øverst).
- `I`: Overgangstilstand skjules.
- `H`: Skjult status.

## stilvariabler

- `--top`: Oppdateres automatisk av komponenten til en negativ verdi av høyde, brukt for posisjonering og overgang.