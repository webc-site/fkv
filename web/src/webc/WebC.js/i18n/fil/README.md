# Dokumentasyon at source code para sa pag-render ng mga bahagi o mga function ng tool

- Dalawang-column na tumutugon na layout, na may mga dokumentong ipinapakita sa kaliwa at source code na ipinapakita sa kanan
- Dokumentasyon at source code na sumusuporta sa Edit at Code shortcut na mga button para tumalon sa source address.
- Ang pamagat at pagpapakilala ay ipinapakita sa itaas, na sumusuporta sa awtomatikong pagsasalin ng i18n.

## Gamitin ang demo

```html
<c-webc-js src="/src/js/dom.js" href="/doc/{code}/js/dom.md"></c-webc-js>
```

## Mga parameter ng ari-arian

- `src`: URL address ng source code.
- `href`: URL address ng markdown ng dokumento, sumusuporta sa paggamit ng `{code}` placeholder upang umangkop sa kasalukuyang wika.