# Comută automat meniul de navigare și butonul de întoarcere

- Înfășurați automat elementele copil pentru a genera meniul de navigare și structurile butoanelor de întoarcere
- Urmăriți automat istoricul de rutare și calculați calea de întoarcere anterioară
- Afișați meniul de navigare când traseul se potrivește cu elementul de meniu, în caz contrar afișați butonul înapoi

## Utilizați demonstrația

```html
<c-nav-l>
  <a href="/">prima pagină</a>
  <a href="/doc">document</a>
</c-nav-l>
```

## clasa de stil

- `B`: activează starea butonului de întoarcere. Afișează butonul Înapoi și ascunde meniul de navigare. Comutată automat în cadrul componentei în funcție de rutare.