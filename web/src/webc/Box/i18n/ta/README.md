# பாப்அப் லேயரை உருவாக்கி காட்டவும்

- பாப்அப் லேயரை உருவாக்கி காட்டுவதற்கான செயல்பாட்டு அழைப்பு
- DOM ஐ மூடும்போது தானாக உறுப்புகளை அகற்றவும்
- Esc விசை ரத்து நடத்தையைத் தடுக்கவும்

## டெமோவைப் பயன்படுத்தவும்

```html
<button>பாப் அப் செய்ய கிளிக் செய்யவும்</button>

<script type="module">
import Box from "webc.site/Box.js";

document.querySelector("button").onclick = () => {
  const dialog = Box(),
    content = document.createElement("div"),
    title = document.createElement("h3"),
    description = document.createElement("p"),
    button = document.createElement("button");

  content.className = "Lg";
  title.textContent = "தலைப்பு";
  description.textContent = "உரை";
  button.className = "Btn Main";
  button.textContent = "சரி";
  button.onclick = () => dialog.close();
  content.append(title, description, button);
  dialog.append(content);
};
</script>
```

## இடைமுக அளவுருக்கள்

### இயல்புநிலை ஏற்றுமதி செயல்பாடுகள்

`Box()`

- **திரும்ப மதிப்பு**: `HTMLDialogElement`, `Box` பாணி வகுப்பின் உரையாடல் உறுப்பு சேர்க்கப்பட்டது.

## பாணி வகுப்பு

### `.Box`

பொருத்துதல், மையப்படுத்துதல் மற்றும் பின்னணி முகமூடியின் மங்கலை அமைக்க `dialog` உறுப்புக்கு பொருந்தும்.

### `.Lg`

உட்புற உள்ளடக்கத்திற்குப் பயன்படுத்தப்படும் ஒரு பாணி வகுப்பு, பின்னணி நிறம், வட்டமான மூலைகள், நிழல் மற்றும் செங்குத்து தளவமைப்பு ஆகியவற்றை அமைத்தல் மற்றும் உள் `h3` மற்றும் `p` அமைப்பை வரையறுக்கிறது.