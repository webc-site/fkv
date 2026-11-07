# Treguesi i statusit të ngarkimit ose përpunimit

- Unifikoni kursorin në gjendje pritjeje
- Shfaq animacionin me valëzim për të treguar statusin e përpunimit
- Aplikoni një maskë turbullimi në një formular për të parandaluar paraqitjet e dyfishta

## Përdorni demonstrimin

```html
<!-- tregues i pavarur i ngarkimit -->
<div class="Ing"></div>

<!-- Ngarkimi i formularit me maskë -->
<form class="Ing">
  <input type="text" placeholder="Kutia e hyrjes">
  <button type="submit">dorëzojë</button>
</form>
```

## klasë stili

### `.Ing`
- Statusi i treguesit është `wait`.
- Pseudo elementi `::before` jep një animacion të valëzuar.
- Kur përdoret në elementin `form`, pseudoelementi `::after` jep një maskë turbullimi 1px, duke çaktivizuar ndërveprimin dhe përzgjedhjen e tekstit.