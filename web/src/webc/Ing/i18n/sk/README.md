# Indikátor stavu načítania alebo spracovania

- Zjednoťte kurzor do stavu čakania
- Zobrazuje animáciu zvlnenia na označenie stavu spracovania
- Použite masku rozmazania na formulár, aby ste zabránili duplicitným odoslaniam

## Použite demo

```html
<!-- nezávislý indikátor zaťaženia -->
<div class="Ing"></div>

<!-- Načítava sa formulár s maskou -->
<form class="Ing">
  <input type="text" placeholder="Vstupné pole">
  <button type="submit">predložiť</button>
</form>
```

## štýlová trieda

### `.Ing`
- Stav ukazovateľa je `wait`.
- Pseudoprvok `::before` vykreslí vlnovú animáciu.
- Pri použití na prvku `form` pseudoprvok `::after` vykreslí masku rozmazania 1 pixel, čím zakáže interakciu a výber textu.