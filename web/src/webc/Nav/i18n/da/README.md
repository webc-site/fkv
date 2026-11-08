# Øverste navigationslinje, der automatisk sammenklappes eller udvides, når siden ruller

- **Auto Skjul**: Skjul automatisk, når du ruller ned på siden.
- **Auto-udvid**: Vises automatisk, når siden rulles op, eller rullepanelet forsvinder.
- **Ydeevneoptimering**: Brug `requestAnimationFrame` til at begrænse rulning.
- **Udgivelsesressourcer**: Begivenhedslyttere og observatører fjernes automatisk, når komponenten ødelægges.

## Brug demoen

```html
<main>
  <c-vs>
    <nav class="Nav">
      <c-nav>
        <a class="logo" href="#">LOGO</a>
      </c-nav>
      <c-nav>
        <a class="item" href="#">Forbindelse</a>
      </c-nav>
    </nav>
    <div class="content">Stryg ned for at skjule navigationen, stryg op for at vise den</div>
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

## Brugsanvisning

Skal bruges som et underordnet eller efterkommerelement af `<c-vs>`.

## stilstatus

Komponenten skifter følgende klassenavn til `nav.Nav`:

- `D`: Vis status (skub ned eller øverst).
- `I`: Overgangstilstand skjules.
- `H`: Skjult status.

## stilvariabler

- `--top`: Opdateres automatisk af komponenten til en negativ værdi af højden, brugt til positionering og overgang.