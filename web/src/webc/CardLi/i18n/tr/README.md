# Kart listesini eşzamansız olarak yükleme ve görüntüleme

- **Eşzamansız yükleme**: Eşzamansız bir işlev aracılığıyla verileri alın
- **Durum Ekranı**: Üç durumu destekler: yükleme, boş veri ve kart oluşturma.
- **Esnek Düzen**: Kartlar sıralar halinde düzenlenmiştir

## Demoyu kullanın

```html
<script type="module">
import CardLi from "webc.site/CardLi.js";

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

const card_li = CardLi(async () => {
  await sleep(2000);
  const data = [];
  for (let i = 1; i <= 13; ++i) {
    data.push(["Başlık " + i, "Açıklama " + i, "#" + i]);
  }
  return data;
});

document.body.append(card_li);
</script>
```

## Arayüz parametreleri

### CardLi(loader)

- **loader**: `() => Promise<Array<[string, string, string]>>`, eşzamansız veri yükleme işlevi. Her alt anahtarın biçimi şu şekildedir: `[ad, tanıtım, bağlantı]`.

## stil sınıfı

- `.CardLi`: dış öğe
- `.Card`: kart öğesi
- `.Ing`: Animasyonlu öğe yükleniyor