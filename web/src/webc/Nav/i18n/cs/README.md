# Horní navigační panel, který se při posouvání stránky automaticky sbalí nebo rozbalí

- **Auto Hide**: Automaticky se skryje při posouvání stránky dolů.
- **Automatické rozbalení**: Automaticky se zobrazí, když se stránka posune nahoru nebo zmizí posuvník.
- **Optimalizace výkonu**: Použijte `requestAnimationFrame` k omezení rolování.
- **Prostředky vydání**: Posluchači a pozorovatelé událostí jsou automaticky odstraněni, když je komponenta zničena.

## Použijte demo

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
    <div class="content">Přejetím dolů skryjete navigaci, přejetím nahoru ji zobrazíte</div>
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

## Návod k použití

Musí být použit jako podřízený nebo podřízený prvek `<c-vs>`.

## stav stylu

Komponenta přepne na `nav.Nav` následující název třídy:

- `D`: Zobrazit stav (přejetím dolů nebo nahoře).
- `I`: Stav přechodu je skrytý.
- `H`: Skrytý stav.

## stylové proměnné

- `--top`: Automaticky aktualizováno komponentou na zápornou hodnotu výšky, která se používá pro umístění a přechod.