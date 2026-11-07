# Ibabaw nga navigation bar nga awtomatik nga mo-collapse o molapad kung mag-scroll ang panid

- **Awtomatikong Pagtago**: Awtomatikong itago kung mag-scroll down sa panid.
- **Awtomatikong pagpalapad**: Awtomatikong gipakita kon ang panid gi-scroll pataas o ang scroll bar mawala.
- **Pag-optimize sa Performance**: Gamita ang `requestAnimationFrame` aron i-throttle ang pag-scroll.
- **Pagpagawas sa mga kahinguhaan**: Ang mga tigpaminaw ug tigpaniid sa panghitabo awtomatik nga tangtangon kung ang component maguba.

## Gamita ang demo

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
    <div class="content">Pag-swipe paubos aron itago ang nabigasyon, pag-swipe pataas aron ipakita kini</div>
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

## Mga panudlo alang sa paggamit

Kinahanglang gamiton isip bata o kaliwat nga elemento sa `<c-vs>`.

## kahimtang sa estilo

Gibalhin sa component ang mosunod nga ngalan sa klase sa `nav.Nav`:

- `D`: Ipakita ang status (slide paubos o sa ibabaw).
- `I`: Gitago ang kahimtang sa transisyon.
- `H`: Gitago nga kahimtang.

## mga kausaban sa estilo

- `--top`: Awtomatikong gi-update sa component ngadto sa negatibong bili sa gitas-on, gigamit alang sa pagpoposisyon ug pagbalhin.