# Sélection déroulante, la largeur s'adapte à l'élément actuel

La largeur du `select` natif est déterminée par défaut par le texte d'option le plus long. Lors du changement d'options courtes, il y a trop d'espace blanc, ce qui affecte l'apparence.

Ce composant couvre de manière transparente le `select` natif au-dessus de la balise `b`, permettant à la largeur de s'adapter avec précision à la longueur du texte actuellement sélectionné.

- Conserver les styles de menu déroulant `select` natifs et la prise en charge de l'accessibilité sur tous les terminaux
- Prend en charge les opérations interactives natives telles que la navigation complète par onglets du clavier et le changement de touche de direction.
- Adaptez la largeur à l'option actuelle pour éliminer l'excès d'espace blanc

## Utilisez la démo

```html
<c-select>
  <option value="js">JavaScript</option>
  <option value="rs">Rust</option>
  <option value="py">Python</option>
</c-select>
```