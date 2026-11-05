# Açılan təbəqə yaradın və göstərin

- Popup təbəqəni yaratmaq və göstərmək üçün funksional çağırış
- Bağlandıqda DOM-dan elementləri avtomatik silin
- Esc açarının ləğvi davranışının qarşısını alın

## Demodan istifadə edin

```html
<button>Açmaq üçün klikləyin</button>

<script type="module">
import Box from "webc.site/Box.js";

document.querySelector("button").onclick = () => {
  const dialog = Box(),
    content = document.createElement("div"),
    title = document.createElement("h3"),
    description = document.createElement("p"),
    button = document.createElement("button");

  content.className = "Lg";
  title.textContent = "Başlıq";
  description.textContent = "Mətn";
  button.className = "Btn Main";
  button.textContent = "OK";
  button.onclick = () => dialog.close();
  content.append(title, description, button);
  dialog.append(content);
};
</script>
```

## İnterfeys parametrləri

### Defolt ixrac edilmiş funksiyalar

`Box()`

- **Qayıt dəyəri**: `HTMLDialogElement`, `Box` üslub sinfinin dialoq elementi əlavə edildi.

## stil sinfi

### `.Box`

Yerləşdirmə, mərkəzləşdirmə və fon maskasının bulanıqlığını təyin etmək üçün `dialog` elementinə tətbiq edilir.

### `.Lg`

Daxili məzmuna tətbiq olunan üslub sinfi, fon rəngini, yuvarlaq küncləri, kölgəni və şaquli düzümü təyin edən və daxili `h3` və `p` düzümünü təyin edir.