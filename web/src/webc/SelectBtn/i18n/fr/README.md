# Combinaison d'une zone de sélection déroulante et d'un bouton

- Collez la zone de sélection et le bouton horizontalement
- Enregistrer automatiquement l'état sélectionné dans localStorage
- Interceptez les clics sur les boutons et envoyez des événements personnalisés

## Utilisez la démo

```html
<c-select-btn key="my-select-key" value="option1">
  <c-select>
    <option value="option1">Option 1</option>
    <option value="option2">Option 2</option>
  </c-select>
  <button>courir</button>
</c-select-btn>

<script type="module">
  import "webc.site/SelectBtn.js";
  const el = document.querySelector("c-select-btn");
  el.addEventListener("submit", (e) => {
    console.log(e.value);
  });
</script>
```

## propriété

- `key` : nom de clé utilisé pour la persistance de localStorage
- `value` : valeur actuellement sélectionnée

## événement

- `submit` : distribué lorsque le bouton est cliqué, `e.value` porte la valeur actuellement sélectionnée