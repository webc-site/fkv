# Automatiškai perjungti naršymo meniu ir grįžimo mygtuką

- Automatiškai apvyniokite antrinius elementus, kad sukurtumėte naršymo meniu ir grąžinimo mygtukų struktūras
- Automatiškai sekti maršruto istoriją ir apskaičiuoti ankstesnį grįžimo kelią
- Rodyti navigacijos meniu, kai maršrutas sutampa su meniu punktu, kitu atveju rodyti atgal mygtuką

## Naudokite demonstracinę versiją

```html
<c-nav-l>
  <a href="/">pirmas puslapis</a>
  <a href="/doc">dokumentas</a>
</c-nav-l>
```

## stiliaus klasė

- `B`: suaktyvina grįžimo mygtuko būseną. Rodyti atgal mygtuką ir paslėpti naršymo meniu. Automatiškai perjungiamas komponente pagal maršrutą.