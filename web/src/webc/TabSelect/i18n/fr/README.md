# Page à onglet avec sélection déroulante

- Prend en charge l'intégration d'une zone de sélection déroulante à largeur adaptative (`<c-select>`) dans l'étiquette, qui ne sera affichée que lorsque la page à onglet actuelle est activée.
- Spécification structurelle : l'élément enfant utilise `nav > a` comme navigation par étiquette et l'élément enfant direct `b` sert de zone de contenu.
- Associe automatiquement la balise `a[value]`, `a` à `c-select` (en fonction de l'attribut value du groupe et de la valeur de l'option sélectionnée) et le nœud de contenu `b[slot]`.
- Prend en charge la persistance automatique de l'élément actuellement activé sur `localStorage` via l'attribut `key`.
- Prend en charge l'enregistrement et la restauration automatiques des sous-sélections historiques pour chaque groupe dans `localStorage` à l'aide de `key + '/' + tab`.
- Prend en charge la spécification de l'onglet actif via l'attribut `value`.

## Utilisez la démo

```html
<c-tab-select key="my-tab-select-key" value="language/js">
  <nav>
    <a value="language">
      <span>langue</span>
      <c-select>
        <option value="js">JavaScript</option>
        <option value="rs">Rust</option>
      </c-select>
    </a>
    <a value="os">
      <span>système</span>
      <c-select>
        <option value="mac">macOS</option>
        <option value="win">Windows</option>
      </c-select>
    </a>
  </nav>
  <b slot="language">
    <b slot="js">JavaScript est un langage de script</b>
    <b slot="rs">Rust est un langage de programmation au niveau système</b>
  </b>
  <b slot="os">
    <b slot="mac">macOS est un système d'exploitation graphique basé sur Unix</b>
    <b slot="win">Windows est un système d'exploitation développé par Microsoft</b>
  </b>
</c-tab-select>
```

## propriété

- `key` : nom de clé utilisé pour la persistance de localStorage
- `value` : valeur actuellement sélectionnée

## Événements personnalisés

- `change` : Déclenché lors du changement d'onglet, `e.value` est la valeur `value` de l'option actuellement activée.