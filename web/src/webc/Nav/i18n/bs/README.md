# Gornja traka za navigaciju koja se automatski skuplja ili širi kada se stranica pomiče

- **Automatsko sakrivanje**: Automatski sakrivanje pri pomicanju stranice prema dolje.
- **Automatsko proširenje**: Automatski se prikazuje kada se stranica pomakne prema gore ili traka za pomicanje nestane.
- **Optimizacija performansi**: Koristite `requestAnimationFrame` da biste smanjili pomicanje.
- **Oslobodite resurse**: Slušatelji događaja i posmatrači se automatski uklanjaju kada se komponenta uništi.

## Koristite demo

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
    <div class="content">Prevucite prstom prema dolje da sakrijete navigaciju, prevucite prema gore da je prikažete</div>
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

## Uputstvo za upotrebu

Mora se koristiti kao podređeni ili potomak elementa `<c-vs>`.

## status stila

Komponenta mijenja sljedeće ime klase na `nav.Nav`:

- `D`: Prikaži status (klizanje prema dolje ili na vrhu).
- `I`: Prijelazno stanje je skriveno.
- `H`: Skriven status.

## varijable stila

- `--top`: Komponenta automatski ažurira na negativnu vrijednost visine, koja se koristi za pozicioniranje i prijelaz.