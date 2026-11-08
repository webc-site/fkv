# Dokumentacja i kod źródłowy do renderowania komponentów lub funkcji narzędzi

- Dwukolumnowy responsywny układ, z dokumentami wyświetlanymi po lewej stronie i kodem źródłowym po prawej
- Dokumentacja i kod źródłowy obsługują przyciski skrótu Edytuj i Kod umożliwiające przejście do adresu źródłowego.
- Tytuł i wprowadzenie są wyświetlane u góry, obsługując automatyczne tłumaczenie i18n.

## Skorzystaj z wersji demonstracyjnej

```html
<c-webc-js src="/src/js/dom.js" href="/doc/{code}/js/dom.md"></c-webc-js>
```

## Parametry właściwości

- `src`: Adres URL kodu źródłowego.
- `href`: adres URL przeceny dokumentu, obsługuje użycie symbolu zastępczego `{code}` w celu dostosowania do bieżącego języka.