# Analysez et restituez en toute sécurité le texte Markdown au format HTML.

- **Décontamination de la sécurité** : défendez-vous contre les attaques XSS à l'aide de l'`setHTML` API native ou des `DOMParser` nettoyeurs
- **Extraction de contenu** : lit automatiquement le contenu textuel de l'emplacement par défaut lors de l'initialisation et le restitue
- **Composition intégrée** : styles de liste prédéfinie, tableau, bloc de code, citation et boîte d'avertissement

## Utilisez la démo

### rendu statique

```html
<c-md>
# titre
Ceci est un morceau de contenu Markdown
</c-md>
```

### affectation dynamique

```html
<c-md id="md"></c-md>

<script type="module">
import "webc.site/Md.js";
document.querySelector("#md").value = "#Nouveau titre\nNouveau contenu";
</script>
```

## Paramètres d'interface

Transmettez le texte Markdown directement dans la balise (comme emplacement par défaut) ou mettez à jour dynamiquement via `value`.

## Description du style

La classe de style `.Md` est automatiquement ajoutée lors de l'initialisation du composant. Prend en charge la disposition de boîte d'avertissement étendue Markdown suivante (déclenchée par la syntaxe de référence) :

- `[!NOTE]`：Informations rapides
- `[!TIP]` : Suggestions/Conseils
- `[!IMPORTANT]` : Attention importante
- `[!WARNING]` : avertissement de risque
- `[!CAUTION]` : avertissement de risque extrêmement élevé

## mécanisme de sécurité

Lors du rendu, appelez d'abord l'API `setHTML` native pour injecter du HTML en toute sécurité. Si le navigateur ne le prend pas en charge, il rétrogradera automatiquement vers un nettoyeur récursif basé sur `DOMParser` et mettra en œuvre la politique de sécurité suivante :
- Supprimez les balises non sécurisées (par exemple `script`, `iframe`, etc.)
- Effacer les attributs d'événement `on*`
- Défendez-vous contre les attaques XSS en filtrant les attributs commençant par les protocoles dangereux tels que `javascript:`, `vbscript:` et `data:` (sauf `data:image/`).