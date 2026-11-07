# Fond en verre dépoli et effet de surbrillance des bords

La bordure semi-transparente et l'ombre intérieure créent des reflets en micro-relief.
En tant que dépendance de style de base des composants de bouton et de zone de saisie.

## Utilisez la démo

```html
<main class="Lg">
  <article>
    <h3>titre</h3>
    <pre>Texte du contenu</pre>
  </article>
</main>
```

## cours de style

### `.Lg`
Style de base de la carte. Appliquez un `backdrop-filter` flou, une bordure `border` micro-gaufrée et une `box-shadow` ombre intérieure. Appliquez le filtre `svg/glass.svg#h` au survol.

### `main.Lg`
Le conteneur de blocs principal de la page. Coins arrondis `24px` avec remplissage réactif (`16px` si la largeur est inférieure à `800px`, `24px` sinon).

### `.Btn`, `.BtnC`, `.Input`
Nom de la classe du composant de base. Après avoir introduit ce style, vous obtiendrez automatiquement une disposition flexible, des relations hiérarchiques et des propriétés de base du verre dépoli.