# Składnik wpisu na blogu

- **Automatyczne grupowanie lat**: Automatycznie klasyfikuj lata według daty artykułu i układaj je w odwrotnej kolejności według roku
- **Uproszczony format daty**: Automatycznie usuwaj przedrostek roku podczas wyświetlania i wyświetlaj tylko miesiąc i dzień
- **Podsumowanie i tekst przeceny**: Wbudowany podgląd podsumowania listy i renderowanie pełnego tekstu pojedynczego artykułu
- **Trójwymiarowy tytuł z gradientem**: Zintegruj `H1Txt`, aby wyświetlić nagłówek i datę wydania

## Skorzystaj z wersji demonstracyjnej

```html
<c-blog></c-blog>

<script type="module">
import "webc.site/Blog.js";

const blog = document.querySelector("c-blog");
blog.prefix = "/blog/";
blog.mdUrl = (key, lang) => "/doc/" + lang + "/blog/" + key + ".md";
blog._ = {
  „2026/publikacja”: [„tytuł”, „streszczenie”, „27.07.2026”]
};

// Przełącz, aby załadować określoną treść artykułu (przekaż pusty ciąg, aby wrócić do listy)
blog.load("2026/publish");
</script>
```

## Parametry interfejsu

### Właściwości i metody

- `blog.prefix`: pobierz lub ustaw prefiks linku do artykułu (np. `"/blog/"`)
- `blog.mdUrl`: Pobierz lub ustaw funkcję generowania adresu artykułu Markdown `(key, lang) => url`
- `blog._`: Pobierz lub ustaw słownik metadanych listy artykułów `{ [key]: [title, summary, date] }`
- `blog.load(key)`: Przełącz, aby załadować treść artykułu o podanym kluczu, przekaż pusty ciąg `""`, aby wrócić do listy blogów