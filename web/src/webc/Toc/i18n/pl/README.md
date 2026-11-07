# Komponent nawigacji katalogowej

- **Responsywna szuflada**: Wyświetlaj pasek boczny na szerokim ekranie, automatycznie zwijaj na wąskim ekranie i współpracuj z `c-side` szufladą i przyciskiem wyzwalającym tytuł
- **Sterownik wielojęzyczny**: Obsługuje słownik obiektowy lub funkcje wielojęzyczne, automatycznie reaguje na zmianę języka
- **Automatyczny podkatalog**: Automatycznie analizuj tytuły `h1`~`h6` w kontenerze docelowym, aby wygenerować katalog dodatkowy i monitorować podświetlenie przewijania rzutni Hash.
- **Ładowanie asynchroniczne**: Kliknij element katalogu, aby automatycznie asynchronicznie uzyskać tekst Markdown i wstawić go do obszaru zawartości, wyświetlając animację ładowania `Ing`

## Skorzystaj z wersji demonstracyjnej

```html
<c-toc></c-toc>
<c-md></c-md>

<script type="module">
import "webc.site/Toc.js";

const toc = document.querySelector("c-toc");
toc.target = document.querySelector("c-md");
toc.prefix = "doc";
toc.mdUrl = (key, lang) => "/doc/" + lang + "/" + key + ".md";
toc.li = (lang) => ({
  "README": "WebC.site",
  "use": "użyj komponentu"
});
</script>
```

## Parametry interfejsu

### Właściwości i metody

- `toc.prefix`: Pobierz lub ustaw ciąg prefiksu routingu (np. `"doc"`, ukośnik końcowy jest opcjonalny), klucz `README` zostanie automatycznie odwzorowany na pustą ścieżkę (tj. sam przedrostek)
- `toc.li`: Pobierz lub ustaw funkcję generowania wielojęzycznego katalogu `(lang) => ({ key: title })`
- `toc.mdUrl`: Pobierz lub ustaw funkcję generowania adresu URL Markdown `(key, lang) => url`
- `toc.target`: Pobierz lub ustaw powiązany docelowy węzeł renderujący (taki jak `c-md`). Jeśli nie zostanie określony, automatycznie znajdzie `c-md` / `main` / `article`
- `toc.load(key)`: Ręczne uruchomienie ładowania dokumentu i rozwijania katalogu odpowiadającego kluczowi
- `toc.sync()`: Ręcznie przeanalizuj tytuł w bieżącym węźle docelowym, zsynchronizuj katalog dodatkowy i podświetl stan