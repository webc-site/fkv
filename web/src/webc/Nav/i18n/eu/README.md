# Orria korritzean automatikoki tolesten edo zabaltzen den goiko nabigazio-barra

- **Ezkutatu automatikoki**: automatikoki ezkutatu orrialdean behera korritzean.
- **Hedatu automatikoki**: automatikoki bistaratzen da orria gora korritzen denean edo korritze-barra desagertzen denean.
- **Errendimenduaren optimizazioa**: Erabili `requestAnimationFrame` desplazamendua murrizteko.
- **Askatu baliabideak**: Gertaeren entzuleak eta behatzaileak automatikoki kentzen dira osagaia suntsitzen denean.

## Erabili demoa

```html
<main>
  <c-vs>
    <nav class="Nav">
      <c-nav>
        <a class="logo" href="#">LOGO</a>
      </c-nav>
      <c-nav>
        <a class="item" href="#">Esteka</a>
      </c-nav>
    </nav>
    <div class="content">Irristatu behera nabigazioa ezkutatzeko, irristatu gora erakusteko</div>
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

## Erabiltzeko jarraibideak

`<c-vs>`-ren seme-alaba edo ondorengo elementu gisa erabili behar da.

## estilo egoera

Osagaiak klase-izen hau aldatzen du `nav.Nav`-n:

- `D`: Erakutsi egoera (lestatu behera edo goian).
- `I`: trantsizio-egoera ezkutatzen ari da.
- `H`: Ezkutuko egoera.

## estilo aldagaiak

- `--top`: Osagaiak automatikoki eguneratzen du altuera-balio negatibo batera, kokatzeko eta trantsiziorako erabiltzen dena.