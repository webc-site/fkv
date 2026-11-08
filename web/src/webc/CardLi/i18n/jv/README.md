# Muat lan nampilake dhaptar kertu asynchronously

- **Asynchronous loading**: Entuk data liwat fungsi asinkron
- **Status Display**: Ndhukung telung status: loading, data kosong, lan kertu rendering.
- **Fleksibel Layout**: Kertu disusun ing baris

## Gunakake demo

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

## Paramèter antarmuka

### CardLi(loader)

- **loader**: `() => Promise<Array<[string, string, string]>>`, fungsi muat data asinkron. Format saben subkunci yaiku `[jeneng, introduksi, link]`.

## kelas gaya

- `.CardLi`: unsur njaba
- `.Card`: unsur kertu
- `.Ing`: Ngunggah unsur animasi