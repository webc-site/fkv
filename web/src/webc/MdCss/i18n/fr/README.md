# Styles de typographie optimisés pour l'expérience de lecture

- **Composition de base** : Adaptez-vous aux titres, aux paragraphes, aux caractères gras, italiques, barrés, au code en ligne, aux images et aux lignes de séparation.
- **Animation de lien** : Afficher l'animation de soulignement lors du survol.
- **Liste de projets** : la liste non ordonnée utilise des puces, la liste ordonnée utilise des numéros de série en arrière-plan arrondis et prend en charge l'imbrication à plusieurs niveaux.
- **Liste des tâches** : fournit un style de case à cocher, prend en charge l'état des tâches et terminé.
- **Conseils d'avertissement** : prend en charge cinq types de boîtes d'avertissement : Remarque, Conseil, Important, Avertissement et Attention.
- **Tableaux et code** : fournit une mise en surbrillance des lignes du tableau et des blocs de code avec des ombres et des arrière-plans dégradés.

## Utilisez la démo

```html
<div class="Md">
  <h1>titre</h1>
  <p>Texte du paragraphe et <a href="#">lien</a></p>
  <hr>
  <h2>Titre de deuxième niveau</h2>
  <ul><li><input type="checkbox" checked="" disabled=""> Élément de tâche</li></ul>
  <blockquote class="q note">
    <b class="qt"><i class="qi"></i>`<p>Noter le contenu</p>
  </blockquote>
</div>
```

## cours de style

Les styles sont appliqués aux éléments HTML sous le conteneur `.Md`.

### Style de boîte d'alerte

Ajoutez le nom de classe suivant sur l'élément `blockquote` avec le nom de classe `.q` pour activer/désactiver la zone d'alerte :

- `note` : remarques
- `tip` : astuce
- `important` : Important
- `warning` : avertissement
- `caution` : Remarque