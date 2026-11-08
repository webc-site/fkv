# Dokumentasie en bronkode vir die weergawe van komponente of gereedskapfunksies

- Twee-kolom responsiewe uitleg, met dokumente wat aan die linkerkant vertoon word en bronkode aan die regterkant
- Dokumentasie en bronkode-ondersteuning Wysig- en Kode-kortpadknoppies om na die bronadres te spring.
- Die titel en inleiding word boaan vertoon, wat i18n outomatiese vertaling ondersteun.

## Gebruik die demo

```html
<c-webc-js src="/src/js/dom.js" href="/doc/{code}/js/dom.md"></c-webc-js>
```

## Eiendom parameters

- `src`: URL-adres van die bronkode.
- `href`: URL-adres van dokumentmarkering, ondersteun die gebruik van `{code}` plekhouer om by die huidige taal aan te pas.