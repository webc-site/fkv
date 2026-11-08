# Documentation et code source pour le rendu des composants ou des fonctions d'outils

- Mise en page réactive à deux colonnes, avec les documents affichés à gauche et le code source affiché à droite
- La documentation et le code source prennent en charge les boutons de raccourci Modifier et Code pour accéder à l'adresse source.
- Le titre et l'introduction sont affichés en haut, prenant en charge la traduction automatique i18n.

## Utilisez la démo

```html
<c-webc-js src="/src/js/dom.js" href="/doc/{code}/js/dom.md"></c-webc-js>
```

## Paramètres de propriété

- `src` : Adresse URL du code source.
- `href` : adresse URL de la démarque du document, prend en charge l'utilisation de l'espace réservé `{code}` pour s'adapter à la langue actuelle.