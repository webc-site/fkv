# Navigacijski meni prilagojen različnim velikostim zaslona

- Namizje: povezave so prikazane v vodoravnih ploščicah
- Mobilna različica: gumb Strni v meni, kliknite, da izvlečete predal stranske vrstice
- Aktivacija poti: samodejno primerjajte trenutno pot in zgoščeno vrednost ter dodajte ime razreda statusa aktivacije ustrezni povezavi `A`
- Samodejno zapri: Ko mobilni terminal klikne povezavo ali zunanje ozadje stranske vrstice, se stranska vrstica samodejno zapre.

## uporaba

```html
<c-nav-menu>
  <a href="/">sprednja stran</a>
  <a href="/doc">dokument</a>
  <a href="/about">približno</a>
</c-nav-menu>
```

## ilustrirati

Ko je komponenta inicializirana, če v podrejenem elementu ni `c-side`, bosta gumb mobilnega menija in stranska vrstica `c-side` samodejno ustvarjena, vse oznake `a` pa bodo premaknjene v stransko vrstico.

## stilski razred

- **A**: Aktivirana povezava bo samodejno dodala ime razreda `A`