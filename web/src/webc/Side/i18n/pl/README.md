# Panele szuflad wysuwane z boku

- Automatycznie twórz wewnętrzne struktury przewijania
- Kliknij obszar zewnętrzny, aby automatycznie zwinąć
- Przesuń palcem w lewo i w prawo, aby zamknąć
- Boczne pozycjonowanie przesuwne w lewo i prawo
- Zapewnij wydarzenia otwarte i zamknięte

## Skorzystaj z wersji demonstracyjnej

```html
<button class="open-btn">Otwarte</button>

<c-side class="right">
  <div>Zawartość szuflady</div>
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

- `open(el)`: Funkcja eksportu, otwarta szuflada
- `close(el)`: Funkcja eksportu, zamknij szufladę
- `el.open()`: metoda instancji, otwórz szufladę
- `el.close()`: metoda instancji, zamknij szufladę

### wydarzenie

- `open`: Włącza się po włączeniu
- `close`: Odpala po zamknięciu

## klasa stylu

- `.right`: pozycjonowanie wysuwanego prawego suwaka
- `.top`: Pozycjonowanie wysuwane
- `.bottom`: wysuń się z pozycjonowania
- (Jeśli nie ustawiono, domyślnie przesuwa się w lewo)
- `.open`: stan otwarty
- `.H`: stan ukryty
- `.T`: stan przejścia animacji