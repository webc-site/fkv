# Top Navigatiounsbar déi automatesch zesummeklappt oder erweidert wann d'Säit scrollt

- **Auto Verstoppen**: Automatesch verstoppen wann Dir op d'Säit scrollt.
- **Auto-expandéieren**: Automatesch ugewisen wann d'Säit erop scrollt oder d'Scrollbar verschwënnt.
- **Performance Optimisatioun**: Benotzt `requestAnimationFrame` fir d'Scrolling ze drosselen.
- **Release Ressourcen **: Eventnolauschterer an Observateure ginn automatesch geläscht wann de Komponent zerstéiert gëtt.

## Benotzt d'Demo

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
    <div class="content">Swipt erof fir d'Navigatioun ze verstoppen, swipt erop fir se ze weisen</div>
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

## Gebrauchsanweisung

Muss als Kand oder Nokommen Element vun `<c-vs>` benotzt ginn.

## Stil Status

De Komponent wiesselt de folgende Klassennumm op `nav.Nav`:

- `D`: Status weisen (Rutsch erof oder uewen).
- `I`: Iwwergangszoustand verstoppt.
- `H`: Hidden Status.

## Stil Verännerlechen

- `--top`: Automatesch vun der Komponent aktualiséiert op en negativen Héichtwäert, benotzt fir Positionéierung an Iwwergang.