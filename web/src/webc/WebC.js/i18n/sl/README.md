# Dokumentacija in izvorna koda za upodabljanje komponent ali orodnih funkcij

- Odzivna postavitev v dveh stolpcih, z dokumenti, prikazanimi na levi, in izvorno kodo, prikazano na desni
- Dokumentacija in izvorna koda podpirata bližnjična gumba Urejanje in Koda za skok na izvorni naslov.
- Naslov in uvod sta prikazana na vrhu, kar podpira samodejno prevajanje i18n.

## Uporabite demo

```html
<c-webc-js src="/src/js/dom.js" href="/doc/{code}/js/dom.md"></c-webc-js>
```

## Parametri lastnosti

- `src`: naslov URL izvorne kode.
- `href`: URL naslov oznake dokumenta, podpira uporabo nadomestnega znaka `{code}` za prilagajanje trenutnemu jeziku.