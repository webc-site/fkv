# Panely zásuvek, které se vysouvají z boku

- Automaticky vytvářet vnitřní rolovací struktury
- Kliknutím na vnější oblast se automaticky sbalí
- Zavřete přejetím prstem doleva a doprava
- Boční posuvné polohování v levém a pravém směru
- Poskytujte otevřené a uzavřené události

## Použijte demo

```html
<button class="open-btn">OTEVŘENO</button>

<c-side class="right">
  <div>Obsah šuplíku</div>
</c-side>

<script type="module">
import { open } from "webc.site/Side.js";

const btn = document.querySelector(".open-btn"),
  side = document.querySelector("c-side");

btn.onclick = () => open(side);
</script>
```

## rozhraní

### metoda

- `open(el)`: Funkce exportu, otevřít zásuvku
- `close(el)`: Funkce exportu, zavřete zásuvku
- `el.open()`: metoda instance, otevřít zásuvku
- `el.close()`: metoda instance, zavřete zásuvku

### událost

- `open`: Spustí se při zapnutí
- `close`: Spustí se při zavření

## stylová třída

- `.right`: umístění vysouvání vpravo
- `.top`: Umístění vysunutí
- `.bottom`: vysunout z polohy
- (Pokud není nastaveno, výchozí nastavení je vysunout doleva)
- `.open`: otevřený stav
- `.H`: skrytý stav
- `.T`: stav přechodu animace