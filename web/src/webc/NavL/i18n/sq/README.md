# Ndërroni automatikisht menunë e navigimit dhe butonin e kthimit

- Përfundoni automatikisht elementët fëmijë për të gjeneruar strukturat e menusë së navigimit dhe butonit të kthimit
- Gjurmoni automatikisht historinë e rrugëtimit dhe llogarisni rrugën e mëparshme të kthimit
- Shfaq menynë e navigimit kur itinerari përputhet me artikullin e menysë, përndryshe shfaq butonin e kthimit

## Përdorni demonstrimin

```html
<c-nav-l>
  <a href="/">faqe e parë</a>
  <a href="/doc">dokument</a>
</c-nav-l>
```

## klasë stili

- `B`: aktivizon gjendjen e butonit të kthimit. Shfaqni butonin prapa dhe fshihni menunë e navigimit. Kalohet automatikisht brenda komponentit bazuar në rrugëzimin.