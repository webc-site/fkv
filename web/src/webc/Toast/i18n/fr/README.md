# Fenêtre contextuelle d'invite au bord de la page

- **Appel de fonction** : fournit des interfaces d'invite normales et d'erreur
- **Pile automatique** : plusieurs pointes calculent automatiquement l'espacement et s'empilent verticalement
- **Arrêt programmé et manuel** : prend en charge l'arrêt programmé et manuel

## Utilisez la démo

```js
import { toast, toastErr } from "webc.site/js/Toast.js";

toast((el) => {
  el.textContent = "Contenu de l'invite";
});

toastErr((el) => {
  el.textContent = "Contenu d'erreur";
});

toast(
  (el) => {
    el.textContent = "Fermer automatiquement après 99 secondes";
  },
  99
);
```

## Paramètres d'interface

### toast(render, timeout)

Créez et affichez des invites.

- `render` : `(el) => void`, rappel de rendu, `el` est l'élément de boîte d'invite.
- `timeout` : `Number`, secondes d'expiration. Par défaut 9. Réglez sur 0 pour ne pas s'arrêter automatiquement.
- Valeur de retour : élément de boîte d'invite. Prend en charge `el.close()` désactivé.

### toastErr(render, timeout)

Créez et affichez des messages d'erreur. Les paramètres sont les mêmes que ceux de `toast`, avec la classe de style `.ERR`.

## cours de style

- `.Toast` : classe de base de boîte d'invite.
- `.ERR` : état d'erreur.
- `.animated` / `.fadeInR` / `.fadeOutR` : transition animée.
- `.x` : bouton Fermer.