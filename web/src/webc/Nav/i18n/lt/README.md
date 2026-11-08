# Viršutinė naršymo juosta, kuri automatiškai susitraukia arba išplečiama, kai puslapis slenka

- **Automatiškai slėpti**: automatiškai paslėpti slenkant puslapiu žemyn.
- **Automatinis išplėtimas**: automatiškai rodomas, kai puslapis slenkamas aukštyn arba slinkties juosta išnyksta.
- **Našumo optimizavimas**: naudokite `requestAnimationFrame`, kad sumažintumėte slinkimą.
- **Paleisti išteklius**: įvykių klausytojai ir stebėtojai automatiškai pašalinami, kai komponentas sunaikinamas.

## Naudokite demonstracinę versiją

```html
<main>
  <c-vs>
    <nav class="Nav">
      <c-nav>
        <a class="logo" href="#">LOGO</a>
      </c-nav>
      <c-nav>
        <a class="item" href="#">Nuoroda</a>
      </c-nav>
    </nav>
    <div class="content">Braukite žemyn, kad paslėptumėte navigaciją, braukite aukštyn, kad ją parodytumėte</div>
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

## Naudojimo instrukcijos

Turi būti naudojamas kaip antrinis arba palikuonis `<c-vs>` elementas.

## stiliaus statusas

Komponentas įjungia šį klasės pavadinimą `nav.Nav`:

- `D`: rodyti būseną (slinkti žemyn arba viršuje).
- `I`: perėjimo būsena paslėpta.
- `H`: paslėpta būsena.

## stiliaus kintamieji

- `--top`: komponentas automatiškai atnaujina iki neigiamos aukščio vertės, naudojama padėties nustatymui ir perėjimui.