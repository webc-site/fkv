# Copier le texte dans le presse-papiers

Cliquez sur le bouton pour copier le texte de l'élément spécifié.

- Prend en charge l'utilisation de rel pour spécifier le sélecteur d'élément à copier
- Lorsqu'il n'est pas spécifié, l'élément frère précédent est copié par défaut.
- Boîte d'invite Pophover imbriquée
- Soutien international

## Utilisez la démo

```html
<!-- Copier des éléments spécifiques -->
<span id="target">Voici le texte à copier</span>
<c-copy rel="#target"></c-copy>

<!-- Les éléments précédents sont copiés par défaut -->
<span>le texte de l'élément précédent</span>
<c-copy></c-copy>
```

## paramètre

| propriété | illustrer |
| --- | --- |
| rel | Copiez le sélecteur de l'élément cible. Facultatif |

## style de statut

- `c-pophover.Done` : style de zone d'invite après une copie réussie