# Bezpiecznie analizuj i renderuj tekst Markdown do formatu HTML.

- **Oczyszczanie bezpieczeństwa**: obrona przed atakami XSS przy użyciu natywnego `setHTML` API lub `DOMParser` narzędzi czyszczących
- **Wyodrębnianie treści**: Automatycznie odczytuje zawartość tekstową domyślnego gniazda podczas inicjalizacji i renderuje ją
- **Wbudowany skład**: gotowe listy, tabele, bloki kodu, style okien cytatów i ostrzeżeń

## Skorzystaj z wersji demonstracyjnej

### renderowanie statyczne

```html
<c-md>
# tytuł
To jest fragment zawartości Markdown
</c-md>
```

### przypisanie dynamiczne

```html
<c-md id="md"></c-md>

<script type="module">
import "webc.site/Md.js";
document.querySelector("#md").value = "#Nowy tytuł\nNowa treść";
</script>
```

## Parametry interfejsu

Przekaż tekst Markdown bezpośrednio w tagu (jako miejsce domyślne) lub aktualizuj dynamicznie za pomocą `value`.

## Opis stylu

Klasa stylu `.Md` jest dodawana automatycznie podczas inicjalizacji komponentu. Obsługuje następujący układ rozszerzonego okna ostrzegawczego Markdown (uruchamiany przez składnię odniesienia):

- `[!NOTE]`: Monituj o informacje
- `[!TIP]`: Sugestie/wskazówki
- `[!IMPORTANT]`: Ważna uwaga
- `[!WARNING]`: Ostrzeżenie o ryzyku
- `[!CAUTION]`: Ostrzeżenie o wyjątkowo wysokim ryzyku

## mechanizm bezpieczeństwa

Podczas renderowania najpierw wywołaj natywny interfejs API `setHTML`, aby bezpiecznie wstrzyknąć kod HTML. Jeśli przeglądarka tego nie obsługuje, automatycznie przejdzie na rekurencyjny program czyszczący w oparciu o `DOMParser` i zastosuje następującą politykę bezpieczeństwa:
- Usuń niebezpieczne tagi (np. `script`, `iframe` itp.)
- Wyczyść `on*` atrybuty zdarzenia
- Broń się przed atakami XSS, filtrując atrybuty, zaczynając od niebezpiecznych protokołów, takich jak `javascript:`, `vbscript:` i `data:` (z wyjątkiem `data:image/`).