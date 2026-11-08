# Navigacijski izbornik prilagođen različitim veličinama zaslona

- Radna površina: veze se prikazuju u horizontalnim pločicama
- Mobilna verzija: Sažmi u gumb izbornika, klikni da izvučeš ladicu bočne trake
- Usmjerite aktivaciju: automatski usporedite trenutni put i hash vrijednost i dodajte naziv klase statusa aktivacije odgovarajućoj vezi `A`
- Automatsko zatvaranje: kada mobilni terminal klikne na vezu ili vanjsku pozadinu bočne trake, bočna traka se automatski zatvara.

## koristiti

```html
<c-nav-menu>
  <a href="/">naslovna stranica</a>
  <a href="/doc">dokument</a>
  <a href="/about">oko</a>
</c-nav-menu>
```

## ilustrirati

Kada se komponenta inicijalizira, ako nema `c-side` u podređenom elementu, gumb mobilnog izbornika i bočna traka `c-side` automatski će se stvoriti, a sve oznake `a` premjestit će se u bočnu traku.

## stilska klasa

- **A**: Aktivirana veza automatski će dodati naziv klase `A`