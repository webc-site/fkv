# missatge de bombolla flotant

Mostra informació d'eines sobre el ratolí quan es passa el ratolí o l'element està activat.

- Basat en component web natiu, lleuger i d'alt rendiment
- Admet el posicionament espacial adaptatiu en quatre direccions (class="top/bottom/left/right")
- Utilitza un fons degradat de textura de vidre esmerilat negre i caràcters blancs, amb uns contorns físics exquisits.
- Càlcul adaptatiu de posició integrat, ajusta automàticament la direcció emergent segons l'espai restant a la finestra gràfica
- Utilitzeu `slot="pop"` per mostrar contingut d'indicacions flotants

## Utilitzeu la demostració

```html
<!-- Calcula automàticament la direcció (recomanat, no cal declarar la classe) -->
<c-pophover>
  <button>automàtic</button>
  <div slot="pop">Aquesta és una punta de bombolla</div>
</c-pophover>

<!-- direcció fixa -->
<c-pophover class="left">
  <button>Esquerra</button>
  <div slot="pop">Aquesta és una punta de bombolla</div>
</c-pophover>

<c-pophover class="right">
  <button>dret</button>
  <div slot="pop">Aquesta és una punta de bombolla</div>
</c-pophover>

<c-pophover class="bottom">
  <button>Avall</button>
  <div slot="pop">Aquesta és una punta de bombolla</div>
</c-pophover>

<c-pophover class="top">
  <button>superior</button>
  <div slot="pop">Aquesta és una punta de bombolla</div>
</c-pophover>
```

## Variables CSS

Els estils personalitzats són compatibles amb les variables CSS següents:

| nom de la variable | valor per defecte | il·lustrar |
| :--- | :--- | :--- |
| `--pophover-gap` | `8px` | L'espai entre la informació d'eina de la bombolla i l'element activador |
| `--pophover-margin` | `24px` | Marge de seguretat de la vora de la finestra durant el càlcul de posicionament adaptatiu |
| `--pophover-bg-top` | `#18181ce0` | Color superior de fons degradat de bombolles |
| `--pophover-bg-mid` | `#121215e5` | El color de fons de les fletxes dels costats esquerre i dret de la bombolla |
| `--pophover-bg-bottom` | `#0c0c0feb` | El color de la part inferior i de les fletxes laterals superior i inferior del fons degradat de la bombolla |

## Estil d'informació eines d'icona (`.Ico`)

Per als botons d'indicació flotant de tipus d'icona pura (com ara "Edita", "Suprimeix", etc.), podeu aplicar una icona unificada i un estil de desplaçament del cursor afegint el nom de classe `.Ico` al contenidor o component exterior.

### Exemple d'estructura

```html
<b class="Edit Ico">
  <c-pophover>
    <a></a>
    <div slot="pop">editar</div>
  </c-pophover>
</b>
```

El disseny bàsic, la mida (18 x 18 px) i els canvis de color del cursor s'han unificat a `.Ico > c-pophover > a`. Els desenvolupadors només han d'especificar el fitxer d'icona corresponent per separat, com ara:
```css
.Edit > c-pophover > a {
  mask-image: url("./svg/edit.svg")
}
```