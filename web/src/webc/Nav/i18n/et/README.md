# Ülemine navigeerimisriba, mis lehe kerimisel automaatselt aheneb või laieneb

- **Automaatne peitmine**: peida automaatselt lehel alla kerimisel.
- **Automaatne laiendamine**: kuvatakse automaatselt, kui lehekülge üles keritakse või kerimisriba kaob.
- **Toimivuse optimeerimine**: kasutage kerimise reguleerimiseks `requestAnimationFrame`.
- **Avalda ressursid**: sündmuste kuulajad ja vaatlejad eemaldatakse automaatselt, kui komponent hävib.

## Kasutage demo

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
    <div class="content">Navigeerimise peitmiseks pühkige alla, kuvamiseks pühkige üles</div>
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

## Kasutusjuhend

Tuleb kasutada elemendi `<c-vs>` alam- või järglasena.

## stiili staatus

Komponent lülitab sisse järgmise klassinime `nav.Nav`:

- `D`: oleku kuvamine (libistage alla või üleval).
- `I`: üleminekuolek on peidetud.
- `H`: peidetud olek.

## stiilimuutujad

- `--top`: komponendi poolt automaatselt värskendatud kõrgus negatiivseks väärtuseks, mida kasutatakse positsioneerimiseks ja üleminekuks.