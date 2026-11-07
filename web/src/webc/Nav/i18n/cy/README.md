# Bar llywio uchaf sy'n cwympo neu'n ehangu'n awtomatig pan fydd y dudalen yn sgrolio

- ** Cuddio Awtomatig**: Cuddiwch yn awtomatig wrth sgrolio i lawr y dudalen.
- **Awto-ehangu**: Dangosir yn awtomatig pan fydd y dudalen yn cael ei sgrolio i fyny neu pan fydd y bar sgrolio yn diflannu.
- **Optimeiddio Perfformiad**: Defnyddiwch `requestAnimationFrame` i sbarduno sgrolio.
- **Adnoddau rhyddhau**: Mae gwrandawyr digwyddiadau ac arsylwyr yn cael eu tynnu'n awtomatig pan fydd y gydran yn cael ei dinistrio.

## Defnyddiwch y demo

```html
<main>
  <c-vs>
    <nav class="Nav">
      <c-nav>
        <a class="logo" href="#">LOGO</a>
      </c-nav>
      <c-nav>
        <a class="item" href="#">Cyswllt</a>
      </c-nav>
    </nav>
    <div class="content">Sychwch i lawr i guddio'r llywio, swipe i fyny i'w ddangos</div>
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

## Cyfarwyddiadau ar gyfer defnyddio

Rhaid ei ddefnyddio fel elfen plentyn neu ddisgynnydd `<c-vs>`.

## statws arddull

Mae'r gydran yn newid yr enw dosbarth canlynol ar `nav.Nav`:

- `D`: Dangos statws (sleid i lawr neu ar y brig).
- `I`: Cyflwr trawsnewid yn cael ei guddio.
- `H`: Statws cudd.

## newidynnau arddull

- `--top`: Wedi'i ddiweddaru'n awtomatig gan y gydran i werth negyddol uchder, a ddefnyddir ar gyfer lleoli a thrawsnewid.