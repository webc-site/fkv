# Débogage du code HTML et JS en ligne

- Affichez une barre de débogage sous le code et fournissez les portails de débogage CodePen et JSFiddle
- Mémoriser automatiquement la plateforme de débogage sélectionnée
- Prend en charge l'injection de rappels via l'attribut `.gen` pour générer dynamiquement du code HTML, CSS et JS
- Identifiez automatiquement les éléments enfants avec le nom de classe `language-js` et exécutez-les en mode JS.

## Utilisez la démo

```html
<c-htm-demo><pre><code class="language-html">
<h1>Démo</h1>
</code></pre></c-htm-demo>

<script type="module">
import "webc.site/HtmDemo.js";

const el = document.querySelector("c-htm-demo");
el.gen = (type, text) => {
  // Retourne [html, css, js]
  return [
    text,
    "h1 { color: red; }",
    "console.log(1)"
  ];
};
</script>
```

## Paramètres d'interface

### Propriétés et méthodes

- `.gen` : fonction de rappel injectée. **Ce rappel doit être injecté pour que la barre de débogage soit affichée. **
  - paramètre:
    - `type` : Type de contenu injecté (`HTML = 1`, `JS = 2`).
    - `text` : texte de code extrait de l'élément enfant.
  - Valeur de retour : `[htm, css, js]`