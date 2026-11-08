# invite de bulle flottante

Afficher les informations de l'info-bulle de survol lorsque la souris est survolée ou que l'élément est activé.

- Basé sur un Composant Web natif, léger et performant
- Prend en charge le positionnement spatial adaptatif dans quatre directions (class="top/bottom/left/right")
- Il utilise un fond dégradé de texture de verre dépoli noir et des caractères blancs, avec des reflets de contour physiques exquis.
- Calcul adaptatif de position intégré, ajuste automatiquement la direction du pop-up en fonction de l'espace restant dans la fenêtre d'affichage
- Utilisez `slot="pop"` pour afficher le contenu de l'invite flottante

## Utilisez la démo

```html
<!-- Calculer automatiquement la direction (recommandé, pas besoin de déclarer la classe) -->
<c-pophover>
  <button>automatique</button>
  <div slot="pop">Ceci est une astuce à bulles</div>
</c-pophover>

<!-- direction fixe -->
<c-pophover class="left">
  <button>Gauche</button>
  <div slot="pop">Ceci est une astuce à bulles</div>
</c-pophover>

<c-pophover class="right">
  <button>droite</button>
  <div slot="pop">Ceci est une astuce à bulles</div>
</c-pophover>

<c-pophover class="bottom">
  <button>Vers le bas</button>
  <div slot="pop">Ceci est une astuce à bulles</div>
</c-pophover>

<c-pophover class="top">
  <button>supérieur</button>
  <div slot="pop">Ceci est une astuce à bulles</div>
</c-pophover>
```

## Variables CSS

Les styles personnalisés sont pris en charge via les variables CSS suivantes :

| nom de la variable | valeur par défaut | illustrer |
| :--- | :--- | :--- |
| `--pophover-gap` | `8px` | L'espace entre l'info-bulle de la bulle et l'élément déclencheur |
| `--pophover-margin` | `24px` | Marge de sécurité du bord de la fenêtre lors du calcul du positionnement adaptatif |
| `--pophover-bg-top` | `#18181ce0` | Couleur supérieure de fond dégradé de bulles |
| `--pophover-bg-mid` | `#121215e5` | La couleur de fond des flèches sur les côtés gauche et droit de la bulle |
| `--pophover-bg-bottom` | `#0c0c0feb` | La couleur des flèches latérales inférieure et supérieure et inférieure du fond dégradé de bulles |

## Style d'info-bulle de l'icône (`.Ico`)

Pour les boutons d'invite flottants de type icône pure (tels que « Modifier », « Supprimer », etc.), vous pouvez appliquer une icône unifiée et un style de survol en ajoutant le nom de classe `.Ico` au conteneur externe ou au composant lui-même.

### Exemple de structure

```html
<b class="Edit Ico">
  <c-pophover>
    <a></a>
    <div slot="pop">modifier</div>
  </c-pophover>
</b>
```

La mise en page de base, la taille (18 x 18 px) et les modifications de couleur de survol ont été unifiées dans `.Ico > c-pophover > a`. Les développeurs doivent uniquement spécifier le fichier d'icône correspondant séparément, tel que :
```css
.Edit > c-pophover > a {
  mask-image: url("./svg/edit.svg")
}
```