# Shiriti i sipërm i navigimit që paloset ose zgjerohet automatikisht kur faqja lëviz

- **Fshehja automatike**: Fshih automatikisht kur lëviz faqen poshtë.
- **Zgjerimi automatik**: Shfaqet automatikisht kur faqja lëviz lart ose kur shiriti i lëvizjes zhduket.
- **Optimizimi i performancës**: Përdorni `requestAnimationFrame` për të penguar lëvizjen.
- **Resurset e lëshimit**: Dëgjuesit dhe vëzhguesit e ngjarjeve hiqen automatikisht kur komponenti shkatërrohet.

## Përdorni demonstrimin

```html
<main>
  <c-vs>
    <nav class="Nav">
      <c-nav>
        <a class="logo" href="#">LOGO</a>
      </c-nav>
      <c-nav>
        <a class="item" href="#">Lidhje</a>
      </c-nav>
    </nav>
    <div class="content">Rrëshqitni poshtë për të fshehur navigimin, rrëshqitni lart për ta shfaqur atë</div>
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

## Udhëzime për përdorim

Duhet të përdoret si element fëmijë ose pasardhës i `<c-vs>`.

## statusi i stilit

Komponenti ndërron emrin e klasës vijuese në `nav.Nav`:

- `D`: Shfaq statusin (rrëshqitni poshtë ose lart).
- `I`: Gjendja e tranzicionit po fshihet.
- `H`: Statusi i fshehur.

## variablat e stilit

- `--top`: Përditësohet automatikisht nga komponenti në një vlerë negative të lartësisë, e përdorur për pozicionimin dhe kalimin.