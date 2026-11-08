# Açılır katman oluşturma ve görüntüleme

- Açılır katman oluşturmak ve görüntülemek için işlevsel çağrı
- Kapatıldığında DOM'daki öğeleri otomatik olarak kaldır
- Esc anahtarı iptal etme davranışını önleyin

## Demoyu kullanın

```html
<button>Açılmak için tıklayın</button>

<script type="module">
import Box from "webc.site/Box.js";

document.querySelector("button").onclick = () => {
  const dialog = Box(),
    content = document.createElement("div"),
    title = document.createElement("h3"),
    description = document.createElement("p"),
    button = document.createElement("button");

  content.className = "Lg";
  title.textContent = "Başlık";
  açıklama.textContent = "Metin";
  button.className = "Btn Main";
  butonu.textContent = "Tamam";
  button.onclick = () => dialog.close();
  content.append(title, description, button);
  dialog.append(content);
};
</script>
```

## Arayüz parametreleri

### Varsayılan dışa aktarılan işlevler

`Box()`

- **Dönüş değeri**: `HTMLDialogElement`, `Box` stil sınıfının iletişim öğesi eklendi.

## stil sınıfı

### `.Box`

Konumlandırmayı, ortalamayı ve arka plan maskesi bulanıklığını ayarlamak için `dialog` öğesine uygulanır.

### `.Lg`

İç içeriğe uygulanan, arka plan rengini, yuvarlatılmış köşeleri, gölgeyi ve dikey düzeni ayarlayan ve iç `h3` ve `p` düzenini tanımlayan bir stil sınıfı.