# Dokumentacija i izvorni kod za komponente renderiranja ili funkcije alata

- Responzivni izgled u dva stupca, s dokumentima prikazanim s lijeve strane i izvornim kodom prikazanim s desne strane
- Podrška za dokumentaciju i izvorni kod Gumbi prečaca za uređivanje i kod za skok na izvornu adresu.
- Naslov i uvod prikazani su na vrhu, podržavajući i18n automatski prijevod.

## Koristite demo

```html
<c-webc-js src="/src/js/dom.js" href="/doc/{code}/js/dom.md"></c-webc-js>
```

## Parametri svojstva

- `src`: URL adresa izvornog koda.
- `href`: URL adresa označavanja dokumenta, podržava korištenje `{code}` rezerviranog mjesta za prilagodbu trenutnom jeziku.