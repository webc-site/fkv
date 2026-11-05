# Automatski prebacite navigacijski meni i dugme za povratak

- Automatski omotajte podređene elemente da biste generirali navigacijski meni i strukture gumba za povratak
- Automatski pratite istoriju rutiranja i izračunajte prethodnu povratnu putanju
- Prikažite navigacioni meni kada se ruta poklapa sa stavkom menija, u suprotnom prikažite dugme za povratak

## Koristite demo

```html
<c-nav-l>
  <a href="/">naslovna strana</a>
  <a href="/doc">dokument</a>
</c-nav-l>
```

## stilska klasa

- `B`: aktivira stanje dugmeta za povratak. Prikažite dugme za povratak i sakrijte navigacioni meni. Automatski se prebacuje unutar komponente na osnovu rutiranja.