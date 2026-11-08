# Meniu de navigare adaptat la diferite dimensiuni de ecran

- Desktop: Linkurile sunt afișate în plăci orizontale
- Versiunea mobilă: restrângeți butonul de meniu, faceți clic pentru a glisa în afara sertarului barei laterale
- Activarea rutei: comparați automat calea curentă și valoarea hash și adăugați numele clasei de stare de activare la linkul corespunzător `A`
- Închidere automată: când terminalul mobil face clic pe un link sau pe fundalul extern al barei laterale, bara laterală se închide automat.

## utilizare

```html
<c-nav-menu>
  <a href="/">prima pagină</a>
  <a href="/doc">document</a>
  <a href="/about">despre</a>
</c-nav-menu>
```

## ilustra

Când componenta este inițializată, dacă nu există `c-side` în elementul secundar, butonul de meniu mobil și bara laterală `c-side` vor fi create automat, iar toate etichetele `a` vor fi mutate în bara laterală.

## clasa de stil

- **A**: linkul activat va adăuga automat numele clasei `A`