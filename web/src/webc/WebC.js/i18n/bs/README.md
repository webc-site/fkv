# Dokumentacija i izvorni kod za renderiranje komponenti ili funkcija alata

- Reagirajući izgled u dvije kolone, s dokumentima prikazanim na lijevoj strani i izvornim kodom prikazanim na desnoj strani
- Podrška za dokumentaciju i izvorni kod, dugmad za prečice Uredi i Kod za skok na izvornu adresu.
- Naslov i uvod su prikazani na vrhu, podržavajući i18n automatski prijevod.

## Koristite demo

```html
<c-webc-js src="/src/js/dom.js" href="/doc/{code}/js/dom.md"></c-webc-js>
```

## Parametri svojstva

- `src`: URL adresa izvornog koda.
- `href`: URL adresa umanjenja dokumenta, podržava korištenje `{code}` čuvara mjesta za prilagođavanje trenutnom jeziku.