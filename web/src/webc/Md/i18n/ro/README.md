# Analizați și redați în siguranță textul Markdown în HTML.

- **Dezinfectarea securității**: Apărați-vă împotriva atacurilor XSS folosind API-ul nativ `setHTML` sau produse de curățare `DOMParser`
- **Extragerea conținutului**: Citiți automat conținutul text al slotului implicit în timpul inițializării și redați-l
- **Cometare încorporată**: stiluri de listă prestabilită, tabel, bloc de coduri, citate și casete de avertizare

## Utilizați demonstrația

### randare statica

```html
<c-md>
# titlu
Aceasta este o bucată de conținut Markdown
</c-md>
```

### atribuire dinamică

```html
<c-md id="md"></c-md>

<script type="module">
import "webc.site/Md.js";
document.querySelector("#md").value = "#Titlu nou\nConținut nou";
</script>
```

## Parametrii interfeței

Introduceți textul Markdown direct în etichetă (ca spațiu prestabilit) sau actualizați dinamic prin `value`.

## Descrierea stilului

Clasa de stil `.Md` este adăugată automat când componenta este inițializată. Acceptă următorul aspect al casetei de avertizare extinsă Markdown (declanșat de sintaxa de referință):

- `[!NOTE]`：Informații prompte
- `[!TIP]`: Sugestii/Sfaturi
- `[!IMPORTANT]`: atenție importantă
- `[!WARNING]`: avertisment de risc
- `[!CAUTION]`: avertisment de risc extrem de ridicat

## mecanism de securitate

La randare, apelați mai întâi API-ul nativ `setHTML` pentru a injecta HTML în siguranță. Dacă browserul nu îl acceptă, va trece automat la un program de curățare recursiv bazat pe `DOMParser` și va implementa următoarea politică de securitate:
- Eliminați etichetele nesigure (de ex., `script`, `iframe` etc.)
- Ștergeți `on*` atributele evenimentului
- Apărați-vă împotriva atacurilor XSS prin filtrarea atributelor care încep cu protocoale periculoase, cum ar fi `javascript:`, `vbscript:` și `data:` (cu excepția `data:image/`).