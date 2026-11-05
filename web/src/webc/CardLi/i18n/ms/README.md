# Muatkan dan paparkan senarai kad secara tidak segerak

- **Pemuatan tak segerak**: Dapatkan data melalui fungsi tak segerak
- **Paparan Status**: Menyokong tiga keadaan: memuatkan, data kosong dan kad pemaparan.
- **Reka Letak Fleksibel**: Kad disusun dalam baris

## Gunakan demo

```html
<script type="module">
import CardLi from "webc.site/CardLi.js";

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

const card_li = CardLi(async () => {
  await sleep(2000);
  const data = [];
  for (let i = 1; i <= 13; ++i) {
    data.push(["Tajuk " + i, "Penerangan " + i, "#" + i]);
  }
  return data;
});

document.body.append(card_li);
</script>
```

## Parameter antara muka

### CardLi(loader)

- **pemuat**: `() => Promise<Array<[string, string, string]>>`, fungsi pemuatan data tak segerak. Format setiap subkunci ialah `[nama, pengenalan, pautan]`.

## kelas gaya

- `.CardLi`: elemen luar
- `.Card`: elemen kad
- `.Ing`: Memuatkan elemen animasi