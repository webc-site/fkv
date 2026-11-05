# Felső navigációs sáv, amely automatikusan összecsukódik vagy kinyílik, amikor az oldal görget

- **Automatikus elrejtés**: Automatikus elrejtés, amikor lefelé görget az oldalon.
- **Automatikus kibontás**: Automatikusan megjelenik, ha az oldalt felfelé görgetik, vagy a görgetősáv eltűnik.
- **Teljesítményoptimalizálás**: Használja a `requestAnimationFrame` gombot a görgetés szabályozásához.
- **Erőforrások kiadása**: Az eseményfigyelők és megfigyelők automatikusan eltávolításra kerülnek, ha az összetevő megsemmisül.

## Használd a demót

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
    <div class="content">Csúsztassa lefelé ujját a navigáció elrejtéséhez, csúsztassa felfelé a megjelenítéséhez</div>
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

## Használati utasítás

A(z) `<c-vs>` gyermek vagy leszármazott elemeként kell használni.

## stílus állapota

Az összetevő a következő osztálynevet kapcsolja be `nav.Nav`:

- `D`: Állapot megjelenítése (csúsztassa lefelé vagy felül).
- `I`: Az átmeneti állapot elrejtve.
- `H`: Rejtett állapot.

## stílusváltozók

- `--top`: A komponens automatikusan frissíti a magasság negatív értékére, pozicionáláshoz és átmenethez.