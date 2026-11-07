# Horný navigačný panel, ktorý sa pri posúvaní stránky automaticky zbalí alebo rozbalí

- **Automaticky skryť**: Automaticky sa skryje pri posúvaní stránky nadol.
- **Automatické rozbalenie**: Automaticky sa zobrazí, keď sa stránka posunie nahor alebo zmizne posuvná lišta.
- **Optimalizácia výkonu**: Použite `requestAnimationFrame` na obmedzenie posúvania.
- **Prostriedky uvoľnenia**: Poslucháči a pozorovatelia udalostí sa automaticky odstránia, keď je komponent zničený.

## Použite demo

```html
<main>
  <c-vs>
    <nav class="Nav">
      <c-nav>
        <a class="logo" href="#">LOGO</a>
      </c-nav>
      <c-nav>
        <a class="item" href="#">Odkaz</a>
      </c-nav>
    </nav>
    <div class="content">Potiahnutím nadol skryjete navigáciu, potiahnutím nahor ju zobrazíte</div>
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

## Návod na použitie

Musí byť použitý ako podradený alebo podradený prvok `<c-vs>`.

## stav štýlu

Komponent prepne nasledujúci názov triedy na `nav.Nav`:

- `D`: Zobraziť stav (posunúť nadol alebo hore).
- `I`: Stav prechodu je skrytý.
- `H`: Skrytý stav.

## štýlové premenné

- `--top`: Automaticky aktualizované komponentom na zápornú hodnotu výšky, ktorá sa používa na umiestnenie a prechod.