# Onglets commutables

- L'état sélectionné de l'étiquette est parfaitement intégré à la zone de contenu ci-dessous.
- Spécifications structurelles : les éléments enfants sont fixés pour utiliser `nav > a` comme navigation par étiquette, et l'élément enfant direct `b` est utilisé comme zone de contenu.
- Associez automatiquement la balise `a[value]` au nœud de contenu `b[slot]`.
- Prend en charge la persistance automatique de la page active actuelle sur `localStorage` via l'attribut `key`.
- Prend en charge la spécification de la page à onglet activée via l'attribut `value` (s'il n'y a pas de cache et `value`, la balise avec `class="A"` ou la première balise sera sélectionnée par défaut).
- Implémentation légère de Light DOM, aucune obstruction Shadow DOM, haut degré de liberté dans la personnalisation du style.

## Utilisez la démo

```html
<c-tab key="my-tab-key" value="banana">
  <nav>
    <a value="apple">pomme</a>
    <a value="banana">banane</a>
    <a value="orange">Tangerine</a>
  </nav>
  <b slot="apple">La pomme est un fruit nutritif</b>
  <b slot="banana">La banane est un fruit tropical</b>
  <b slot="orange">Les oranges sont riches en vitamine C</b>
</c-tab>
```

## propriété

- `key` : nom de clé utilisé pour la persistance de localStorage
- `value` : valeur actuellement sélectionnée

## Événements personnalisés

- `change` : Déclenché lorsque l'onglet est changé, `e.value` est la valeur `value` de la balise actuellement activée.