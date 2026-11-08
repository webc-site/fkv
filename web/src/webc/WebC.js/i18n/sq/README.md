# Dokumentacioni dhe kodi burimor për paraqitjen e komponentëve ose funksioneve të veglave

- Struktura e përgjegjshme me dy kolona, ​​me dokumente të shfaqura në të majtë dhe kodin burimor të shfaqur në të djathtë
- Dokumentacioni dhe kodi burimor mbështesin butonat e shkurtores Edit dhe Code për të kaluar në adresën e burimit.
- Titulli dhe prezantimi shfaqen në krye, duke mbështetur përkthimin automatik i18n.

## Përdorni demonstrimin

```html
<c-webc-js src="/src/js/dom.js" href="/doc/{code}/js/dom.md"></c-webc-js>
```

## Parametrat e vetive

- `src`: adresa URL e kodit burimor.
- `href`: Adresa e URL-së së shënimit të dokumentit, mbështet përdorimin e vendmbajtësit `{code}` për t'u përshtatur me gjuhën aktuale.