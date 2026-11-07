# Panely zásuviek, ktoré sa vysúvajú z boku

- Automaticky vytvorte vnútorné rolovacie štruktúry
- Kliknutím na vonkajšiu oblasť sa automaticky zbalí
- Zatvorte potiahnutím prstom doľava a doprava
- Bočné posuvné polohovanie v smere doľava a doprava
- Poskytujte otvorené a zatvorené udalosti

## Použite demo

```html
<button class="open-btn">OTVORENÉ</button>

<c-side class="right">
  <div>Obsah zásuvky</div>
</c-side>

<script type="module">
import { open } from "webc.site/Side.js";

const btn = document.querySelector(".open-btn"),
  side = document.querySelector("c-side");

btn.onclick = () => open(side);
</script>
```

## rozhranie

### metóda

- `open(el)`: Funkcia exportu, otvorenie zásuvky
- `close(el)`: Funkcia exportu, zatvorenie zásuvky
- `el.open()`: metóda inštancie, otvorenie zásuvky
- `el.close()`: metóda inštancie, zatvorte zásuvku

### udalosť

- `open`: Spustí sa pri zapnutí
- `close`: Spustí sa pri zatvorení

## štýlová trieda

- `.right`: vysúvanie vpravo
- `.top`: Vysunutie polohy
- `.bottom`: vysunúť z polohy
- (Ak nie je nastavené, predvolene sa vysunie doľava)
- `.open`: otvorený stav
- `.H`: skrytý stav
- `.T`: stav prechodu animácie