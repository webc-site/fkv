# Zgornja navigacijska vrstica, ki se samodejno strne ali razširi, ko se stran premika

- **Samodejno skrivanje**: Samodejno skrije, ko se pomikate navzdol po strani.
- **Samodejno razširi**: Samodejno se prikaže, ko se stran pomakne navzgor ali izgine drsni trak.
- **Optimizacija zmogljivosti**: Uporabite `requestAnimationFrame` za zmanjšanje drsenja.
- **Viri za izdajo**: poslušalci in opazovalci dogodkov se samodejno odstranijo, ko je komponenta uničena.

## Uporabite demo

```html
<main>
  <c-vs>
    <nav class="Nav">
      <c-nav>
        <a class="logo" href="#">LOGO</a>
      </c-nav>
      <c-nav>
        <a class="item" href="#">Povezava</a>
      </c-nav>
    </nav>
    <div class="content">Povlecite navzdol, da skrijete navigacijo, povlecite navzgor, da jo prikažete</div>
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

## Navodila za uporabo

Uporabiti ga je treba kot podrejenega ali podrejenega elementa `<c-vs>`.

## stilski status

Komponenta preklopi naslednje ime razreda na `nav.Nav`:

- `D`: Prikaži stanje (drs navzdol ali na vrhu).
- `I`: Prehodno stanje je skrito.
- `H`: Skrito stanje.

## stilske spremenljivke

- `--top`: samodejno posodobi komponenta na negativno vrednost višine, ki se uporablja za pozicioniranje in prehod.