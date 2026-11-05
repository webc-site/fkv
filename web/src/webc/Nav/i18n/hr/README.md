# Gornja navigacijska traka koja se automatski sažima ili proširuje kada se stranica pomiče

- **Automatsko skrivanje**: Automatsko skrivanje prilikom pomicanja stranice prema dolje.
- **Automatsko proširivanje**: Automatski se prikazuje kada se stranica pomiče prema gore ili kada traka za pomicanje nestane.
- **Optimizacija performansi**: Koristite `requestAnimationFrame` za usporavanje pomicanja.
- **Oslobodi resurse**: Slušatelji i promatrači događaja automatski se uklanjaju kada se komponenta uništi.

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
    <div class="content">Prijeđite prstom prema dolje da biste sakrili navigaciju, prijeđite prstom prema gore da biste je prikazali</div>
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

## Upute za uporabu

Mora se koristiti kao element podređeni ili potomak `<c-vs>`.

## status stila

Komponenta prebacuje sljedeći naziv klase na `nav.Nav`:

- `D`: Prikaži status (kliznite prema dolje ili na vrh).
- `I`: prijelazno stanje je skriveno.
- `H`: Skriven status.

## stilske varijable

- `--top`: Komponenta automatski ažurira na negativnu vrijednost visine, koja se koristi za pozicioniranje i prijelaz.