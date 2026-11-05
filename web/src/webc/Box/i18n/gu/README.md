# પોપઅપ લેયર બનાવો અને પ્રદર્શિત કરો

- પોપઅપ લેયર બનાવવા અને પ્રદર્શિત કરવા માટે કાર્યાત્મક કૉલ
- બંધ થવા પર DOMમાંથી ઘટકોને આપમેળે દૂર કરો
- Esc કી રદ કરવાની વર્તણૂકને અટકાવો

## ડેમોનો ઉપયોગ કરો

```html
<button>પોપ અપ કરવા માટે ક્લિક કરો</button>

<script type="module">
import Box from "webc.site/Box.js";

document.querySelector("button").onclick = () => {
  const dialog = Box(),
    content = document.createElement("div"),
    title = document.createElement("h3"),
    description = document.createElement("p"),
    button = document.createElement("button");

  content.className = "Lg";
  title.textContent = "શીર્ષક";
  description.textContent = "ટેક્સ્ટ";
  button.className = "Btn Main";
  button.textContent = "ઠીક";
  button.onclick = () => dialog.close();
  content.append(title, description, button);
  dialog.append(content);
};
</script>
```

## ઇન્ટરફેસ પરિમાણો

### ડિફૉલ્ટ નિકાસ કરેલા કાર્યો

`Box()`

- **વળતર મૂલ્ય**: `HTMLDialogElement`, `Box` શૈલી વર્ગનું સંવાદ ઘટક ઉમેરવામાં આવ્યું છે.

## શૈલી વર્ગ

### `.Box`

પોઝિશનિંગ, સેન્ટરિંગ અને બેકગ્રાઉન્ડ માસ્ક બ્લર સેટ કરવા માટે `dialog` એલિમેન્ટ પર લાગુ થાય છે.

### `.Lg`

આંતરિક સામગ્રી પર લાગુ શૈલી વર્ગ, પૃષ્ઠભૂમિ રંગ, ગોળાકાર ખૂણા, પડછાયા અને વર્ટિકલ લેઆઉટને સેટ કરીને અને આંતરિક `h3` અને `p`ના લેઆઉટને વ્યાખ્યાયિત કરીને.