# Obere Navigationsleiste, die beim Scrollen der Seite automatisch minimiert oder erweitert wird

- **Automatisch ausblenden**: Wird automatisch ausgeblendet, wenn auf der Seite nach unten gescrollt wird.
- **Automatisch erweitern**: Wird automatisch angezeigt, wenn die Seite nach oben gescrollt wird oder die Bildlaufleiste verschwindet.
- **Leistungsoptimierung**: Verwenden Sie `requestAnimationFrame`, um das Scrollen zu drosseln.
- **Ressourcen freigeben**: Ereignis-Listener und -Beobachter werden automatisch entfernt, wenn die Komponente zerstört wird.

## Nutzen Sie die Demo

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
    <div class="content">Wischen Sie nach unten, um die Navigation auszublenden, und nach oben, um sie anzuzeigen</div>
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

Muss als untergeordnetes oder abgeleitetes Element von `<c-vs>` verwendet werden.

## Stilstatus

Die Komponente schaltet den folgenden Klassennamen auf `nav.Nav`:

- `D`: Status anzeigen (nach unten oder oben schieben).
- `I`: Übergangsstatus wird ausgeblendet.
- `H`: Ausgeblendeter Status.

## Stilvariablen

- `--top`: Wird von der Komponente automatisch auf einen negativen Höhenwert aktualisiert, der für Positionierung und Übergang verwendet wird.