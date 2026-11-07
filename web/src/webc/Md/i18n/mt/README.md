# Analiżi u tirrendi b'mod sikur it-test ta' Markdown għal HTML.

- **Sanitizzazzjoni tas-Sigurtà**: Iddefendi kontra attakki XSS billi tuża `setHTML` API jew `DOMParser` tindif nattiv
- ** Estrazzjoni tal-Kontenut **: Aqra awtomatikament il-kontenut tat-test tas-slot default waqt l-inizjalizzazzjoni u agħtih
- **Tiposettjar inkorporat**: lista ssettjata minn qabel, tabella, blokka tal-kodiċi, stili ta’ kwotazzjoni u kaxxa ta’ twissija

## Uża d-demo

### rendering statiku

```html
<c-md>
# titolu
Din hija biċċa kontenut ta' Markdown
</c-md>
```

### assenjazzjoni dinamika

```html
<c-md id="md"></c-md>

<script type="module">
import "webc.site/Md.js";
document.querySelector("#md").value = "#Titolu ġdid\nKontenut ġdid";
</script>
```

## Parametri tal-interface

Għaddi t-test Markdown direttament fit-tikketta (bħala slot default), jew aġġorna b'mod dinamiku permezz ta' `value`.

## Deskrizzjoni tal-istil

Il-klassi tal-istil `.Md` tiżdied awtomatikament meta l-komponent jiġi inizjalizzat. Jappoġġja t-tqassim tal-kaxxa ta' twissija estiża ta' Markdown li ġej (ikkaġunata minn sintassi ta' referenza):

- `[!NOTE]`：Informazzjoni fil-pront
- `[!TIP]`: Suġġerimenti/Suġġerimenti
- `[!IMPORTANT]`: Attenzjoni importanti
- `[!WARNING]`: Twissija ta' riskju
- `[!CAUTION]`: Twissija ta' riskju estremament għoli

## mekkaniżmu tas-sigurtà

Meta tirrendi, l-ewwel ċempel lill-API nattiva `setHTML` biex tinjetta HTML b'mod sikur. Jekk il-brawżer ma jappoġġjahx, awtomatikament se jiddegrada għal cleaner rikorsiv ibbażat fuq `DOMParser` u jimplimenta l-politika ta' sigurtà li ġejja:
- Neħħi tikketti mhux sikuri (eż. `script`, `iframe`, eċċ.)
- Ċar `on*` attributi tal-avveniment
- Iddefendi kontra attakki XSS billi tiffiltra attributi li jibdew bi protokolli perikolużi bħal `javascript:`, `vbscript:`, u `data:` (ħlief `data:image/`).