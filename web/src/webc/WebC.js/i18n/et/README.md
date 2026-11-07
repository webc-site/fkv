# Dokumentatsioon ja lähtekood komponentide või tööriistafunktsioonide renderdamiseks

- Kahe veeruga reageeriv paigutus, kus dokumendid kuvatakse vasakul ja lähtekood kuvatakse paremal
- Dokumentatsioon ja lähtekood toetavad lähteaadressile hüppamiseks kiirklahvi Redigeerimine ja Kood.
- Pealkiri ja tutvustus kuvatakse ülaosas, toetades i18n automaattõlget.

## Kasutage demo

```html
<c-webc-js src="/src/js/dom.js" href="/doc/{code}/js/dom.md"></c-webc-js>
```

## Omandi parameetrid

- `src`: lähtekoodi URL-aadress.
- `href`: dokumendi märgistuse URL-aadress, toetab praeguse keelega kohanemiseks kohatäite `{code}` kasutamist.