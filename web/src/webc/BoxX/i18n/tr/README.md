# Kapat düğmesiyle açılır bir katman oluşturun ve görüntüleyin

- Açılır katman oluşturmak ve görüntülemek için işlevsel çağrı
- Yerleşik kapatma düğmesi, öğeyi DOM'dan kapatmak ve kaldırmak için tıklayın
- Giriş kutusunun odağını değiştirmek veya açılır katmanı kapatmak için Esc tuşuna basın

## Demoyu kullanın

```html
<button>Açılmak için tıklayın</button>

<script type="module">
import BoxX from "webc.site/BoxX.js";

document.querySelector("button").onclick = () => {
  const dialog = BoxX(),
    main = document.createElement("main"),
    h3 = document.createElement("h3");
  main.classList.add("Lg");
  h3.textContent = "metin";
  main.appendChild(h3);
  dialog.appendChild(main);
};
</script>
```


## Arayüz parametreleri

Varsayılan dışa aktarma işlevi `HTMLDialogElement` değerini döndürür ve `Box` ve `X` stil sınıfları diyalog öğesine eklenmiştir.

## stil sınıfı

### `.Box.X`

`dialog` öğesine uygulanan diyalog stilini belirtir.

### `a.X`

`a` öğesine uygulanan kapat düğmesi stilini belirtir.