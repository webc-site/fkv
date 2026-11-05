# Augšējā navigācijas josla, kas automātiski sakļaujas vai izvēršas, ritinot lapu

- **Automātiski paslēpt**: automātiski paslēpt, ritinot lapu uz leju.
- **Automātiski izvērst**: tiek automātiski parādīts, kad lapa tiek ritināta uz augšu vai ritjosla pazūd.
- **Veiktspējas optimizācija**: izmantojiet `requestAnimationFrame`, lai samazinātu ritināšanu.
- **Izlaiduma resursi**: notikumu uztvērēji un novērotāji tiek automātiski noņemti, kad komponents tiek iznīcināts.

## Izmantojiet demonstrāciju

```html
<main>
  <c-vs>
    <nav class="Nav">
      <c-nav>
        <a class="logo" href="#">LOGO</a>
      </c-nav>
      <c-nav>
        <a class="item" href="#">Saite</a>
      </c-nav>
    </nav>
    <div class="content">Velciet uz leju, lai paslēptu navigāciju, un velciet uz augšu, lai to parādītu</div>
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

## Lietošanas instrukcija

Jāizmanto kā `<c-vs>` bērna vai pēcnācēja elements.

## stila statuss

Komponents ieslēdz šādu klases nosaukumu `nav.Nav`:

- `D`: rādīt statusu (slīdiet uz leju vai augšpusē).
- `I`: pārejas stāvoklis tiek paslēpts.
- `H`: slēptais statuss.

## stila mainīgie

- `--top`: komponents automātiski atjaunina uz negatīvu augstuma vērtību, ko izmanto pozicionēšanai un pārejai.