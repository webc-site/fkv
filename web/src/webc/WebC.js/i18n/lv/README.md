# Dokumentācija un pirmkods komponentu vai rīku funkciju renderēšanai

- Divu kolonnu adaptīvs izkārtojums ar dokumentiem, kas tiek parādīti kreisajā pusē, un avota kodu tiek parādīti labajā pusē
- Dokumentācija un pirmkods atbalsta īsinājumtaustiņus Rediģēt un Kods, lai pārietu uz avota adresi.
- Virsraksts un ievads tiek parādīti augšpusē, atbalstot i18n automātisko tulkojumu.

## Izmantojiet demonstrāciju

```html
<c-webc-js src="/src/js/dom.js" href="/doc/{code}/js/dom.md"></c-webc-js>
```

## Īpašuma parametri

- `src`: avota koda URL adrese.
- `href`: dokumenta atzīmes URL adrese, atbalsta `{code}` viettura izmantošanu, lai pielāgotos pašreizējai valodai.