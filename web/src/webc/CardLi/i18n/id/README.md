# Memuat dan menampilkan daftar kartu secara asinkron

- **Pemuatan asinkron**: Mendapatkan data melalui fungsi asinkron
- **Tampilan Status**: Mendukung tiga status: memuat, mengosongkan data, dan merender kartu.
- **Tata Letak Fleksibel**: Kartu disusun dalam baris

## Gunakan demonya

```html
<script type="module">
import CardLi from "webc.site/CardLi.js";

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

const card_li = CardLi(async () => {
  await sleep(2000);
  const data = [];
  for (let i = 1; i <= 13; ++i) {
    data.push(["Judul " + i, "Deskripsi " + i, "#" + i]);
  }
  return data;
});

document.body.append(card_li);
</script>
```

## Parameter antarmuka

### CardLi(loader)

- **loader**: `() => Promise<Array<[string, string, string]>>`, fungsi pemuatan data asinkron. Format setiap subkunci adalah `[nama, pendahuluan, tautan]`.

## kelas gaya

- `.CardLi`: elemen luar
- `.Card`: elemen kartu
- `.Ing`: Memuat elemen animasi