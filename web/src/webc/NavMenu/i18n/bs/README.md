# Navigacijski meni prilagođen različitim veličinama ekrana

- Radna površina: Veze se prikazuju u horizontalnim pločicama
- Mobilna verzija: Dugme sažimajte u meni, kliknite da izvučete fioku bočne trake
- Aktivacija rute: automatski usporedite trenutnu putanju i hash vrijednost i dodajte naziv klase statusa aktivacije na odgovarajući link `A`
- Automatsko zatvaranje: Kada mobilni terminal klikne na vezu ili vanjsku pozadinu bočne trake, bočna traka se automatski zatvara.

## koristiti

```html
<c-nav-menu>
  <a href="/">naslovna strana</a>
  <a href="/doc">dokument</a>
  <a href="/about">o</a>
</c-nav-menu>
```

## ilustrirati

Kada se komponenta inicijalizira, ako nema `c-side` u podređenom elementu, dugme mobilnog menija i `c-side` bočna traka će se automatski kreirati, a sve oznake `a` će biti premještene u bočnu traku.

## stilska klasa

- **A**: Aktivirani link će automatski dodati naziv klase `A`