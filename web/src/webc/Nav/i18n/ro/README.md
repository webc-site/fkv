# Bara de navigare de sus, care se restrânge sau se extinde automat atunci când pagina derulează

- **Ascunde automat**: ascunde automat când derulezi în jos în pagină.
- **Extindere automată**: Afișat automat când pagina este derulată în sus sau bara de defilare dispare.
- **Optimizarea performanței**: utilizați `requestAnimationFrame` pentru a accelera derularea.
- **Resurse de lansare**: ascultătorii și observatorii evenimentelor sunt eliminați automat când componenta este distrusă.

## Utilizați demonstrația

```html
<main>
  <c-vs>
    <nav class="Nav">
      <c-nav>
        <a class="logo" href="#">LOGO</a>
      </c-nav>
      <c-nav>
        <a class="item" href="#">Legătură</a>
      </c-nav>
    </nav>
    <div class="content">Glisați în jos pentru a ascunde navigarea, glisați în sus pentru a o afișa</div>
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

## Instructiuni de utilizare

Trebuie folosit ca element copil sau descendent al `<c-vs>`.

## statutul stilului

Componenta comută următorul nume de clasă pe `nav.Nav`:

- `D`: afișați starea (glisați în jos sau în partea de sus).
- `I`: starea de tranziție este ascunsă.
- `H`: stare ascunsă.

## variabile de stil

- `--top`: Actualizat automat de către componentă la o valoare negativă a înălțimii, utilizată pentru poziționare și tranziție.