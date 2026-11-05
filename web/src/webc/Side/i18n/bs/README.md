# Ploče ladica koje se izvlače sa strane

- Automatski kreirajte unutrašnje strukture za pomicanje
- Kliknite na vanjsko područje da se automatski skupi
- Prevucite lijevo i desno da zatvorite
- Bočno klizno pozicioniranje u lijevom i desnom smjeru
- Osigurajte otvorene i zatvorene događaje

## Koristite demo

```html
<button class="open-btn">Otvori</button>

<c-side class="right">
  <div>Sadržaj ladice</div>
</c-side>

<script type="module">
import { open } from "webc.site/Side.js";

const btn = document.querySelector(".open-btn"),
  side = document.querySelector("c-side");

btn.onclick = () => open(side);
</script>
```

## interfejs

### metoda

- `open(el)`: Funkcija izvoza, otvorena ladica
- `close(el)`: Izvoz funkcija, zatvori ladicu
- `el.open()`: metoda instance, otvori ladicu
- `el.close()`: metoda instance, zatvori ladicu

### događaj

- `open`: Pali se kada je uključen
- `close`: Pali kada je zatvoren

## stilska klasa

- `.right`: desno izvlačenje pozicioniranja
- `.top`: Izvucite pozicioniranje
- `.bottom`: pomaknite se iz pozicioniranja
- (Ako nije postavljeno, zadana postavka je povlačenje lijevo)
- `.open`: otvoren status
- `.H`: skriveno stanje
- `.T`: stanje prijelaza animacije