# ಪಾಪ್ಅಪ್ ಪದರವನ್ನು ರಚಿಸಿ ಮತ್ತು ಪ್ರದರ್ಶಿಸಿ

- ಪಾಪ್ಅಪ್ ಲೇಯರ್ ರಚಿಸಲು ಮತ್ತು ಪ್ರದರ್ಶಿಸಲು ಕ್ರಿಯಾತ್ಮಕ ಕರೆ
- ಮುಚ್ಚಿದಾಗ DOM ನಿಂದ ಅಂಶಗಳನ್ನು ಸ್ವಯಂಚಾಲಿತವಾಗಿ ತೆಗೆದುಹಾಕಿ
- Esc ಕೀ ರದ್ದತಿ ನಡವಳಿಕೆಯನ್ನು ತಡೆಯಿರಿ

## ಡೆಮೊ ಬಳಸಿ

```html
<button>ಪಾಪ್ ಅಪ್ ಮಾಡಲು ಕ್ಲಿಕ್ ಮಾಡಿ</button>

<script type="module">
import Box from "webc.site/Box.js";

document.querySelector("button").onclick = () => {
  const dialog = Box(),
    content = document.createElement("div"),
    title = document.createElement("h3"),
    description = document.createElement("p"),
    button = document.createElement("button");

  content.className = "Lg";
  title.textContent = "ಶೀರ್ಷಿಕೆ";
  description.textContent = "ಪಠ್ಯ";
  button.className = "Btn Main";
  button.textContent = "ಸರಿ";
  button.onclick = () => dialog.close();
  content.append(title, description, button);
  dialog.append(content);
};
</script>
```

## ಇಂಟರ್ಫೇಸ್ ನಿಯತಾಂಕಗಳು

### ಡೀಫಾಲ್ಟ್ ರಫ್ತು ಮಾಡಲಾದ ಕಾರ್ಯಗಳು

`Box()`

- **ಹಿಂತಿರುಗಿ ಮೌಲ್ಯ**: `HTMLDialogElement`, `Box` ಶೈಲಿಯ ವರ್ಗದ ಡೈಲಾಗ್ ಅಂಶವನ್ನು ಸೇರಿಸಲಾಗಿದೆ.

## ಶೈಲಿ ವರ್ಗ

### `.Box`

ಸ್ಥಾನೀಕರಣ, ಕೇಂದ್ರೀಕರಣ ಮತ್ತು ಹಿನ್ನೆಲೆ ಮಾಸ್ಕ್ ಬ್ಲರ್ ಅನ್ನು ಹೊಂದಿಸಲು `dialog` ಅಂಶಕ್ಕೆ ಅನ್ವಯಿಸುತ್ತದೆ.

### `.Lg`

ಒಳಗಿನ ವಿಷಯಕ್ಕೆ ಅನ್ವಯಿಸಲಾದ ಶೈಲಿಯ ವರ್ಗ, ಹಿನ್ನೆಲೆ ಬಣ್ಣ, ದುಂಡಾದ ಮೂಲೆಗಳು, ನೆರಳು ಮತ್ತು ಲಂಬ ವಿನ್ಯಾಸವನ್ನು ಹೊಂದಿಸುವುದು ಮತ್ತು ಒಳಗಿನ `h3` ಮತ್ತು `p` ವಿನ್ಯಾಸವನ್ನು ವ್ಯಾಖ್ಯಾನಿಸುವುದು.